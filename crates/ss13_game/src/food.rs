//! Food (server): tg `/obj/item/food` with `/datum/component/edible` (`edible.dm`: `TryToEat`,
//! `TakeBite`, `On_Consume`), tasting (`mob/living/taste.dm`, `reagents/chemistry/taste.dm`),
//! `/datum/element/food_trash` and `/datum/element/processable` (`atom/_atom.dm`
//! `StartProcessingAtom`, edible `created_by_processing`).
//!
//! - Every food item ([`DmFood`]) gets a reagent holder of its `max_volume` (`INJECTABLE`)
//!   with its `food_reagents` at its purity, so chemistry (syringes, examine) sees it.
//! - Out of combat mode a held food clicked on yourself is eaten bite by bite: each bite takes
//!   `eat_time` (a `do_after`), moves `bite_consumption` units into the stomach, tastes them and
//!   starts the next bite; on someone else it is fed after 3 s, one bite. The eater's fullness
//!   decides the message and refuses a bite past `600 * (1 + overeat / 4000 s)`.
//! - An eaten-up food is deleted; its `trash_type` takes its place in the eater's hand.
//! - A tool clicked on food with a matching `processable` (a knife on bread, a rolling pin on
//!   dough) works on it for the element's time (`use_tool`) and replaces it with `amount`
//!   results that split its reagents (`table_required`: on a table or a tray).
// shortcut: no foodtypes, likes/dislikes, food buffs, decomposition, `check_liked`, golem
// materials, eating sounds, voracious/glutton traits or hologram food; the mouth counts as
// covered by anything in the mask slot (as `crate::chem`'s drinking); the radial menu between
// several processing results picks the first.

use bevy::prelude::*;
use ss13_mapload::{DmFood, DmItem, DmObj, DmTool};
use ss13_reagents::metabolism;
use ss13_reagents::{Chemistry, Holder, Owner, flags, is_under};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_shared::protocol::InventorySlot;
use ss13_world::SpatialIndex;

use crate::chem::{Chem, MobReagents, Reagents, name_of, tell_mob, visible};
use crate::combat::{CombatMode, CombatRng};
use crate::hunger::Hunger;
use crate::inventory::{CarriedBy, Inventory};
use crate::tools::{Actor, DoAfter, do_after, tell, use_tool};

pub const FOOD_ROOT: &str = "/obj/item/food";
/// `EAT_TIME_FORCE_FEED`, deciseconds.
pub const EAT_TIME_FORCE_FEED: f32 = 30.0;
/// `DEFAULT_TASTE_SENSITIVITY` (a human tongue's `taste_sensitivity`).
pub const TASTE_SENSITIVITY: f32 = 15.0;
/// `TEXT_NO_TASTE`.
const TEXT_NO_TASTE: &str = "чего-то неописуемого";
/// Reagents with `carry_food_tastes = FALSE`: they never taste of the food they are in.
const NO_FOOD_TASTES: [&str; 2] = [
    "/datum/reagent/consumable/nutriment/fat/oil",
    "/datum/reagent/consumable/nutriment/organ_tissue/stomach_lining",
];
const NUTRIMENT_ROOT: &str = "/datum/reagent/consumable/nutriment";
const TABLE_ROOT: &str = "/obj/structure/table";
const TRAYS: [&str; 2] = ["/obj/item/storage/bag/tray", "/obj/item/plate/oven_tray"];

/// A food item's live edible state (`bitecount`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Food {
    pub bitecount: u32,
}

/// tg `last_taste_time`/`last_taste_text` of a mob.
#[derive(Component, Clone, Debug, Default)]
struct TasteMemory {
    time: f64,
    text: String,
}

/// A timed food action ([`crate::construction::Job::Food`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoodJob {
    /// One bite of the held food, by the eater.
    Bite,
    /// The held food forced on someone else.
    Feed,
    /// The `n`th `processable` of the target food with the held tool.
    Process(u8),
}

