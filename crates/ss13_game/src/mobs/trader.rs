//! NPC traders (server), after tg `/mob/living/basic/trader` (`code/modules/mob/living/basic/
//! trader/`) and `/datum/component/trader` (`code/datums/components/trader/trader.dm`): the
//! generic trader and Mr. Bones.
//!
//! - A hand out of combat mode on the trader opens its menu (tg radial menus, here tgui lists
//!   and Yes/No alerts): buy a product for the holochip in hand, sell the wanted item in hand
//!   for a new holochip (stacks by the sheet, as much as it still wants), or talk (its lore,
//!   what it buys, what it sells). Every answer is checked again: both conscious, still next to
//!   each other, the trader out of combat, the money or the item still in hand, the stock left.
//! - The first capable human it sees makes it set up shop (`setup_shop`): its opening line, a
//!   seat under it, its sign beside it (left of its facing, else right) with sparks.
//! - Hit, it shoots its attacker (`ranged_attacks`, 3 s) shouting its battle line, and gloats
//!   when the fight ends (`ai_swap_combat_mode`); dead, Mr. Bones leaves his remains.
// Left out: `apply_dynamic_human_appearance` (the generic trader keeps its sprite), the generic
// trader's assistant corpse (no corpse spawners), Mr. Bones walking up to his first customer
// before opening (`BB_TRADER_RUSH_TO_SELL`: he opens where he stands), walking off the shop
// spot once it is set (tg `not_while_on_target`), `log_econ`.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::mobs::MobDef;
use ss13_net::chat::server::SendChat;
use ss13_net::server::Networked;
use ss13_shared::chat::Span;
use ss13_shared::{Dir, Facing, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};
use ss13_world::SpatialIndex;

use super::ai::{AiController, Key, Value};
use crate::cargo::money::{Holochip, give_holochip, refresh_holochip};
use crate::combat::CombatRng;
use crate::health::{DEFAULT_MESSAGE_RANGE, Stat};
use crate::inventory::Inventory;
use crate::server::PlayerMob;
use crate::tools::{Stack, stack_amount, tell};

pub const ROOT: &str = "/mob/living/basic/trader";
pub const MR_BONES: &str = "/mob/living/basic/trader/mrbones";
/// `PAYCHECK_CREW`.
const PAYCHECK: f32 = 50.0;
/// `INFINITY` stock or demand.
const UNLIMITED: u32 = u32::MAX;
/// `oview()` of the first customer search: `world.view`.
const VIEW: u32 = 7;
const TITLE: &str = "Торговец";

/// What a trader says (`say_phrases`).
#[derive(Debug, PartialEq)]
pub struct Phrases {
    pub rejected: &'static str,
    pub canceled: &'static str,
    pub accepted: &'static str,
    pub interested: &'static str,
    pub buy: &'static str,
    pub no_cash: &'static str,
    pub worthless: &'static str,
    pub has_enough: &'static str,
    pub lore: &'static [&'static str],
    pub not_buying: &'static str,
    pub not_selling: &'static str,
    pub battle_start: &'static str,
    pub battle_end: &'static str,
    pub opening: &'static str,
}

/// `/datum/trader_data`.
#[derive(Debug, PartialEq)]
pub struct TraderData {
    pub shop_spot: &'static str,
    pub sign: &'static str,
    pub sell_sound: &'static str,
    /// `initial_products`: type, price in `PAYCHECK_CREW`s (stock unlimited).
    pub products: &'static [(&'static str, f32)],
    /// `initial_wanteds`: type, price in `PAYCHECK_CREW`s, price description (demand unlimited).
    pub wanteds: &'static [(&'static str, f32, &'static str)],
    pub phrases: Phrases,
    /// `ranged_attack_casing`.
    pub casing: &'static str,
    /// `loot` that is an object (corpse spawners are left out).
    pub loot: Option<&'static str>,
}

