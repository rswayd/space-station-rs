//! Physical money (server), after tg `get_item_credit_value` (`code/modules/economy/_economy.dm`),
//! `/obj/item/stack/spacecash` (`stacks/cash.dm`), `/obj/item/holochip` (`credit_holochip.dm`),
//! `/obj/item/coin` (`mining/ores_coins.dm`), `/obj/item/poker_chip` (`economy/poker_chips.dm`),
//! the ID card's money procs (`insert_money`, `mass_insert_money`, `click_alt`, the balance on
//! examine; `cards_ids.dm`) and `/datum/component/payment/handle_cardless`.
//!
//! - [`credit_value`]: bills are `value * amount`, a holochip its `credits`, a coin the worth of
//!   its material (`value_per_unit * COIN_MATERIAL_AMOUNT`), a poker chip its `redeem_value`.
//! - Holochips: named and drawn by their sum (`_kilo`/`_mega`/`_giga` states, the coloured
//!   overlay), one used on another merges them, alt-click on a held one splits a sum into a new
//!   chip, an EMP wipes one with `prob(60 / severity)`.
//! - An ID card with an account takes cash, holochips, coins and poker chips used on it (or it
//!   used on them), and a money bag's contents; alt-click on a held ID withdraws a holochip; the
//!   examine shows the account and, with command access, its department budget.
//! - Coins land on a random side when made; a coin used in hand flips (1.5 s cooldown), the
//!   landing is announced 1.5 s later if it stayed where it was and the flipper can act.
//! - [`pay_with_cash`] (`handle_cardless`): what a buyer carries pays, change comes back as a
//!   holochip (`crate::vending` falls back to it when the card cannot pay).
// shortcut: no coin strings (cable coil, wirecutters), material tint or random pixel offset of
// coins, no pulled money in `pay_with_cash`, no Alt+right account linking (`set_new_account`)
// and no debt choice on withdrawal (accounts have no debt yet).
// Not tg: `handle_cardless` stops counting once the cost is covered (tg takes one more item
// when the sum matches exactly and gives it back as change: the same money).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::restaurant::HOLOCHIP;
use ss13_mapload::spawner::Rng;
use ss13_mapload::{DmAtom, DmItem, IconIndex, MapSeed};
use ss13_net::server::Networked;
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};

use super::MONEY_SYMBOL;
use super::economy::{BankAccountId, Economy, bank_card_talk, holder_of};
use crate::health::Stat;
use crate::inventory::{CarriedBy, IdCard, Inventory};
use crate::obj_damage::is_under;
use crate::tools::{Stack, tell};

pub const SPACECASH: &str = "/obj/item/stack/spacecash";
pub const COIN: &str = "/obj/item/coin";
pub const POKER_CHIP: &str = "/obj/item/poker_chip";
const MONEY_BAG: &str = "/obj/item/storage/bag/money";
const CASH_SOUND: &str = "sound/effects/cashregister.ogg";
const COINFLIP_SOUND: &str = "sound/items/coinflip.ogg";
/// `cooldown = world.time + 15` and the `sleep(1.5 SECONDS)` of `coin/attack_self`.
const FLIP_SECS: f64 = 1.5;

/// A holochip's `credits`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Holochip {
    pub credits: u32,
}

/// GLOB.allowed_money (coins, holochips, space cash): what `handle_cardless` spends.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Money;

/// A coin's `coinflip` (the side it shows) and flip `cooldown`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Coin {
    pub side: &'static str,
    pub ready_at: f64,
}

/// A flip in the air: announced at `at` (`sleep(1.5 SECONDS)`).
#[derive(Clone, Copy, Debug)]
struct Flip {
    coin: Entity,
    user: Entity,
    at: f64,
    holder: Option<Entity>,
}

#[derive(Resource)]
struct MoneyState {
    rng: Rng,
    flips: Vec<Flip>,
}

/// On a prompt: the ID card a withdrawal is asked for.
#[derive(Component, Clone, Copy, Debug)]
struct WithdrawFor(Entity);

/// On a prompt: the holochip a split is asked for.
#[derive(Component, Clone, Copy, Debug)]
struct SplitFor(Entity);

pub struct MoneyPlugin;