/// tg `TRAIT_DRIED` (`/datum/element/dryable/apply_dried_status`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Dried;

/// `COLOR_DRIED_TAN`.
const DRIED_TAN: [u8; 4] = [0xAD, 0x72, 0x57, 0xFF];

/// Dried in place (tg `finish_drying` when the dried type is the item's own): `TRAIT_DRIED` and
/// the tan color.
// shortcut: a plain multiply by the tan instead of `color_transition_filter`.
pub fn dry(world: &mut World, item: Entity) {
    world.entity_mut(item).insert(Dried);
    if let Some(mut look) = world.get_mut::<ss13_shared::Appearance>(item) {
        look.color = Some(DRIED_TAN);
    }
}

pub struct FoodServerPlugin;

impl Plugin for FoodServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(create_food_reagents);
    }
}

pub fn is_food(path: &str) -> bool {
    is_under(path, FOOD_ROOT)
}

/// `/datum/component/edible/setup_initial_reagents`.
fn create_food_reagents(
    add: On<Add<DmFood>>,
    foods: Query<(&DmFood, Option<&DmItem>)>,
    chem: Option<Res<Chem>>,
    mut commands: Commands,
) {
    let Ok((dm, item)) = foods.get(add.entity) else {
        return;
    };
    let owner = Owner {
        path: item.map(|i| i.path.clone()).unwrap_or_default(),
        living: false,
    };
    let mut holder = Holder::new(dm.volume, flags::INJECTABLE, owner);
    if let Some(chem) = chem {
        let list: Vec<(u16, f32)> = dm
            .food_reagents
            .iter()
            .filter_map(|r| chem.0.id(&r.path).map(|id| (id, r.amount)))
            .collect();
        holder.add_reagent_list(&chem.0, &list, Some(dm.purity), true);
    }
    commands
        .entity(add.entity)
        .insert((Reagents(holder), Food::default()));
}

fn food_volume(world: &World, food: Entity) -> f32 {
    world
        .get::<Reagents>(food)
        .map_or(0.0, |r| r.0.total_volume)
}

/// tg `IsFoodGone`.
fn food_gone(world: &World, food: Entity) -> bool {
    world.get::<DmFood>(food).is_none() || food_volume(world, food) <= 0.0
}

/// The eater's `get_fullness()`.
fn fullness(world: &World, eater: Entity, only_consumable: bool) -> f32 {
    let nutrition = world.get::<Hunger>(eater).map_or(0.0, |h| h.nutrition);
    match (
        world.get_resource::<Chem>(),
        world.get::<MobReagents>(eater),
    ) {
        (Some(chem), Some(r)) => {
            metabolism::fullness(&chem.0, nutrition, &r.blood, &r.stomach, only_consumable)
        }
        _ => nutrition,
    }
}

fn overeat_secs(world: &World, eater: Entity) -> f32 {
    world.get::<Hunger>(eater).map_or(0.0, |h| h.overeat_secs)
}

/// tg `item_interaction` of food on a mob (`UseFromHand` -> `TryToEat`) and of a tool on food
/// (`processable`). True when the click was food's.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<CombatMode>(mob).is_some_and(|c| c.0) {
        return false;
    }
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    if world.get::<DmFood>(held).is_some() && world.get::<MobReagents>(target).is_some() {
        try_to_eat(world, actor, target);
        return true;
    }
    let Some(tool) = world.get::<DmTool>(held).map(|t| t.behaviour.clone()) else {
        return false;
    };
    let Some(index) = world
        .get::<DmFood>(target)
        .and_then(|f| f.processing.iter().position(|p| p.tool == tool))
    else {
        return false;
    };
    try_process(world, actor, target, index);
    true
}

/// tg `CanConsume`.
fn can_consume(world: &mut World, actor: Actor, eater: Entity) -> bool {
    if world.get::<MobReagents>(eater).is_none() {
        return false;
    }
    let masked = world
        .get::<Inventory>(eater)
        .and_then(|i| i.slot(InventorySlot::Mask))
        .is_some();
    if masked {
        tell(world, actor.client, Span::Warning, "рот закрыт!");
        return false;
    }
    true
}