const COMMON: Phrases = Phrases {
    rejected: "Sorry, I'm not a fan of anything you're showing me. Give me something better and we'll talk.",
    canceled: "What a shame, tell me if you changed your mind.",
    accepted: "Pleasure doing business with you.",
    interested: "Hey, you've got an item that interests me, I'd like to buy it, I'll give you some cash for it, deal?",
    buy: "Pleasure doing business with you.",
    no_cash: "Sorry adventurer, I can't give credit! Come back when you're a little mmmmm... richer!",
    worthless: "This item seems to be worthless on a closer look, I won't buy this.",
    has_enough: "I already bought enough of this for the time being.",
    lore: &["Hello! I am the test trader.", "Oooooooo~!"],
    not_buying: "I'm currently buying nothing at the moment.",
    not_selling: "I'm currently selling nothing at the moment.",
    battle_start: "Thief!",
    battle_end: "That is a discount I call death.",
    opening: "Welcome to my shop, friend!",
};

pub static GENERIC: TraderData = TraderData {
    shop_spot: "/obj/structure/chair/plastic",
    sign: "/obj/structure/trader_sign",
    sell_sound: "sound/effects/cashregister.ogg",
    products: &[("/obj/item/food/burger/ghost", 4.0)],
    wanteds: &[("/obj/item/ectoplasm", 2.0, "")],
    phrases: COMMON,
    casing: "/obj/item/ammo_casing/shotgun/buckshot",
    loot: None,
};

pub static BONES: TraderData = TraderData {
    shop_spot: "/obj/structure/chair/wood/wings",
    sign: "/obj/structure/trader_sign/mrbones",
    sell_sound: "sound/mobs/non-humanoids/hiss/hiss2.ogg",
    products: &[
        ("/obj/item/clothing/head/helmet/skull", 3.0),
        ("/obj/item/clothing/mask/bandana/skull/black", 1.0),
        ("/obj/item/food/cookie/sugar/spookyskull", 0.2),
        ("/obj/item/instrument/trombone/spectral", 200.0),
        ("/obj/item/shovel/serrated", 3.0),
    ],
    wanteds: &[
        ("/obj/item/reagent_containers/condiment/milk", 20.0, ""),
        ("/obj/item/stack/sheet/bone", 8.4, ", per sheet of bone"),
    ],
    phrases: Phrases {
        buy: "Bone appetit!",
        lore: &[
            "Hello, I am Mr. Bones!",
            "The ride never ends!",
            "I'd really like a refreshing carton of milk!",
            "I'm willing to play big prices for BONES! Need materials to make merch, eh?",
            "It's a beautiful day outside. Birds are singing, Flowers are blooming... On days like these, kids like you... Should be buying my wares!",
        ],
        battle_start: "The ride ends for you!",
        battle_end: "Mr. Bones never misses!",
        opening: "My wild ride is open!",
        ..COMMON
    },
    casing: "/obj/item/ammo_casing/energy/bolt/halloween",
    loot: Some("/obj/effect/decal/remains/human"),
};

/// `ranged_attack_sound`.
const SHOT_SOUND: &str = "sound/items/weapons/gun/pistol/shot.ogg";

/// Every type a trader spawns or looks up (`GAME_RUNTIME_TYPES`).
pub fn runtime_types() -> Vec<&'static str> {
    let mut out = Vec::new();
    for data in [&GENERIC, &BONES] {
        out.extend([data.shop_spot, data.sign, data.casing]);
        out.extend(data.products.iter().map(|p| p.0));
        out.extend(data.loot);
    }
    out
}

/// The trader data of type `path`.
pub fn data_of(path: &str) -> Option<&'static TraderData> {
    if is_under(path, MR_BONES) {
        Some(&BONES)
    } else if is_under(path, ROOT) {
        Some(&GENERIC)
    } else {
        None
    }
}

/// `/datum/component/trader`: stock and demand left, and the shop's state.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Trader {
    pub data: &'static TraderData,
    pub stock: Vec<u32>,
    pub demand: Vec<u32>,
    /// `ai_swap_combat_mode`: a target is set.
    pub in_combat: bool,
    /// `BB_SHOP_SPOT`.
    pub shop: Option<Entity>,
}

impl Trader {
    pub fn new(data: &'static TraderData) -> Self {
        Self {
            data,
            stock: vec![UNLIMITED; data.products.len()],
            demand: vec![UNLIMITED; data.wanteds.len()],
            in_combat: false,
            shop: None,
        }
    }
}

/// The price of `pc` paychecks (DM credits are whole).
fn price(pc: f32) -> u32 {
    (pc * PAYCHECK).round() as u32
}

