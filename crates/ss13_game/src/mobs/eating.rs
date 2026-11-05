//! Basic mobs eating, after tg `/datum/element/basic_eating` (`code/datums/elements/basic_eating.dm`)
//! and the controllers' `BB_BASIC_FOODS` (`find_food`, `move_to_and_eat`): what a mob eats, the
//! bite (heals `heal_amt`), and the `BB_NEXT_FOOD_EAT` cooldown its AI waits before looking for
//! more (tg `on_mob_eat`, `EAT_FOOD_COOLDOWN` unless the controller sets `BB_EAT_FOOD_COOLDOWN`).
//!
//! A mob eats what it is fed out of combat mode (tg `try_feed`) and what its AI walks up to.
// shortcut: the food lists are copied from each mob's `Initialize` (they are static lists or
// `typecacheof` expressions the DM dump does not resolve); move them to `mobs.ron` when the
// dump carries them.

use bevy::prelude::*;
use ss13_net::chat::server::SendChat;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};

use super::BasicMob;
use super::ai::{AiController, Key, Value};
use crate::combat::CombatRng;
use crate::health::{DEFAULT_MESSAGE_RANGE, DamageType, Health};
use crate::inventory::CarriedBy;
use crate::server::PlayerMob;

/// `EAT_FOOD_COOLDOWN`, seconds.
pub const EAT_FOOD_COOLDOWN: f64 = 45.0;

/// A mob's `basic_eating` (and `BB_BASIC_FOODS`, the same list everywhere it is ported).
#[derive(Debug, PartialEq)]
pub struct Eating {
    /// `food_types`: DM type roots.
    pub foods: &'static [&'static str],
    /// `heal_amt`.
    pub heal: f32,
    /// `BB_EAT_FOOD_COOLDOWN`, seconds.
    pub cooldown: f64,
}

static COW: Eating = Eating {
    foods: &["/obj/item/food/grown/wheat"],
    heal: 0.0,
    cooldown: EAT_FOOD_COOLDOWN,
};
static GOAT: Eating = Eating {
    foods: &[
        "/obj/structure/alien/resin/flower_bud",
        "/obj/structure/glowshroom",
        "/obj/structure/spacevine",
    ],
    heal: 0.0,
    cooldown: EAT_FOOD_COOLDOWN,
};
static LIZARD: Eating = Eating {
    foods: &["/mob/living/basic/butterfly", "/mob/living/basic/cockroach"],
    heal: 5.0,
    cooldown: EAT_FOOD_COOLDOWN,
};
/// BandaStation `lizard/big` (iguanas, gators, crocodiles).
static BIG_LIZARD: Eating = Eating {
    foods: &[
        "/obj/item/food/meat",
        "/obj/item/food/deadmouse",
        "/mob/living/basic/mouse",
        "/mob/living/basic/butterfly",
        "/mob/living/basic/cockroach",
    ],
    heal: 5.0,
    cooldown: EAT_FOOD_COOLDOWN,
};
static MOTHROACH: Eating = Eating {
    foods: &["/obj/item/clothing"],
    heal: 0.0,
    cooldown: 60.0,
};
static PENGUIN: Eating = Eating {
    foods: &["/obj/item/fish"],
    heal: 10.0,
    cooldown: EAT_FOOD_COOLDOWN,
};
static SNAKE: Eating = Eating {
    foods: &["/mob/living/basic/mouse", "/obj/item/food/deadmouse"],
    heal: 2.0,
    cooldown: EAT_FOOD_COOLDOWN,
};
static TURTLE: Eating = Eating {
    foods: &["/obj/item/seeds"],
    heal: 0.0,
    cooldown: EAT_FOOD_COOLDOWN,
};
static SNAIL: Eating = Eating {
    foods: &["/obj/item/food/grown", "/obj/item/food/appleslice"],
    heal: 0.0,
    cooldown: EAT_FOOD_COOLDOWN,
};

/// The `basic_eating` of mob type `path` (cows and snakes through `tameable`).
pub fn eating_of(path: &str) -> Option<&'static Eating> {
    let under = |root: &str| ss13_mapload::fidelity::is_under(path, root);
    let table: [(&str, &'static Eating); 9] = [
        ("/mob/living/basic/lizard/big", &BIG_LIZARD),
        ("/mob/living/basic/lizard", &LIZARD),
        ("/mob/living/basic/cow", &COW),
        ("/mob/living/basic/goat", &GOAT),
        ("/mob/living/basic/mothroach", &MOTHROACH),
        ("/mob/living/basic/pet/penguin", &PENGUIN),
        ("/mob/living/basic/snake", &SNAKE),
        ("/mob/living/basic/turtle", &TURTLE),
        ("/mob/living/basic/snail", &SNAIL),
    ];
    table
        .into_iter()
        .find(|(root, _)| under(root))
        .map(|(_, e)| e)
}

