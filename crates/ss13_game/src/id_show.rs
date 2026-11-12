//! Showing and reading an ID card (server), after tg `/obj/item/card/id/attack_self` and
//! `/obj/item/card/id/examine` (`code/game/objects/items/cards_ids.dm`).

use bevy::prelude::*;
use ss13_shared::chat::Span;

use crate::inventory::IdCard;
use crate::security::name_of;

pub struct IdShowPlugin;

impl Plugin for IdShowPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // Last: the cards with their own use in hand (chameleon, emag) come first.
        app.add_click_handler(ClickPhase::AttackSelf, 4993, |w, c| {
            show(w, c.mob, c.target)
        });
    }
}

/// `attack_self`: everyone around sees the card. True when `item` is an ID card.
// shortcut: no "(НЕСОВЕРШЕННОЛЕТНИЙ)" mark (cards carry no `registered_age`).
pub fn show(world: &mut World, user: Entity, item: Entity) -> bool {
    if world.get::<IdCard>(item).is_none() {
        return false;
    }
    let (who, card) = (name_of(world, user), name_of(world, item));
    crate::chem::visible(
        world,
        user,
        &format!("Вы показываете {card}."),
        &format!("{who} показывает вам: {card}."),
        Span::Notice,
    );
    true
}

/// `examine`: the account the card is registered to.
pub fn examine_lines(world: &World, item: Entity, lines: &mut Vec<(Span, String)>) {
    if world.get::<IdCard>(item).is_none() {
        return;
    }
    let account = crate::cargo::economy::account_of(world, item).and_then(|id| {
        world
            .get_resource::<crate::cargo::economy::Economy>()?
            .account(id)
    });
    match account {
        Some(account) => lines.push((
            Span::Plain,
            format!(
                "Аккаунт привязанный к ID-карте принадлежит '{}' и отображает баланс в размере \
                 {}{}.",
                account.holder,
                account.balance,
                crate::cargo::MONEY_SYMBOL
            ),
        )),
        None => lines.push((
            Span::Notice,
            "Alt-ПКМ, чтобы привязать ID-карту к банковскому счёту.".to_owned(),
        )),
    }
}