/// What an open prompt asks the customer about.
#[derive(Component, Clone, Debug, PartialEq)]
enum Step {
    Root(Vec<Choice>),
    Buy,
    ConfirmBuy(usize),
    ConfirmSell {
        item: Entity,
        wanted: usize,
        cost: u32,
    },
    Talk,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Choice {
    Buy,
    Sell,
    Talk,
}

impl Choice {
    fn label(self) -> &'static str {
        match self {
            Self::Buy => "Купить",
            Self::Sell => "Продать",
            Self::Talk => "Поговорить",
        }
    }
}

/// The prompt's trader.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
struct TradeWith(Entity);

pub(super) fn build(app: &mut App) {
    app.add_systems(
        Update,
        (
            answers
                .in_set(ss13_tgui::server::UiSystems::Handle)
                .after(ss13_tgui::prompt::PromptSystems::Answer),
            (combat_mode, setup_shop).after(crate::health::HealthSystems),
        ),
    );
}

pub(super) fn attach(world: &mut World, mob: Entity, def: &MobDef) {
    let Some(data) = data_of(&def.path) else {
        return;
    };
    world.entity_mut(mob).insert(Trader::new(data));
    let gun = super::troopers::Gun {
        casing: data.casing,
        sound: SHOT_SOUND,
        cooldown: 3.0,
        burst: 1,
    };
    let casing = world
        .get_resource::<ss13_proto::Prototypes>()
        .and_then(|p| p.get(data.casing))
        .and_then(|p| p.get::<ss13_mapload::DmCasing>())
        .map(|c| c.data.clone());
    if let Some(casing) = casing {
        world
            .entity_mut(mob)
            .insert(super::troopers::ranged_attacks(gun, &casing));
    }
}

/// `death_drops` of a dead trader.
pub(super) fn on_death(world: &mut World, def: &MobDef, pos: TilePos) {
    if let Some(loot) = data_of(&def.path).and_then(|d| d.loot) {
        crate::construction::spawn_obj(world, loot, pos);
    }
}

/// The trader says `text` (`say`).
fn say(world: &mut World, trader: Entity, text: &str) {
    let Some(&pos) = world.get::<TilePos>(trader) else {
        return;
    };
    let name = super::defense::name_of(world, trader);
    let clients = crate::obj_damage::clients_in_range(world, pos, DEFAULT_MESSAGE_RANGE, &[]);
    world.write_message(SendChat {
        clients,
        message: ss13_chat::server::say_message(&name, text),
    });
}