/// tg `TryToEat` up to its `do_after`.
fn try_to_eat(world: &mut World, actor: Actor, eater: Entity) {
    let food = actor.item.expect("held food");
    if food_gone(world, food) || !can_consume(world, actor, eater) {
        return;
    }
    if world.get::<DoAfter>(actor.mob).is_some() {
        return;
    }
    let eat_time = world.get::<DmFood>(food).map_or(10.0, |f| f.eat_time);
    if eater == actor.mob {
        do_after(
            world,
            actor,
            eater,
            eat_time,
            0,
            crate::construction::Job::Food(FoodJob::Bite),
        );
        return;
    }
    let fullness = fullness(world, eater, false) + 10.0;
    let (feeder, victim, src) = (
        name_of(world, actor.mob),
        name_of(world, eater),
        name_of(world, food),
    );
    if fullness <= 600.0 * (1.0 + overeat_secs(world, eater) / 2000.0) {
        visible(
            world,
            actor.mob,
            &format!("Вы пытаетесь накормить {victim} с помощью {src}."),
            &format!("{feeder} пытается накормить {victim} с помощью {src}."),
            Span::Danger,
        );
        tell_mob(
            world,
            eater,
            Span::Danger,
            &format!("{feeder} пытается накормить вас с помощью {src}."),
        );
    } else {
        visible(
            world,
            actor.mob,
            &format!("Вы больше не можете протолкнуть {src} в горло {victim}!"),
            &format!("{feeder} больше не может протолкнуть {src} в горло {victim}!"),
            Span::Danger,
        );
        return;
    }
    do_after(
        world,
        actor,
        eater,
        EAT_TIME_FORCE_FEED,
        0,
        crate::construction::Job::Food(FoodJob::Feed),
    );
}

/// `ru_eat_verb` (BandaStation `ru_verbs.toml` `eat_verbs`): second person, or third with `s`.
fn ru_eat_verb(verb: &str, third: bool) -> String {
    let (you, they) = match verb {
        "bite" => ("кусаете", "кусает"),
        "chew" => ("жуёте", "жуёт"),
        "nibble" => ("покусываете", "покусывает"),
        "gnaw" => ("грызёте", "грызёт"),
        "gobble" => ("пожираете", "пожирает"),
        "chomp" => ("лопаете", "лопает"),
        _ if third => return format!("{verb}s"),
        _ => return verb.to_owned(),
    };
    if third { they } else { you }.to_owned()
}

/// The rest of a [`FoodJob`] after its `do_after`, with tg's checks at that moment.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: FoodJob) {
    let actor = Actor {
        client: action.client,
        mob,
        item: action.item,
    };
    match job {
        FoodJob::Bite => self_bite(world, actor),
        FoodJob::Feed => {
            let (Some(food), eater) = (action.item, action.target) else {
                return;
            };
            if food_gone(world, food) || !can_consume(world, actor, eater) {
                return;
            }
            let (feeder, victim, src) = (
                name_of(world, mob),
                name_of(world, eater),
                name_of(world, food),
            );
            visible(
                world,
                mob,
                &format!("Вы заставляете {victim} съесть {src}!"),
                &format!("{feeder} заставляет {victim} съесть {src}!"),
                Span::Danger,
            );
            tell_mob(
                world,
                eater,
                Span::Danger,
                &format!("{feeder} заставляет вас съесть {src}!"),
            );
            take_bite(world, eater, mob, food);
        }
        FoodJob::Process(index) => finish_process(world, actor, action.target, index as usize),
    }
}

