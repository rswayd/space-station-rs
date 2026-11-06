//! Mining vouchers and the mining point transfer card (server), after tg `/obj/item/mining_voucher`,
//! `/obj/item/card/mining_point_card` (`order_computer/mining_order.dm`) and
//! `/datum/element/voucher_redeemer` with the mining console's `/datum/voucher_set/mining`
//! sets ([`ss13_mapload::DmOrderConsole::vouchers`]).
//!
//! - A voucher used on the mining equipment order console asks which set to take (tg's radial
//!   menu, here a list); the chosen set's items appear on the console's tile and the voucher is
//!   used up. The choice counts only while the user still holds the voucher, is conscious and
//!   not incapacitated, and the console is within reach.
//! - An ID swiped over the point card asks the direction ("Transfer Card → ID" or
//!   "ID → Transfer Card"), then the amount; at most what the source has moves between the
//!   card's `points` and the mining points of the ID's account.
// tg checks nothing after the point card's two inputs; here the card must still be held or
// within reach and the ID still carried by the user, so an open prompt cannot move points later
// from elsewhere.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmItem, DmName, DmOrderConsole};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};

use crate::cargo::economy::{Economy, account_of};
use crate::examine::Line;
use crate::health::Stat;
use crate::inventory::{CarriedBy, IdCard, Inventory};
use crate::obj_damage::is_under;
use crate::sound::{PlaySound, playsound};
use crate::tools::tell;

const VOUCHER: &str = "/obj/item/mining_voucher";
const POINT_CARD: &str = "/obj/item/card/mining_point_card";
const MINING_CONSOLE: &str = ss13_mapload::mining::MINING_CONSOLE;
const TO_USER_ID: &str = "Transfer Card → ID";
const TO_POINT_CARD: &str = "ID → Transfer Card";
/// `tgui_input_number`'s default `max_value`.
const MAX_INPUT: f64 = 10000.0;

/// A point card's `points` (none yet: 0).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PointCard {
    pub points: i64,
}

/// On a voucher prompt: what it redeems where.
#[derive(Component, Clone, Copy, Debug)]
struct VoucherPrompt {
    voucher: Entity,
    console: Entity,
}

/// On a point card prompt: the card, the ID and, once chosen, the direction (`true`: to the
/// ID).
#[derive(Component, Clone, Copy, Debug)]
struct TransferPrompt {
    card: Entity,
    id: Entity,
    to_id: Option<bool>,
}

pub(super) struct EquipmentPlugin;

impl Plugin for EquipmentPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            answers
                .in_set(ss13_tgui::server::UiSystems::Handle)
                .after(ss13_tgui::prompt::PromptSystems::Answer),
        );
    }
}

fn path_is(world: &World, e: Entity, root: &str) -> bool {
    world
        .get::<DmItem>(e)
        .is_some_and(|i| is_under(&i.path, root))
}

fn name(world: &World, e: Entity) -> String {
    world
        .get::<DmName>(e)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default()
}

/// A voucher on the mining console, an ID on a point card. True when handled.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    redeem_voucher(world, client, mob, held, target)
        || swipe_point_card(world, client, mob, held, target)
}

/// `voucher_redeemer/redeem_voucher`.
fn redeem_voucher(
    world: &mut World,
    client: Entity,
    mob: Entity,
    voucher: Entity,
    console: Entity,
) -> bool {
    if !path_is(world, voucher, VOUCHER) {
        return false;
    }
    let is_mining =
        crate::construction::path_of(world, console).is_some_and(|p| is_under(&p, MINING_CONSOLE));
    let Some(sets) = world
        .get::<DmOrderConsole>(console)
        .filter(|_| is_mining)
        .map(|c| c.vouchers.clone())
    else {
        return false;
    };
    if sets.is_empty() {
        return false;
    }
    let items = sets.iter().map(|s| s.name.clone()).collect();
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Mining Voucher",
            "Redeem a set",
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(VoucherPrompt { voucher, console });
    true
}

/// `/obj/item/card/mining_point_card/item_interaction` with an ID.
fn swipe_point_card(
    world: &mut World,
    client: Entity,
    mob: Entity,
    id: Entity,
    card: Entity,
) -> bool {
    if !path_is(world, card, POINT_CARD) || world.get::<IdCard>(id).is_none() {
        return false;
    }
    tell(world, client, Span::Notice, "starting transfer");
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Mining Points Transfer",
            "To ID (from card) or to card (from ID)?",
            PromptKind::Alert {
                buttons: vec![TO_USER_ID.to_owned(), TO_POINT_CARD.to_owned()],
                autofocus: true,
            },
        ),
    );
    world.entity_mut(prompt).insert(TransferPrompt {
        card,
        id,
        to_id: None,
    });
    true
}

fn able(world: &World, mob: Entity) -> bool {
    world.get::<Stat>(mob) == Some(&Stat::Conscious)
        && !crate::status_effects::incapacitation(world, mob).incapacitated
}

fn carried_by(world: &World, item: Entity, mob: Entity) -> bool {
    world.get::<CarriedBy>(item).is_some_and(|c| c.0 == mob)
}