fn pick(world: &mut World, lines: &[&'static str]) -> &'static str {
    if lines.is_empty() {
        return "";
    }
    let mut rng = CombatRng(world.resource::<CombatRng>().0);
    let i = rng.range(0, lines.len() as u32 - 1) as usize;
    world.resource_mut::<CombatRng>().0 = rng.0;
    lines[i]
}

fn face(world: &mut World, trader: Entity, customer: Entity) {
    let (Some(&a), Some(&b)) = (world.get::<TilePos>(trader), world.get::<TilePos>(customer))
    else {
        return;
    };
    if let Some(dir) = crate::pull::dir_between(a, b)
        && let Some(mut facing) = world.get_mut::<Facing>(trader)
    {
        facing.set_if_neq(Facing(dir));
    }
}

/// The name of type `path` (`initial(thing.name)`).
fn type_name(world: &World, path: &str) -> String {
    world
        .get_resource::<ss13_proto::Prototypes>()
        .and_then(|p| p.get(path))
        .and_then(|p| p.get::<ss13_mapload::DmName>())
        .map(|n| n.name.clone())
        .unwrap_or_else(|| path.rsplit('/').next().unwrap_or(path).to_owned())
}

/// `can_trade` and `check_menu`: the trader conscious and out of combat, the customer
/// conscious, able and next to it.
fn can_trade(world: &World, trader: Entity, customer: Entity) -> bool {
    let Some(t) = world.get::<Trader>(trader) else {
        return false;
    };
    let near = match (world.get::<TilePos>(trader), world.get::<TilePos>(customer)) {
        (Some(&a), Some(&b)) => world
            .resource::<SpatialIndex>()
            .adjacent(b, a, |e| e == trader),
        _ => false,
    };
    !t.in_combat
        && world.get::<Stat>(trader) == Some(&Stat::Conscious)
        && world.get::<Stat>(customer) == Some(&Stat::Conscious)
        && !crate::status_effects::incapacitation(world, customer).incapacitated
        && near
}

fn ask(world: &mut World, client: Entity, customer: Entity, trader: Entity, step: Step) {
    let (message, kind) = match &step {
        Step::Root(choices) => (
            String::new(),
            PromptKind::List {
                items: choices.iter().map(|c| c.label().to_owned()).collect(),
                default: None,
            },
        ),
        Step::Buy => {
            let data = world.get::<Trader>(trader).map(|t| t.data);
            let items = data.map_or_else(Vec::new, |d| {
                d.products.iter().map(|p| type_name(world, p.0)).collect()
            });
            (
                "Что купить?".to_owned(),
                PromptKind::List {
                    items,
                    default: None,
                },
            )
        }
        Step::Talk => (
            String::new(),
            PromptKind::List {
                items: vec![
                    "Расскажи о себе".to_owned(),
                    "Что продаёшь?".to_owned(),
                    "Что покупаешь?".to_owned(),
                ],
                default: None,
            },
        ),
        Step::ConfirmBuy(_) | Step::ConfirmSell { .. } => (
            "Согласиться?".to_owned(),
            PromptKind::Alert {
                buttons: vec!["Да".to_owned(), "Нет".to_owned()],
                autofocus: true,
            },
        ),
    };
    let prompt = ask_world(world, Ask::new(client, customer, TITLE, message, kind));
    world.entity_mut(prompt).insert((step, TradeWith(trader)));
}

/// `on_attack_hand`: the trader's menu, for a customer out of combat mode. True when it opened.
pub fn attack_hand(world: &mut World, customer: Entity, trader: Entity) -> bool {
    let Some(client) = world.get::<PlayerMob>(customer).map(|p| p.client) else {
        return false;
    };
    let Some(t) = world.get::<Trader>(trader).cloned() else {
        return false;
    };
    if !can_trade(world, trader, customer) {
        return false;
    }
    let mut choices = Vec::new();
    if !t.data.products.is_empty() {
        choices.push(Choice::Buy);
    }
    if !t.data.wanteds.is_empty() {
        choices.push(Choice::Sell);
    }
    choices.push(Choice::Talk);
    face(world, trader, customer);
    ask(world, client, customer, trader, Step::Root(choices));
    true
}

fn answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answered: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, customer, answer) in answered {
        let (Some(step), Some(TradeWith(trader))) = (
            world.get::<Step>(prompt).cloned(),
            world.get::<TradeWith>(prompt).copied(),
        ) else {
            continue;
        };
        if !can_trade(world, trader, customer) {
            continue;
        }
        answer_step(world, client, customer, trader, step, answer);
    }
}