impl Plugin for MoneyPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::AttackSelf, 3187, |w, c| {
            crate::cargo::money::coin_attack_self(w, c.mob, c.target)
        });
        app.add_click_handler(ClickPhase::OtherHand, 473, |w, c| {
            crate::cargo::money::item_interaction(w, c.client, c.mob, c.held, c.target)
        });
        app.add_click_handler(ClickPhase::HeldOnTarget, 2977, |w, c| {
            crate::cargo::money::item_interaction(w, c.client, c.mob, c.held, c.target)
        });
        app.add_systems(Startup, init)
            .add_observer(on_item_added)
            .add_systems(
                Update,
                (
                    land_flips,
                    holochip_emps.in_set(crate::emp::EmpSystems::Effects),
                    prompt_answers
                        .in_set(ss13_tgui::server::UiSystems::Handle)
                        .after(ss13_tgui::prompt::PromptSystems::Answer),
                ),
            );
    }
}

fn init(world: &mut World) {
    let seed = world.get_resource::<MapSeed>().map_or(0, |s| s.0);
    world.insert_resource(MoneyState {
        rng: Rng::new(seed ^ 0x6d6f_6e79),
        flips: Vec::new(),
    });
}

/// `/obj/item/stack/spacecash/cN::value`.
pub fn bill_value(path: &str) -> u32 {
    if !is_under(path, SPACECASH) {
        return 0;
    }
    let tail = path.rsplit('/').next().unwrap_or("");
    match tail {
        "c1" => 1,
        "c10" => 10,
        "c20" => 20,
        "c50" => 50,
        "c100" => 100,
        "c200" => 200,
        "c500" => 500,
        "c1000" => 1000,
        "c10000" => 10000,
        _ => 0,
    }
}

/// `coin/finalize_material_effects`: `value_per_unit * COIN_MATERIAL_AMOUNT` (20) of the coin's
/// material; the antag token (`override_material_worth`) and the eldritch coin
/// (`material_flags = NONE`) are worth nothing.
pub fn coin_value(path: &str) -> u32 {
    if !is_under(path, COIN) || is_under(path, "/obj/item/coin/antagtoken") {
        return 0;
    }
    // Per sheet (`value_per_unit * SHEET_MATERIAL_AMOUNT`) / 5.
    const MATERIALS: [(&str, u32); 12] = [
        ("gold", 25),
        ("silver", 10),
        ("diamond", 100),
        ("plasma", 40),
        ("uranium", 20),
        ("titanium", 25),
        ("bananium", 200),
        ("adamantine", 100),
        ("mythril", 300),
        ("plastic", 5),
        ("runite", 120),
        ("iron", 1),
    ];
    let material = path
        .strip_prefix("/obj/item/coin/")
        .and_then(|rest| rest.split('/').next())
        .unwrap_or("");
    MATERIALS
        .iter()
        .find(|(m, _)| *m == material)
        .map_or(0, |(_, v)| *v)
}

/// `/obj/item/poker_chip/*::redeem_value`.
pub fn poker_chip_value(path: &str) -> u32 {
    if !is_under(path, POKER_CHIP) {
        return 0;
    }
    match path.rsplit('/').next().unwrap_or("") {
        "white_black" => 1,
        "white_blue" => 10,
        "red" => 25,
        "blue" => 100,
        "green" => 250,
        "black" => 1000,
        _ => 0,
    }
}

/// tg `iscash`: coins, space cash, holochips, poker chips.
pub fn is_cash(world: &World, item: Entity) -> bool {
    world.get::<DmItem>(item).is_some_and(|i| {
        is_under(&i.path, SPACECASH)
            || is_under(&i.path, COIN)
            || is_under(&i.path, POKER_CHIP)
            || is_under(&i.path, HOLOCHIP)
    })
}

/// tg `get_item_credit_value`: what `item` is worth as money (0 for anything else).
pub fn credit_value(world: &World, item: Entity) -> i64 {
    if let Some(chip) = world.get::<Holochip>(item) {
        return i64::from(chip.credits);
    }
    let Some(path) = world.get::<DmItem>(item).map(|i| i.path.as_str()) else {
        return 0;
    };
    if is_under(path, SPACECASH) {
        let amount = world.get::<Stack>(item).map_or(1, |s| s.amount);
        return i64::from(bill_value(path)) * i64::from(amount);
    }
    i64::from(coin_value(path).max(poker_chip_value(path)))
}