/// tg `TryToEat` after the eater's own `do_after`: the fullness message, the bite and the next
/// bite.
fn self_bite(world: &mut World, actor: Actor) {
    let Some(food) = actor.item else {
        return;
    };
    let eater = actor.mob;
    if food_gone(world, food) || !can_consume(world, actor, eater) {
        return;
    }
    let dm = world
        .get::<DmFood>(food)
        .expect("checked by food_gone")
        .clone();
    let fullness = fullness(world, eater, false) + 10.0;
    let verb = {
        let mut rng = world.resource_mut::<CombatRng>();
        let i = rng.range(0, dm.eatverbs.len().max(1) as u32 - 1) as usize;
        dm.eatverbs.get(i).cloned().unwrap_or_else(|| "bite".into())
    };
    let (you, they) = (ru_eat_verb(&verb, false), ru_eat_verb(&verb, true));
    let (user, src) = (name_of(world, eater), name_of(world, food));
    let (satiety, nutrition) = world
        .get::<Hunger>(eater)
        .map_or((0.0, 0.0), |h| (h.satiety, h.nutrition));
    if dm.junkiness > 0.0
        && satiety < -150.0
        && nutrition > crate::hunger::NUTRITION_LEVEL_STARVING + 50.0
    {
        tell(
            world,
            actor.client,
            Span::Warning,
            "Вам сейчас не хочется есть фастфуд!",
        );
        return;
    }
    let (own, others) = if fullness > 600.0 * (1.0 + overeat_secs(world, eater) / 4000.0) {
        visible(
            world,
            eater,
            &format!("Вы больше не можете протолкнуть {src} себе в горло!"),
            &format!("{user} больше не может протолкнуть {src} себе в горло!"),
            Span::Warning,
        );
        return;
    } else if fullness > 500.0 {
        (
            format!("Вы неохотно {you} часть от {src}."),
            format!("{user} неохотно {they} часть от {src}."),
        )
    } else if fullness > 150.0 {
        (format!("Вы {you} {src}."), format!("{user} {they} {src}."))
    } else if fullness > 50.0 {
        (
            format!("Вы жадно {you} {src}."),
            format!("{user} жадно {they} {src}."),
        )
    } else {
        (
            format!("Вы жадно {you} {src}, проглатывая пищу!"),
            format!("{user} жадно {they} {src}, проглатывая пищу!"),
        )
    };
    visible(world, eater, &own, &others, Span::Notice);
    take_bite(world, eater, eater, food);
    // "If we're not force-feeding and there's an eat delay, try take another bite".
    if dm.eat_time > 0.0 && world.get_entity(food).is_ok() {
        try_to_eat(world, actor, eater);
    }
}

/// tg `TakeBite`: `bite_consumption` units into the eater's stomach, tasted; an emptied food is
/// consumed.
fn take_bite(world: &mut World, eater: Entity, feeder: Entity, food: Entity) {
    let Some(dm) = world.get::<DmFood>(food).cloned() else {
        return;
    };
    if let Some(mut hunger) = world.get_mut::<Hunger>(eater)
        && hunger.satiety > -200.0
    {
        hunger.satiety -= dm.junkiness;
    }
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let Some(mut holder) = world
        .get_mut::<Reagents>(food)
        .map(|mut r| std::mem::replace(&mut r.0, Holder::new(0.0, 0, Owner::default())))
    else {
        return;
    };
    if holder.total_volume <= 0.0 {
        world.entity_mut(food).insert(Reagents(holder));
        return;
    }
    let part = (dm.bite_consumption / holder.total_volume).min(1.0);
    let bite: Vec<(u16, f32)> = holder
        .reagents
        .iter()
        .map(|r| (r.id, r.volume * part))
        .collect();
    // tg `expel_ingested` without a stomach.
    // shortcut: the bite is lost (no vomit decal with the reagents).
    if crate::medical::organs::lacks(world, eater, crate::medical::organs::Slot::Stomach) {
        let mut gone = Holder::new(dm.bite_consumption, 0, Owner::default());
        holder.trans_to(
            &chem,
            &mut gone,
            dm.bite_consumption,
            1.0,
            None,
            false,
            false,
        );
    } else if let Some(mut m) = world.get_mut::<MobReagents>(eater) {
        holder.trans_to(
            &chem,
            &mut m.stomach,
            dm.bite_consumption,
            1.0,
            None,
            false,
            false,
        );
    }
    let left = holder.total_volume;
    world.entity_mut(food).insert(Reagents(holder));
    if let Some(mut f) = world.get_mut::<Food>(food) {
        f.bitecount += 1;
    }
    let text = taste_message(&chem, &bite, &dm.tastes, TASTE_SENSITIVITY);
    taste(world, eater, text);
    if left <= 0.0 {
        on_consume(world, eater, feeder, food, &dm);
    }
}