fn answer_step(
    world: &mut World,
    client: Entity,
    customer: Entity,
    trader: Entity,
    step: Step,
    answer: Option<PromptAnswer>,
) {
    let Some(t) = world.get::<Trader>(trader).cloned() else {
        return;
    };
    let phrases = &t.data.phrases;
    let item = |a: &Option<PromptAnswer>| match a {
        Some(PromptAnswer::Item(i)) => Some(*i),
        _ => None,
    };
    let yes = matches!(answer, Some(PromptAnswer::Button(0)));
    match step {
        Step::Root(choices) => match item(&answer).and_then(|i| choices.get(i)) {
            Some(Choice::Buy) => ask(world, client, customer, trader, Step::Buy),
            Some(Choice::Sell) => try_sell(world, client, customer, trader),
            Some(Choice::Talk) => ask(world, client, customer, trader, Step::Talk),
            None => {}
        },
        Step::Buy => {
            let Some(i) = item(&answer).filter(|&i| i < t.data.products.len()) else {
                return;
            };
            let (path, pc) = t.data.products[i];
            let name = type_name(world, path);
            face(world, trader, customer);
            if t.stock[i] == 0 {
                say(
                    world,
                    trader,
                    &format!("{name} appears to be out of stock."),
                );
                return;
            }
            say(
                world,
                trader,
                &format!(
                    "It will cost you {} cr to buy the {name}. Are you sure you want to buy it?",
                    price(pc)
                ),
            );
            ask(world, client, customer, trader, Step::ConfirmBuy(i));
        }
        Step::ConfirmBuy(i) => {
            if !yes || i >= t.data.products.len() || t.stock[i] == 0 {
                return;
            }
            face(world, trader, customer);
            let (path, pc) = t.data.products[i];
            if !spend_offhand_money(world, customer, price(pc)) {
                say(world, trader, phrases.no_cash);
                return;
            }
            if let Some(thing) = crate::construction::spawn_detached(world, path) {
                world.entity_mut(thing).insert(Networked);
                let pos = world.get::<TilePos>(customer).copied();
                match world.get::<Inventory>(customer).cloned() {
                    Some(inv) => crate::storage::put_in_hands(world, customer, &inv, thing),
                    None => {
                        if let Some(pos) = pos {
                            crate::storage::take_out(world, thing, pos);
                        }
                    }
                }
            }
            play(world, trader, t.data.sell_sound);
            if let Some(mut tr) = world.get_mut::<Trader>(trader)
                && tr.stock[i] != UNLIMITED
            {
                tr.stock[i] -= 1;
            }
            say(world, trader, phrases.buy);
        }
        Step::ConfirmSell { item, wanted, cost } => {
            let held = world
                .get::<Inventory>(customer)
                .is_some_and(|inv| inv.hands.contains(&Some(item)));
            if !held || t.demand.get(wanted).is_none_or(|&d| d == 0) {
                return;
            }
            if !yes {
                say(world, trader, phrases.canceled);
                return;
            }
            say(world, trader, phrases.accepted);
            play(world, trader, t.data.sell_sound);
            // `exchange_sold_items`: a stack gives what is still wanted, anything else goes.
            let demand = t.demand[wanted];
            let sold = if world.get::<Stack>(item).is_some() {
                let n = stack_amount(world, item).min(demand);
                crate::tools::use_stack(world, item, n);
                n
            } else {
                world.despawn(item);
                1
            };
            if let Some(mut tr) = world.get_mut::<Trader>(trader)
                && tr.demand[wanted] != UNLIMITED
            {
                tr.demand[wanted] = tr.demand[wanted].saturating_sub(sold);
            }
            give_holochip(world, customer, cost);
        }
        Step::Talk => match item(&answer) {
            Some(0) => {
                let line = pick(world, phrases.lore);
                say(world, trader, line);
            }
            Some(1) => trader_sells_what(world, client, trader),
            Some(2) => trader_buys_what(world, client, trader),
            _ => {}
        },
    }
}

fn play(world: &mut World, trader: Entity, sound: &str) {
    if let Some(&pos) = world.get::<TilePos>(trader) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, sound, 50.0, true));
    }
}

/// `spend_buyer_offhand_money`: a holochip in hand with enough credits pays.
fn spend_offhand_money(world: &mut World, customer: Entity, cost: u32) -> bool {
    let Some(inv) = world.get::<Inventory>(customer) else {
        return false;
    };
    let Some(chip) = inv
        .hands
        .iter()
        .flatten()
        .copied()
        .find(|&h| world.get::<Holochip>(h).is_some())
    else {
        return false;
    };
    let credits = world.get::<Holochip>(chip).map_or(0, |c| c.credits);
    if credits < cost {
        return false;
    }
    if credits == cost {
        world.despawn(chip);
    } else {
        world.entity_mut(chip).insert(Holochip {
            credits: credits - cost,
        });
        refresh_holochip(world, chip);
    }
    true
}