/// The mob's [`Eating`].
pub fn eating(world: &World, mob: Entity) -> Option<&'static Eating> {
    eating_of(&world.get::<BasicMob>(mob)?.def.path)
}

/// `is_type_in_list(thing, food_types)`, by the thing's DM type.
pub fn is_food(world: &World, thing: Entity, eating: &Eating) -> bool {
    let Ok(e) = world.get_entity(thing) else {
        return false;
    };
    let path: Option<&str> = e
        .get::<BasicMob>()
        .map(|b| b.def.path.as_str())
        .or_else(|| e.get::<ss13_mapload::DmItem>().map(|i| i.path.as_str()))
        .or_else(|| e.get::<ss13_mapload::DmObj>().map(|o| o.path.as_str()));
    path.is_some_and(|p| {
        eating
            .foods
            .iter()
            .any(|root| ss13_mapload::fidelity::is_under(p, root))
    })
}

/// tg `pickup_item` on a food target: on a turf (not carried), and never a player's mob (tg
/// would delete it; ss13-rs keeps players' entities).
pub fn edible_now(world: &World, eater: Entity, thing: Entity, eating: &Eating) -> bool {
    thing != eater
        && world.get::<TilePos>(thing).is_some()
        && world.get::<CarriedBy>(thing).is_none()
        && world.get::<PlayerMob>(thing).is_none()
        && is_food(world, thing, eating)
}

/// tg `basic_eating/try_eating` + `finish_eating`: the bite message, the heal and the food
/// gone. Returns the game time of the eater's next meal (tg `on_mob_eat` -> `BB_NEXT_FOOD_EAT`)
/// when it ate; the caller puts it on the blackboard (the planner holds it while planning).
pub fn eat(world: &mut World, eater: Entity, food: Entity) -> Option<f64> {
    let eating = eating(world, eater)?;
    if !is_food(world, food, eating) {
        return None;
    }
    let &pos = world.get::<TilePos>(eater)?;
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut rng = CombatRng(world.resource::<CombatRng>().0);
    // tg `pick("bite","chew","nibble","gnaw","gobble","chomp")`, in BandaStation's
    // `[eat_verbs]` third person.
    const VERBS: [&str; 6] = [
        "кусает",
        "жуёт",
        "покусывает",
        "грызёт",
        "пожирает",
        "лопает",
    ];
    let verb = VERBS[rng.range(0, VERBS.len() as u32 - 1) as usize];
    world.resource_mut::<CombatRng>().0 = rng.0;
    let name = |e: Entity| {
        world
            .get::<Name>(e)
            .map(|n| n.as_str().to_owned())
            .or_else(|| world.get::<ss13_mapload::DmName>(e).map(|n| n.name.clone()))
            .unwrap_or_default()
    };
    let (eater_name, food_name) = (name(eater), name(food));
    let mut chars = eater_name.chars();
    let eater_name: String = chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default();
    if eating.heal > 0.0
        && let Some(mut health) = world.get_mut::<Health>(eater)
    {
        health.adjust(DamageType::Brute, -eating.heal);
    }
    let clients = crate::obj_damage::clients_in_range(world, pos, DEFAULT_MESSAGE_RANGE, &[]);
    world.write_message(SendChat {
        clients,
        message: ChatMessage {
            channel: ChatChannel::Info,
            segments: vec![ChatSegment::new(
                Span::Notice,
                format!("{eater_name} {verb} {food_name}."),
            )],
        },
    });
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, "sound/items/eatfood.ogg", 30.0, true),
    );
    world.despawn(food);
    Some(now + eating.cooldown)
}

/// A human feeds a basic mob what it eats, out of combat mode (tg `basic_eating/try_feed`):
/// true when the mob ate it.
pub fn try_feed(world: &mut World, mob: Entity, food: Entity) -> bool {
    let Some(eating) = eating(world, mob) else {
        return false;
    };
    let conscious = world.get::<crate::health::Stat>(mob) == Some(&crate::health::Stat::Conscious);
    if !conscious || !is_food(world, food, eating) {
        return false;
    }
    let Some(next) = eat(world, mob, food) else {
        return false;
    };
    if let Some(mut controller) = world.get_mut::<AiController>(mob) {
        controller.bb.set(Key::NextFoodEat, Value::Time(next));
    }
    true
}