/// The sides of a coin type (`sideslist`).
fn coin_sides(path: &str) -> &'static [&'static str] {
    if is_under(path, "/obj/item/coin/twoheaded") {
        &["heads"]
    } else if is_under(path, "/obj/item/coin/antagtoken") {
        &["valid", "salad"]
    } else if is_under(path, "/obj/item/coin/eldritch") {
        &["heretic", "blade"]
    } else {
        &["heads", "tails"]
    }
}

fn side_name(side: &str) -> &str {
    match side {
        "heads" => "орёл",
        "tails" => "решка",
        other => other,
    }
}

/// Money gets its state when made (map or runtime): `holochip/Initialize`, `coin/Initialize`.
fn on_item_added(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let Ok(item) = items.get(add.entity) else {
        return;
    };
    let path = &item.path;
    let entity = add.entity;
    if is_under(path, HOLOCHIP) {
        let credits = if path.ends_with("/thousand") { 1000 } else { 1 };
        // A spawner that sets the sum itself ([`make_holochip`]) wins.
        commands
            .entity(entity)
            .insert_if_new((Holochip { credits }, Money));
        commands.queue(move |world: &mut World| refresh_holochip(world, entity));
    } else if is_under(path, SPACECASH) {
        commands.entity(entity).insert(Money);
    } else if is_under(path, COIN) {
        let sides = coin_sides(path);
        commands.queue(move |world: &mut World| {
            let side = world
                .get_resource_mut::<MoneyState>()
                .map_or(sides[0], |mut s| {
                    sides[(s.rng.next_u64() % sides.len() as u64) as usize]
                });
            if world.get_entity(entity).is_err() {
                return;
            }
            world.entity_mut(entity).insert((
                Coin {
                    side,
                    ready_at: 0.0,
                },
                Money,
            ));
            set_state(world, entity, &format!("coin_{side}"));
        });
    }
}

fn set_state(world: &mut World, entity: Entity, state: &str) {
    crate::obj_damage::set_icon_state(world, entity, state);
}

/// Gives `chip` (a spawned `/obj/item/holochip`) its sum and look.
pub fn make_holochip(world: &mut World, chip: Entity, credits: u32) {
    world.entity_mut(chip).insert((Holochip { credits }, Money));
    refresh_holochip(world, chip);
}

/// A new holochip of `credits` in `mob`'s hands (at its feet when they are full).
pub fn give_holochip(world: &mut World, mob: Entity, credits: u32) -> Option<Entity> {
    if credits == 0 {
        return None;
    }
    let chip = crate::construction::spawn_detached(world, HOLOCHIP)?;
    world.entity_mut(chip).insert(Networked);
    make_holochip(world, chip, credits);
    if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
        crate::storage::put_in_hands(world, mob, &inventory, chip);
    } else if let Some(pos) = crate::chem::map_pos(world, mob) {
        crate::storage::take_out(world, chip, pos);
    }
    Some(chip)
}

/// `holochip/update_name`, `update_icon_state`, `update_overlays`.
pub fn refresh_holochip(world: &mut World, chip: Entity) {
    let Some(credits) = world.get::<Holochip>(chip).map(|c| c.credits) else {
        return;
    };
    world.entity_mut(chip).insert(Name::new(format!(
        "голочип номиналом {credits}{MONEY_SYMBOL}"
    )));
    let (state, color) = holochip_look(credits);
    set_state(world, chip, state);
    let Some(icon) = world.get::<DmAtom>(chip).map(|a| a.icon.clone()) else {
        return;
    };
    let id = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(&icon, &format!("{state}-color")));
    if let (Some(id), Some(mut appearance)) = (id, world.get_mut::<Appearance>(chip)) {
        let mut overlay = Overlay::new(id);
        overlay.color = Some(color);
        appearance.overlays = vec![overlay];
    }
}

/// The icon state and overlay colour of a holochip of `credits`.
pub fn holochip_look(credits: u32) -> (&'static str, [u8; 4]) {
    let (state, rounded) = match credits {
        0..1_000 => ("holochip", credits),
        1_000..1_000_000 => ("holochip_kilo", credits / 1_000),
        1_000_000..1_000_000_000 => ("holochip_mega", credits / 1_000_000),
        _ => ("holochip_giga", credits / 1_000_000_000),
    };
    let hex: u32 = match rounded {
        0..5 => 0x8E2E38,
        5..10 => 0x914792,
        10..20 => 0xBF5E0A,
        20..50 => 0x358F34,
        // `COLOR_SLIME_METAL`.
        50..100 => 0x676767,
        100..200 => 0x009D9B,
        200..500 => 0x0153C1,
        _ => 0x2C2C2C,
    };
    let [_, r, g, b] = hex.to_be_bytes();
    (state, [r, g, b, 255])
}