/// `try_sell` / `sell_item`: the first held item the trader wants, at its price (per sheet).
fn try_sell(world: &mut World, client: Entity, customer: Entity, trader: Entity) {
    let Some(t) = world.get::<Trader>(trader).cloned() else {
        return;
    };
    let phrases = &t.data.phrases;
    let held: Vec<Entity> = world
        .get::<Inventory>(customer)
        .map(|inv| inv.hands.iter().flatten().copied().collect())
        .unwrap_or_default();
    for item in held {
        let Some(path) = world
            .get::<ss13_mapload::DmItem>(item)
            .map(|d| d.path.clone())
        else {
            continue;
        };
        // The exact type first, then the first parent on the list.
        let wanted = t
            .data
            .wanteds
            .iter()
            .position(|w| w.0 == path)
            .or_else(|| t.data.wanteds.iter().position(|w| is_under(&path, w.0)));
        let Some(wanted) = wanted else {
            continue;
        };
        if t.demand[wanted] == 0 {
            say(world, trader, phrases.has_enough);
            continue;
        }
        let per = price(t.data.wanteds[wanted].1);
        let amount = if world.get::<Stack>(item).is_some() {
            stack_amount(world, item)
        } else {
            1
        };
        let cost = per * amount;
        if cost == 0 {
            say(world, trader, phrases.worthless);
            continue;
        }
        say(world, trader, phrases.interested);
        let name = super::defense::name_of(world, item);
        say(
            world,
            trader,
            &format!("You will receive {cost} cr for the {name}."),
        );
        face(world, trader, customer);
        ask(
            world,
            client,
            customer,
            trader,
            Step::ConfirmSell { item, wanted, cost },
        );
        return;
    }
    say(world, trader, phrases.rejected);
}

fn trader_sells_what(world: &mut World, client: Entity, trader: Entity) {
    let Some(t) = world.get::<Trader>(trader).cloned() else {
        return;
    };
    if t.data.products.is_empty() {
        say(world, trader, t.data.phrases.not_selling);
        return;
    }
    let mut lines = vec!["I'm currently selling the following:".to_owned()];
    for (i, (path, pc)) in t.data.products.iter().enumerate() {
        let name = type_name(world, path);
        let left = match t.stock[i] {
            UNLIMITED => "an infinite amount".to_owned(),
            n => n.to_string(),
        };
        let out = if t.stock[i] == 0 {
            "(OUT OF STOCK) "
        } else {
            ""
        };
        lines.push(format!(
            "• {out}{name} for {} cr; {left} left in stock",
            price(*pc)
        ));
    }
    tell(world, client, Span::Notice, lines.join("\n"));
}

fn trader_buys_what(world: &mut World, client: Entity, trader: Entity) {
    let Some(t) = world.get::<Trader>(trader).cloned() else {
        return;
    };
    if t.data.wanteds.is_empty() {
        say(world, trader, t.data.phrases.not_buying);
        return;
    }
    let mut lines = vec!["I'm willing to buy the following:".to_owned()];
    for (i, (path, pc, desc)) in t.data.wanteds.iter().enumerate() {
        let name = type_name(world, path);
        let more = match t.demand[i] {
            UNLIMITED => "as many as I can.".to_owned(),
            n => n.to_string(),
        };
        let full = if t.demand[i] == 0 {
            "(DOESN'T WANT MORE) "
        } else {
            ""
        };
        lines.push(format!(
            "• {full}{name} for {} cr{desc}; willing to buy {more}",
            price(*pc)
        ));
    }
    tell(world, client, Span::Notice, lines.join("\n"));
}

/// `ai_swap_combat_mode`: the battle lines as a target comes and goes.
fn combat_mode(world: &mut World) {
    let traders: Vec<(Entity, bool, bool)> = world
        .query::<(Entity, &Trader, Option<&AiController>, &Stat)>()
        .iter(world)
        .filter(|(_, _, _, s)| **s == Stat::Conscious)
        .map(|(e, t, ai, _)| {
            let fighting = ai.is_some_and(|a| a.bb.entity(Key::CurrentTarget).is_some());
            (e, t.in_combat, fighting)
        })
        .collect();
    for (trader, was, now) in traders {
        if was == now {
            continue;
        }
        let data = match world.get_mut::<Trader>(trader) {
            Some(mut t) => {
                t.in_combat = now;
                t.data
            }
            None => continue,
        };
        let line = if now {
            data.phrases.battle_start
        } else {
            data.phrases.battle_end
        };
        say(world, trader, line);
    }
}