/// tg `/mob/living/check_tasting_blocks` and `send_taste_message`: at most one taste per 5 s,
/// and the same taste not again within 10 s.
fn taste(world: &mut World, mob: Entity, text: String) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let memory = world
        .get::<TasteMemory>(mob)
        .cloned()
        .unwrap_or(TasteMemory {
            time: f64::NEG_INFINITY,
            text: String::new(),
        });
    if memory.time + 5.0 >= now || (memory.text == text && memory.time + 10.0 >= now) {
        return;
    }
    tell_mob(world, mob, Span::Notice, &format!("Вы чувствуете {text}."));
    world
        .entity_mut(mob)
        .insert(TasteMemory { time: now, text });
}

/// tg `generate_reagents_taste_message` of `bite` (reagent, units) from a food with `tastes`
/// (what its nutriment carries as data).
pub fn taste_message(
    chem: &Chemistry,
    bite: &[(u16, f32)],
    tastes: &[ss13_mapload::Taste],
    threshold: f32,
) -> String {
    let food_total: f32 = tastes.iter().map(|t| t.weight).sum();
    let mut found: Vec<(String, f32)> = Vec::new();
    let mut total = 0.0;
    for &(id, volume) in bite {
        let def = chem.def(id);
        if def.taste_mult == 0.0 {
            continue;
        }
        let carries = is_under(&def.path, NUTRIMENT_ROOT)
            && !NO_FOOD_TASTES.iter().any(|p| is_under(&def.path, p))
            && food_total > 0.0;
        let data: Vec<(&str, f32)> = if carries {
            tastes
                .iter()
                .map(|t| (t.text.as_str(), t.weight / food_total))
                .collect()
        } else {
            vec![(def.taste_description.as_str(), 1.0)]
        };
        for (text, weight) in data {
            let strength = weight * volume * def.taste_mult;
            match found.iter_mut().find(|(t, _)| t == text) {
                Some(entry) => entry.1 += strength,
                None => found.push((text.to_owned(), strength)),
            }
            total += strength;
        }
    }
    if total <= 0.0 {
        return TEXT_NO_TASTE.to_owned();
    }
    if found.len() == 1 {
        return format!("вкус {}", found[0].0);
    }
    // `sortTim(cmp_numeric_dsc)`: stable.
    found.sort_by(|a, b| b.1.total_cmp(&a.1));
    let (mut strong, mut mild, mut weak) = (Vec::new(), Vec::new(), Vec::new());
    for (text, strength) in found {
        let percent = strength / total * 100.0;
        if percent < threshold {
            continue;
        } else if percent <= threshold * 2.0 {
            weak.push(text);
        } else if percent <= threshold * 4.0 {
            mild.push(text);
        } else {
            strong.push(text);
        }
    }
    let mut messages = Vec::new();
    if !strong.is_empty() {
        messages.push(format!("сильный аромат {}", english_list(&strong)));
    }
    if !mild.is_empty() {
        messages.push(format!("вкус {}", english_list(&mild)));
    }
    if !weak.is_empty() {
        messages.push(format!("нотку {}", english_list(&weak)));
    }
    english_list(&messages)
}

/// BandaStation `english_list`: "a", "a и b", "a, b и c"; empty: [`TEXT_NO_TASTE`].
fn english_list(items: &[String]) -> String {
    match items {
        [] => TEXT_NO_TASTE.to_owned(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} и {last}", rest.join(", ")),
    }
}