/// Examine lines of money and ID cards (from `crate::cargo::examine_lines`).
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(Span, String)>) {
    if let Some(chip) = world.get::<Holochip>(target) {
        lines.push((
            Span::Notice,
            format!("В нём {}{MONEY_SYMBOL}.", chip.credits),
        ));
        lines.push((Span::Notice, "Alt-ЛКМ, чтобы разделить.".into()));
        return;
    }
    if let Some(path) = world.get::<DmItem>(target).map(|i| i.path.as_str()) {
        if is_under(path, SPACECASH) {
            // `spacecash/update_desc`.
            let worth = credit_value(world, target);
            lines.push((
                Span::Plain,
                format!("Стоит {worth}{MONEY_SYMBOL} в общей сумме."),
            ));
            return;
        }
        if is_under(path, COIN) {
            lines.push((
                Span::Info,
                format!("Стоит {}{MONEY_SYMBOL}.", coin_value(path)),
            ));
            return;
        }
    }
    let (Some(card), Some(&BankAccountId(id))) = (
        world.get::<IdCard>(target),
        world.get::<BankAccountId>(target),
    ) else {
        return;
    };
    let Some(economy) = world.get_resource::<Economy>() else {
        return;
    };
    let Some(account) = economy.account(id) else {
        return;
    };
    lines.push((
        Span::Plain,
        format!(
            "Аккаунт привязанный к ID-карте принадлежит '{}' и отображает баланс в размере {}{MONEY_SYMBOL}.",
            account.holder, account.balance
        ),
    ));
    if card.access.iter().any(|a| a == "command")
        && let Some(dept) = economy.department(&account.paycheck_department)
    {
        lines.push((
            Span::Plain,
            format!(
                "{} привязанный к ID-карте отображает баланс {}{MONEY_SYMBOL}.",
                dept.holder, dept.balance
            ),
        ));
    }
}

fn name(world: &World, e: Entity) -> String {
    crate::examine::name_of(world, e)
}

/// The held item on a target: holochips merge, money goes onto an ID card (either way round), a
/// money bag empties into one. True when handled.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    _mob: Entity,
    held: Option<Entity>,
    target: Entity,
) -> bool {
    let Some(held) = held else {
        return false;
    };
    if held == target {
        return false;
    }
    // `holochip/item_interaction`.
    if let (Some(a), Some(b)) = (
        world.get::<Holochip>(held).copied(),
        world.get::<Holochip>(target).copied(),
    ) {
        world.entity_mut(target).insert(Holochip {
            credits: b.credits.saturating_add(a.credits),
        });
        world.despawn(held);
        refresh_holochip(world, target);
        tell(world, client, Span::Notice, "объединено!");
        return true;
    }
    let card_target = world.get::<IdCard>(target).is_some();
    if card_target && is_cash(world, held) {
        insert_money(world, client, target, held);
        return true;
    }
    // `card/id/interact_with_atom`.
    if world.get::<IdCard>(held).is_some() && is_cash(world, target) {
        insert_money(world, client, held, target);
        return true;
    }
    let bag = world
        .get::<DmItem>(held)
        .is_some_and(|i| is_under(&i.path, MONEY_BAG));
    if card_target && bag {
        mass_insert_money(world, client, target, held);
        return true;
    }
    false
}