/// `acquire_target` of `BB_FIRST_CUSTOMER` (`capable_human` in `oview`) and `setup_shop`.
fn setup_shop(world: &mut World) {
    let traders: Vec<Entity> = world
        .query::<(Entity, &Trader, &Stat)>()
        .iter(world)
        .filter(|(_, t, s)| t.shop.is_none() && !t.in_combat && **s == Stat::Conscious)
        .map(|(e, _, _)| e)
        .collect();
    for trader in traders {
        let Some(&pos) = world.get::<TilePos>(trader) else {
            continue;
        };
        let customer = world
            .resource::<SpatialIndex>()
            .in_radius(pos, VIEW)
            .filter(|&e| e != trader)
            .find(|&e| {
                world.get::<PlayerMob>(e).is_some()
                    && world.get::<Inventory>(e).is_some()
                    && world.get::<Stat>(e) == Some(&Stat::Conscious)
                    && world
                        .get::<TilePos>(e)
                        .is_some_and(|&p| super::ai::behaviors::can_see(world, pos, p, VIEW))
            });
        if customer.is_some() {
            open_shop(world, trader, pos);
        }
    }
}

/// `/datum/action/setup_shop/Trigger`.
pub fn open_shop(world: &mut World, trader: Entity, pos: TilePos) {
    let Some(data) = world.get::<Trader>(trader).map(|t| t.data) else {
        return;
    };
    say(world, trader, data.phrases.opening);
    let facing = world.get::<Facing>(trader).map_or(Dir::South, |f| f.0);
    let spot = crate::construction::spawn_obj(world, data.shop_spot, pos);
    if let Some(spot) = spot {
        world.entity_mut(spot).insert(Facing(facing));
    }
    if let Some(mut t) = world.get_mut::<Trader>(trader) {
        // A placeholder when the seat type is missing, so it opens once.
        t.shop = Some(spot.unwrap_or(trader));
    }
    if let (Some(spot), Some(mut ai)) = (spot, world.get_mut::<AiController>(trader)) {
        ai.bb.set(Key::ShopSpot, Value::Entity(spot));
    }
    play(world, trader, data.sell_sound);
    // `try_find_valid_spot`: left of its facing (`turn(dir, -90)`), else right.
    let (dx, dy) = facing.offset();
    let turned = |x: i32, y: i32| {
        Dir::DMI_ORDER
            .into_iter()
            .find(|d| d.offset() == (x, y))
            .unwrap_or(Dir::South)
    };
    let sides = [turned(dy, -dx), turned(-dy, dx)];
    let free = |world: &World, p: TilePos| {
        // shortcut: groundless (open space) turfs are not told apart; a dense thing blocks.
        world.resource::<SpatialIndex>().dense_on_tile(p).is_empty()
    };
    let Some(sign_at) = sides
        .into_iter()
        .map(|d| pos.step(d))
        .find(|&p| free(world, p))
    else {
        return;
    };
    if crate::construction::spawn_obj(world, data.sign, sign_at).is_some() {
        crate::effects::particles::do_sparks(world, 3, false, sign_at);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_by_type_and_prices_in_paychecks() {
        assert_eq!(data_of(MR_BONES), Some(&BONES));
        assert_eq!(data_of(ROOT), Some(&GENERIC));
        assert_eq!(data_of("/mob/living/basic/carp"), None);
        assert_eq!(price(BONES.wanteds[1].1), 420);
        assert_eq!(price(BONES.products[2].1), 10);
        assert_eq!(price(GENERIC.products[0].1), 200);
        assert_eq!(BONES.phrases.rejected, GENERIC.phrases.rejected);
        assert_ne!(BONES.phrases.buy, GENERIC.phrases.buy);
    }

    #[test]
    fn trader_types_are_runtime_types() {
        let registry = ss13_mapload::runtime::runtime_types();
        let missing: Vec<&str> = runtime_types()
            .into_iter()
            .filter(|p| !registry.contains(p))
            .collect();
        assert!(missing.is_empty(), "{missing:?}");
    }

    #[test]
    fn holochips_pay_and_only_enough_pays() {
        let mut world = World::new();
        let chip = world.spawn(Holochip { credits: 300 }).id();
        let mut inv = Inventory::default();
        inv.hands[0] = Some(chip);
        let buyer = world.spawn(inv).id();
        assert!(!spend_offhand_money(&mut world, buyer, 400));
        assert!(spend_offhand_money(&mut world, buyer, 300));
        assert!(world.get_entity(chip).is_err(), "spent to nothing");
    }
}