/// tg `On_Consume` and `/datum/element/food_trash`: the food is gone, its trash goes into the
/// hand that held it.
fn on_consume(world: &mut World, eater: Entity, feeder: Entity, food: Entity, dm: &DmFood) {
    let src = name_of(world, food);
    tell_mob(
        world,
        feeder,
        Span::Warning,
        &format!("Ничего более не осталось от {src}, о нет!"),
    );
    let holder = world.get::<CarriedBy>(food).map(|c| c.0);
    let hand = holder.and_then(|h| {
        let inventory = world.get::<Inventory>(h)?;
        inventory.hands.iter().position(|&i| i == Some(food))
    });
    let pos = crate::chem::map_pos(world, food).or_else(|| world.get::<TilePos>(eater).copied());
    world.despawn(food);
    if dm.trash_type.is_empty() {
        return;
    }
    let Some(pos) = pos else {
        return;
    };
    let Some(trash) = crate::construction::spawn_obj(world, &dm.trash_type, pos) else {
        return;
    };
    if let (Some(holder), Some(hand)) = (holder, hand) {
        crate::inventory::pick_up(world, holder, trash, hand);
    }
}

/// A table or a tray on `pos` (tg `table_required`).
fn on_table(world: &World, pos: TilePos) -> bool {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| {
            world
                .get::<DmObj>(e)
                .is_some_and(|o| is_under(&o.path, TABLE_ROOT))
                || world
                    .get::<DmItem>(e)
                    .is_some_and(|i| TRAYS.iter().any(|t| is_under(&i.path, t)))
        })
}

/// tg `processable/try_process` and `StartProcessingAtom` up to `use_tool`.
fn try_process(world: &mut World, actor: Actor, food: Entity, index: usize) {
    let Some(process) = world
        .get::<DmFood>(food)
        .and_then(|f| f.processing.get(index).cloned())
    else {
        return;
    };
    let Some(&pos) = world.get::<TilePos>(food) else {
        return;
    };
    if world.get::<DoAfter>(actor.mob).is_some() {
        return;
    }
    if process.table_required && !on_table(world, pos) {
        let result = prototype_name(world, &process.result);
        tell(
            world,
            actor.client,
            Span::Notice,
            format!("You cannot make {result} here! You need a table or at least a tray."),
        );
        return;
    }
    let src = name_of(world, food);
    tell(
        world,
        actor.client,
        Span::Notice,
        format!("You start working on {src}."),
    );
    use_tool(
        world,
        actor,
        food,
        process.time,
        0,
        crate::construction::Job::Food(FoodJob::Process(index as u8)),
    );
}

fn prototype_name(world: &World, path: &str) -> String {
    world
        .get_resource::<ss13_proto::Prototypes>()
        .and_then(|p| p.get(path).cloned())
        .and_then(|p| p.get::<ss13_mapload::DmName>().map(|n| n.name.clone()))
        .unwrap_or_else(|| ss13_mapload::convert::default_name(path))
}