/// `card/id/insert_money`.
pub fn insert_money(world: &mut World, client: Entity, card: Entity, money: Entity) -> bool {
    let account = world.get::<BankAccountId>(card).map(|a| a.0).filter(|&id| {
        world
            .get_resource::<Economy>()
            .is_some_and(|e| e.account(id).is_some())
    });
    let Some(account) = account else {
        let text = format!(
            "У {} нет привязанного аккаунта, на который можно было бы внести {}!",
            name(world, card),
            name(world, money)
        );
        tell(world, client, Span::Warning, text);
        return false;
    };
    let worth = credit_value(world, money);
    if worth <= 0 {
        let text = format!("{} кажется, ничего не стоит!", name(world, money));
        tell(world, client, Span::Warning, text);
        return false;
    }
    let physical = world.get::<Holochip>(money).is_none();
    let text = if physical {
        format!(
            "Вы запихиваете {} в {}. Деньги исчезают в клубе блюспейс-дыма, пополняя счёт на {worth}{MONEY_SYMBOL}.",
            name(world, money),
            name(world, card)
        )
    } else {
        format!(
            "Вы вставляете {} в {}, пополняя счёт на {worth}{MONEY_SYMBOL}.",
            name(world, money),
            name(world, card)
        )
    };
    let balance = {
        let mut economy = world.resource_mut::<Economy>();
        let a = economy.account_mut(account).expect("checked above");
        a.adjust_money(worth);
        a.balance
    };
    world.despawn(money);
    tell(world, client, Span::Notice, text);
    tell(
        world,
        client,
        Span::Notice,
        format!("Текущий баланс счёта: {balance}{MONEY_SYMBOL}."),
    );
    true
}

/// `card/id/mass_insert_money` with a money bag's contents.
fn mass_insert_money(world: &mut World, client: Entity, card: Entity, bag: Entity) {
    let account = world.get::<BankAccountId>(card).map(|a| a.0).filter(|&id| {
        world
            .get_resource::<Economy>()
            .is_some_and(|e| e.account(id).is_some())
    });
    let Some(account) = account else {
        let text = format!(
            "{} нет привязанного аккаунта для пополнения!",
            name(world, card)
        );
        tell(world, client, Span::Warning, text);
        return;
    };
    let contents: Vec<Entity> = world
        .get::<crate::storage::Stored>(bag)
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    if contents.is_empty() {
        return;
    }
    let total: i64 = contents.iter().map(|&e| credit_value(world, e)).sum();
    if let Some(a) = world.resource_mut::<Economy>().account_mut(account) {
        a.adjust_money(total);
    }
    for item in contents {
        if world.get_entity(item).is_ok() {
            world.despawn(item);
        }
    }
    if total <= 0 {
        return;
    }
    tell(
        world,
        client,
        Span::Notice,
        format!(
            "Вы вставляете содержимое на карту! Они исчезают в клубах блюспейс-дыма, пополняя счёт на {total}{MONEY_SYMBOL}. к привязанному аккаунту."
        ),
    );
}

/// `click_alt` of a holochip (split) and an ID card with an account (withdraw). True when
/// handled; both need the thing in the clicker's own hands or slots (`loc == user`).
pub fn click_alt(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    let own = world.get::<CarriedBy>(target).map(|c| c.0) == Some(mob);
    if let Some(chip) = world.get::<Holochip>(target).copied() {
        if !own {
            tell(
                world,
                client,
                Span::Warning,
                "Вы должны держать голочип, чтобы продолжить!",
            );
            return true;
        }
        let prompt = ask_world(
            world,
            Ask::new(
                client,
                mob,
                "Голочип",
                format!(
                    "Сколько кредитов вы хотите разделить с голочипа? (Макс: {}{MONEY_SYMBOL}.)",
                    chip.credits
                ),
                PromptKind::Number {
                    default: 0.0,
                    min: 0.0,
                    max: f64::from(chip.credits),
                    round: true,
                },
            ),
        );
        world.entity_mut(prompt).insert(SplitFor(target));
        return true;
    }
    if world.get::<IdCard>(target).is_none() {
        return false;
    }
    // `departmental_budget/click_alt`: keeps the vault's bank machine useful.
    if super::economy::is_departmental_card(world, target) {
        tell(
            world,
            client,
            Span::Warning,
            "Снятие средств не совместимо с данным дизайном карты.",
        );
        return true;
    }
    let Some(balance) = world.get::<BankAccountId>(target).and_then(|a| {
        world
            .get_resource::<Economy>()?
            .account(a.0)
            .map(|a| a.balance)
    }) else {
        return false;
    };
    if !own {
        tell(
            world,
            client,
            Span::Warning,
            "Вы должны держать ID-карту, чтобы продолжить!",
        );
        return true;
    }
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Вывести средства",
            format!("Сколько вы хотите вывести? (Макс: {balance}{MONEY_SYMBOL}.)"),
            PromptKind::Number {
                default: 0.0,
                min: 0.0,
                max: balance.max(0) as f64,
                round: true,
            },
        ),
    );
    world.entity_mut(prompt).insert(WithdrawFor(target));
    true
}