fn answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answered: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in answered {
        if let Some(v) = world.get::<VoucherPrompt>(prompt).copied() {
            if let Some(PromptAnswer::Item(i)) = answer {
                redeem(world, client, actor, v.voucher, v.console, i);
            }
            continue;
        }
        let Some(t) = world.get::<TransferPrompt>(prompt).copied() else {
            continue;
        };
        let card_reachable =
            carried_by(world, t.card, actor) || crate::storage::in_reach(world, actor, t.card);
        if !able(world, actor) || !card_reachable || !carried_by(world, t.id, actor) {
            continue;
        }
        match (t.to_id, answer) {
            (None, Some(PromptAnswer::Button(button))) => {
                ask_amount(world, client, actor, t, button == 0);
            }
            (Some(to_id), Some(PromptAnswer::Number(amount))) => {
                transfer(world, client, t.card, t.id, to_id, amount);
            }
            _ => {}
        }
    }
}

/// `check_menu`, then set `index` of the console's `spawn_set(source.drop_location())`. True
/// when redeemed.
pub fn redeem(
    world: &mut World,
    client: Entity,
    actor: Entity,
    voucher: Entity,
    console: Entity,
    index: usize,
) -> bool {
    let holding = world
        .get::<Inventory>(actor)
        .is_some_and(|inv| inv.hands.contains(&Some(voucher)));
    if !able(world, actor)
        || !holding
        || !path_is(world, voucher, VOUCHER)
        || !crate::storage::in_reach(world, actor, console)
    {
        return false;
    }
    let Some(set) = world
        .get::<DmOrderConsole>(console)
        .and_then(|c| c.vouchers.get(index).cloned())
    else {
        return false;
    };
    let Some(&pos) = world.get::<TilePos>(console) else {
        return false;
    };
    for item in &set.items {
        if crate::construction::spawn_obj(world, item, pos).is_none() {
            warn!("mining voucher: no prototype for {item}");
        }
    }
    playsound(
        world,
        PlaySound::new(pos, "sound/machines/card_slide.ogg", 33.0, true),
    );
    tell(
        world,
        client,
        Span::Notice,
        format!("redeemed {}", set.name.to_lowercase()),
    );
    info!("{actor:?} redeemed the mining voucher set {}", set.name);
    world.despawn(voucher);
    true
}

fn account_points(world: &World, id: Entity) -> Option<(u32, i64)> {
    let account = account_of(world, id)?;
    let points = world
        .get_resource::<Economy>()?
        .account(account)?
        .mining_points;
    Some((account, points))
}

fn ask_amount(world: &mut World, client: Entity, actor: Entity, t: TransferPrompt, to_id: bool) {
    // tg reads `registered_account` unchecked; an ID without an account moves nothing.
    let Some((_, id_points)) = account_points(world, t.id) else {
        return;
    };
    let card_points = world.get::<PointCard>(t.card).map_or(0, |c| c.points);
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            actor,
            "Transfer Points",
            format!(
                "How much do you want to transfer? ID Balance: {id_points}, Card Balance: {card_points}"
            ),
            PromptKind::Number {
                default: 0.0,
                min: 0.0,
                max: MAX_INPUT,
                round: true,
            },
        ),
    );
    world.entity_mut(prompt).insert(TransferPrompt {
        to_id: Some(to_id),
        ..t
    });
}

/// The `TO_USER_ID` / `TO_POINT_CARD` branches: at most what the source holds moves.
pub fn transfer(
    world: &mut World,
    client: Entity,
    card: Entity,
    id: Entity,
    to_id: bool,
    amount: f64,
) {
    if !amount.is_finite() || amount < 1.0 {
        return;
    }
    let Some((account, id_points)) = account_points(world, id) else {
        return;
    };
    let card_points = world.get::<PointCard>(card).map_or(0, |c| c.points);
    let wanted = amount.floor() as i64;
    let moved = if to_id {
        wanted.min(card_points)
    } else {
        wanted.min(id_points)
    }
    .max(0);
    let delta = if to_id { moved } else { -moved };
    if let Some(mut economy) = world.get_resource_mut::<Economy>()
        && let Some(a) = economy.account_mut(account)
    {
        a.mining_points += delta;
    } else {
        return;
    }
    world.entity_mut(card).insert(PointCard {
        points: card_points - delta,
    });
    let (card_name, id_name) = (name(world, card), name(world, id));
    let text = if to_id {
        format!("You transfer {moved} mining points from {card_name} to {id_name}.")
    } else {
        format!("You transfer {moved} mining points from {id_name} to {card_name}.")
    };
    tell(world, client, Span::Notice, text);
}

/// `/obj/item/card/mining_point_card/examine`.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<Line>) {
    if !path_is(world, target, POINT_CARD) {
        return;
    }
    let points = world.get::<PointCard>(target).map_or(0, |c| c.points);
    let s = if points == 1 { "" } else { "s" };
    lines.push((
        Span::Notice,
        format!("There's {points} point{s} on the card."),
    ));
}