/// The end of `StartProcessingAtom`: `amount` results, each with its share of the food's
/// reagents (edible `created_by_processing`), and the food deleted.
fn finish_process(world: &mut World, actor: Actor, food: Entity, index: usize) {
    let Some(process) = world
        .get::<DmFood>(food)
        .and_then(|f| f.processing.get(index).cloned())
    else {
        return;
    };
    let Some(&pos) = world.get::<TilePos>(food) else {
        return;
    };
    if process.table_required && !on_table(world, pos) {
        return;
    }
    let src = name_of(world, food);
    let original = world.get::<Reagents>(food).map(|r| r.0.clone());
    let chem = world.get_resource::<Chem>().map(|c| c.0.clone());
    let amount = process.amount.max(1);
    let mut name = prototype_name(world, &process.result);
    for _ in 0..amount {
        let Some(made) = crate::construction::spawn_obj(world, &process.result, pos) else {
            continue;
        };
        name = name_of(world, made);
        let made_holder = world
            .get::<Reagents>(made)
            .map(|r| (r.0.flags, r.0.owner.clone()));
        if let (Some(original), Some(chem), Some((flags, owner))) = (&original, &chem, made_holder)
        {
            let volume = (original.maximum_volume / amount as f32).ceil();
            let mut holder = Holder::new(volume, flags, owner);
            let share = original.total_volume / amount as f32;
            original
                .clone()
                .trans_to(chem, &mut holder, share, 1.0, None, true, true);
            world.entity_mut(made).insert(Reagents(holder));
        }
    }
    world.despawn(food);
    crate::construction::tile_changed(world, pos);
    let plural = if amount > 1 && !name.ends_with('s') {
        "s"
    } else {
        ""
    };
    tell(
        world,
        actor.client,
        Span::Notice,
        format!("You manage to create {amount} {name}{plural} from {src}."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_reagents::{ChemData, ReagentDef};

    fn reagent(path: &str, taste: &str, mult: f32) -> ReagentDef {
        ReagentDef {
            path: path.into(),
            name: path.rsplit('/').next().unwrap().into(),
            description: String::new(),
            color: "#FFFFFF".into(),
            taste_description: taste.into(),
            taste_mult: mult,
            metabolization_rate: 0.4,
            overdose_threshold: 0.0,
            ph: 7.0,
            specific_heat: 200.0,
            chemical_flags: 0,
            purity: 1.0,
            creation_purity: 1.0,
            inverse_chem: None,
            inverse_chem_val: 0.0,
            purge_multiplier: 1.0,
            toxpwr: 0.0,
            health_required: 0.0,
            liver_tolerance_multiplier: 0.0,
            boozepwr: 0.0,
            brute_heal: 0.0,
            nutriment_factor: 15.0,
        }
    }

    #[test]
    fn tastes_follow_generate_reagents_taste_message() {
        let chem = Chemistry::new(ChemData {
            glass_styles: Vec::new(),
            reagents: vec![
                reagent("/datum/reagent/consumable/nutriment", "generic food", 4.0),
                reagent("/datum/reagent/consumable/salt", "salt", 2.0),
                reagent("/datum/reagent/consumable/nutriment/fat/oil", "oil", 0.8),
            ],
            reactions: vec![],
        });
        let bread = [ss13_mapload::Taste {
            text: "bread".into(),
            weight: 10.0,
        }];
        assert_eq!(
            taste_message(&chem, &[(0, 2.0)], &bread, 15.0),
            "вкус bread"
        );
        assert_eq!(
            taste_message(&chem, &[(0, 2.0)], &[], 15.0),
            "вкус generic food",
            "nutriment without food data tastes of itself"
        );
        // bread 8, salt 2, oil 0.08: bread strong (79%), salt weak (19.8% <= 30), oil too weak.
        assert_eq!(
            taste_message(&chem, &[(0, 2.0), (1, 1.0), (2, 0.1)], &bread, 15.0),
            "сильный аромат bread и нотку salt"
        );
        let mixed = [
            ss13_mapload::Taste {
                text: "umami".into(),
                weight: 2.0,
            },
            ss13_mapload::Taste {
                text: "dough".into(),
                weight: 2.0,
            },
            ss13_mapload::Taste {
                text: "laziness".into(),
                weight: 1.0,
            },
        ];
        // 40%, 40%, 20%: two mild ("<= 60"), one weak ("<= 30").
        assert_eq!(
            taste_message(&chem, &[(0, 1.0)], &mixed, 15.0),
            "вкус umami и dough и нотку laziness"
        );
        assert_eq!(taste_message(&chem, &[], &bread, 15.0), TEXT_NO_TASTE);
    }

    #[test]
    fn eat_verbs_are_translated_like_bandastation() {
        assert_eq!(ru_eat_verb("chomp", false), "лопаете");
        assert_eq!(ru_eat_verb("chomp", true), "лопает");
        assert_eq!(ru_eat_verb("slurp", true), "slurps");
    }
}