fn can_act(world: &World, mob: Entity) -> bool {
    world.get::<Stat>(mob) == Some(&Stat::Conscious)
        && !crate::status_effects::incapacitation(world, mob).incapacitated
}

/// The withdrawal and split answers, revalidated: the thing still in the actor's own hands or
/// slots, the actor able to act, the sum still there.
fn prompt_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in answers {
        let withdraw = world.get::<WithdrawFor>(prompt).map(|w| w.0);
        let split = world.get::<SplitFor>(prompt).map(|s| s.0);
        let Some(thing) = withdraw.or(split) else {
            continue;
        };
        let Some(PromptAnswer::Number(n)) = answer else {
            continue;
        };
        let amount = n.floor();
        if amount < 1.0
            || world.get::<CarriedBy>(thing).map(|c| c.0) != Some(actor)
            || !can_act(world, actor)
        {
            continue;
        }
        if withdraw.is_some() {
            withdraw_money(world, client, actor, thing, amount as i64);
        } else {
            split_holochip(
                world,
                client,
                actor,
                thing,
                amount.min(f64::from(u32::MAX)) as u32,
            );
        }
    }
}

/// The rest of `card/id/click_alt`: the sum off the account into a holochip in hand.
fn withdraw_money(world: &mut World, client: Entity, mob: Entity, card: Entity, amount: i64) {
    let Some(account) = world.get::<BankAccountId>(card).map(|a| a.0) else {
        return;
    };
    let result = world.get_resource_mut::<Economy>().and_then(|mut e| {
        let a = e.account_mut(account)?;
        Some(if a.adjust_money(-amount) {
            Ok(())
        } else {
            Err(amount - a.balance)
        })
    });
    match result {
        Some(Ok(())) => {
            give_holochip(world, mob, amount.min(i64::from(u32::MAX)) as u32);
            tell(
                world,
                client,
                Span::Notice,
                format!("Вы выводите {amount}{MONEY_SYMBOL} в голочип."),
            );
        }
        Some(Err(difference)) => bank_card_talk(
            world,
            account,
            &format!(
                "ОШИБКА: Для вывода средств с привязанного аккаунта требуется на {difference} кр. больше."
            ),
        ),
        None => {}
    }
}

/// `holochip/click_alt` -> `spend(split_amount, TRUE)`: a new chip with the split sum (all of it
/// when asked for more), the old one gone when empty.
fn split_holochip(world: &mut World, client: Entity, mob: Entity, chip: Entity, amount: u32) {
    let Some(credits) = world.get::<Holochip>(chip).map(|c| c.credits) else {
        return;
    };
    let taken = amount.min(credits);
    if taken == credits {
        world.despawn(chip);
    } else {
        world.entity_mut(chip).insert(Holochip {
            credits: credits - taken,
        });
        refresh_holochip(world, chip);
    }
    give_holochip(world, mob, taken);
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы разделили {amount}{MONEY_SYMBOL} в новый голочип."),
    );
}

/// `holochip/emp_act`: `prob(60 / severity)` it fizzles away.
fn holochip_emps(world: &mut World, mut cursor: Local<MessageCursor<crate::emp::EmpAct>>) {
    for ev in crate::emp::emp_acts(world, &mut cursor) {
        if !ev.hits_self() || world.get::<Holochip>(ev.target).is_none() {
            continue;
        }
        let wipe = ev.by_severity(60.0);
        if !world.resource_mut::<crate::emp::EmpRng>().prob(wipe) {
            continue;
        }
        if let Some(pos) = crate::chem::map_pos(world, ev.target) {
            let text = format!("{} шипит и исчезает!", name(world, ev.target));
            let clients = crate::obj_damage::clients_in_range(
                world,
                pos,
                crate::health::DEFAULT_MESSAGE_RANGE,
                &[],
            );
            world.write_message(ss13_net::chat::server::SendChat {
                clients,
                message: crate::chem::line(Span::Warning, &text),
            });
        }
        world.despawn(ev.target);
    }
}

/// `coin/attack_self`: a new side, the sound, the landing announced later. True for coins.
pub fn coin_attack_self(world: &mut World, mob: Entity, item: Entity) -> bool {
    let Some(coin) = world.get::<Coin>(item).cloned() else {
        return false;
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    if coin.ready_at > now {
        return true;
    }
    let path = world
        .get::<DmItem>(item)
        .map(|i| i.path.clone())
        .unwrap_or_default();
    let sides = coin_sides(&path);
    let holder = world.get::<CarriedBy>(item).map(|c| c.0);
    let side = {
        let mut state = world.resource_mut::<MoneyState>();
        let side = sides[(state.rng.next_u64() % sides.len() as u64) as usize];
        state.flips.push(Flip {
            coin: item,
            user: mob,
            at: now + FLIP_SECS,
            holder,
        });
        side
    };
    world.entity_mut(item).insert(Coin {
        side,
        ready_at: now + FLIP_SECS,
    });
    set_state(world, item, &format!("coin_{side}"));
    if let Some(pos) = crate::chem::map_pos(world, mob) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, COINFLIP_SOUND, 50.0, true),
        );
    }
    true
}

/// After `sleep(1.5 SECONDS)`: `if(loc == oldloc && user && !user.incapacitated)` the landing.
fn land_flips(world: &mut World) {
    let Some(state) = world.get_resource::<MoneyState>() else {
        return;
    };
    if state.flips.is_empty() {
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    let due: Vec<Flip> = {
        let mut state = world.resource_mut::<MoneyState>();
        let (due, waiting): (Vec<Flip>, Vec<Flip>) =
            state.flips.drain(..).partition(|f| f.at <= now);
        state.flips = waiting;
        due
    };
    for flip in due {
        let Some(side) = world.get::<Coin>(flip.coin).map(|c| c.side) else {
            continue;
        };
        let holder = world.get::<CarriedBy>(flip.coin).map(|c| c.0);
        if holder != flip.holder
            || world.get_entity(flip.user).is_err()
            || crate::status_effects::incapacitation(world, flip.user).incapacitated
        {
            continue;
        }
        let coin = name(world, flip.coin);
        let user = name(world, flip.user);
        let side = side_name(side);
        crate::chem::visible(
            world,
            flip.user,
            &format!("Вы подбрасываете {coin}. Выпадает {side}."),
            &format!("{user} подбрасывает {coin}. Выпадает {side}."),
            Span::Notice,
        );
    }
}

/// `payment/handle_cardless`: the coins, holochips and space cash `mob` carries pay `cost`;
/// what is over comes back as a holochip. False (nothing taken) when they do not cover it.
pub fn pay_with_cash(world: &mut World, mob: Entity, cost: i64) -> bool {
    if cost <= 0 {
        return true;
    }
    let mut money = world.query_filtered::<Entity, With<Money>>();
    let mut carried: Vec<Entity> = money
        .iter(world)
        .filter(|&e| holder_of(world, e) == Some(mob))
        .collect();
    carried.sort_unstable();
    let mut total = 0;
    let mut used = Vec::new();
    for item in carried {
        if total >= cost {
            break;
        }
        total += credit_value(world, item);
        used.push(item);
    }
    if total < cost {
        return false;
    }
    for item in used {
        world.despawn(item);
    }
    let change = total - cost;
    if change > 0 {
        give_holochip(world, mob, change.min(i64::from(u32::MAX)) as u32);
    }
    if let Some(pos) = crate::chem::map_pos(world, mob) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, CASH_SOUND, 20.0, true),
        );
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_follow_tg() {
        assert_eq!(bill_value("/obj/item/stack/spacecash/c1000"), 1000);
        assert_eq!(bill_value("/obj/item/stack/spacecash"), 0);
        assert_eq!(bill_value("/obj/item/stack/sheet/iron"), 0);
        assert_eq!(coin_value("/obj/item/coin/gold"), 25);
        assert_eq!(coin_value("/obj/item/coin/gold/doubloon"), 25);
        assert_eq!(coin_value("/obj/item/coin/mythril"), 300);
        assert_eq!(coin_value("/obj/item/coin/iron"), 1);
        assert_eq!(coin_value("/obj/item/coin/antagtoken"), 0);
        assert_eq!(coin_value("/obj/item/coin/eldritch"), 0);
        assert_eq!(coin_value("/obj/item/coin"), 0);
        assert_eq!(poker_chip_value("/obj/item/poker_chip/green"), 250);
        assert_eq!(poker_chip_value("/obj/item/poker_chip"), 0);
    }

    #[test]
    fn holochip_looks_by_sum() {
        assert_eq!(holochip_look(3), ("holochip", [0x8E, 0x2E, 0x38, 255]));
        assert_eq!(holochip_look(250).0, "holochip");
        assert_eq!(holochip_look(250).1, [0x01, 0x53, 0xC1, 255]);
        assert_eq!(
            holochip_look(1000),
            ("holochip_kilo", [0x8E, 0x2E, 0x38, 255])
        );
        assert_eq!(holochip_look(25_000).1, [0x35, 0x8F, 0x34, 255]);
        assert_eq!(holochip_look(2_000_000).0, "holochip_mega");
        assert_eq!(holochip_look(3_000_000_000).0, "holochip_giga");
    }

    #[test]
    fn coin_sides_by_type() {
        assert_eq!(coin_sides("/obj/item/coin/twoheaded"), &["heads"]);
        assert_eq!(coin_sides("/obj/item/coin/eldritch"), &["heretic", "blade"]);
        assert_eq!(coin_sides("/obj/item/coin/gold"), &["heads", "tails"]);
    }

    fn world() -> World {
        let mut world = World::new();
        world.insert_resource(Economy::new(3));
        world.init_resource::<Messages<ss13_net::chat::server::SendChat>>();
        world
    }

    fn item(world: &mut World, path: &str) -> Entity {
        world
            .spawn(DmItem {
                path: path.to_owned(),
                ..default()
            })
            .id()
    }

    #[test]
    fn money_goes_onto_the_card() {
        let mut w = world();
        let id = w
            .resource_mut::<Economy>()
            .open_account("Ассистент", 10, "CIV");
        let card = w
            .spawn((IdCard { access: Vec::new() }, BankAccountId(id)))
            .id();
        let client = w.spawn_empty().id();
        let bills = item(&mut w, "/obj/item/stack/spacecash/c100");
        w.entity_mut(bills).insert(Stack { amount: 3 });
        assert_eq!(credit_value(&w, bills), 300);
        assert!(insert_money(&mut w, client, card, bills));
        assert!(w.get_entity(bills).is_err());
        let chip = item(&mut w, HOLOCHIP);
        w.entity_mut(chip).insert(Holochip { credits: 45 });
        assert!(item_interaction(&mut w, client, client, Some(card), chip));
        assert_eq!(w.resource::<Economy>().account(id).unwrap().balance, 345);
        // Worthless things and cards without an account refuse.
        let token = item(&mut w, "/obj/item/coin/antagtoken");
        assert!(!insert_money(&mut w, client, card, token));
        assert!(w.get_entity(token).is_ok());
        let blank = w.spawn(IdCard { access: Vec::new() }).id();
        let coin = item(&mut w, "/obj/item/coin/gold");
        assert!(!insert_money(&mut w, client, blank, coin));
        assert!(insert_money(&mut w, client, card, coin));
        assert_eq!(w.resource::<Economy>().account(id).unwrap().balance, 370);
    }

    #[test]
    fn holochips_merge() {
        let mut w = world();
        let client = w.spawn_empty().id();
        let a = item(&mut w, HOLOCHIP);
        let b = item(&mut w, HOLOCHIP);
        w.entity_mut(a).insert(Holochip { credits: 30 });
        w.entity_mut(b).insert(Holochip { credits: 12 });
        assert!(item_interaction(&mut w, client, client, Some(a), b));
        assert!(w.get_entity(a).is_err());
        assert_eq!(w.get::<Holochip>(b), Some(&Holochip { credits: 42 }));
    }

    #[test]
    fn carried_cash_pays_only_when_it_covers_the_cost() {
        let mut w = world();
        let mob = w.spawn_empty().id();
        let chips: Vec<Entity> = [50, 50]
            .into_iter()
            .map(|credits| {
                let chip = item(&mut w, HOLOCHIP);
                w.entity_mut(chip)
                    .insert((Holochip { credits }, Money, CarriedBy(mob)));
                chip
            })
            .collect();
        // Someone else's money does not count.
        let other = w.spawn_empty().id();
        let theirs = item(&mut w, HOLOCHIP);
        w.entity_mut(theirs)
            .insert((Holochip { credits: 500 }, Money, CarriedBy(other)));
        assert!(!pay_with_cash(&mut w, mob, 101));
        assert!(chips.iter().all(|&c| w.get_entity(c).is_ok()));
        assert!(pay_with_cash(&mut w, mob, 70));
        assert!(chips.iter().all(|&c| w.get_entity(c).is_err()));
        assert!(w.get_entity(theirs).is_ok());
    }
}
