//! ID card stickers (server), after BandaStation `modular_bandastation/id_stickers`
//! (`base.dm`, `card_interactions.dm`, `stickers.dm`).
//!
//! - A sticker (`/obj/item/id_sticker/*`) used on an advanced ID card (in hand or next to the
//!   user) is applied in 2 s: the card wears it as an overlay, examine names it and adds its
//!   line (`id_card_desc`). A card takes one sticker.
//! - Ctrl+shift on a stickered card in reach: in combat mode the sticker is torn off and lost
//!   (the poster rip sound), else it is peeled off in 5 s into the user's hands.
// shortcut: a holographic sticker keeps the colour it was rendered in (no colour picker: GAGS
// icons are rendered at import); the card keeps the sticker's type, name and sprite, and a
// peeled sticker is a fresh one of that type; the PDA painter does not refuse stickered cards.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, IconStateId, Overlay};

use super::server::item_path;
use crate::obj_damage::is_under;
use crate::tools::{Actor, tell};

const STICKER: &str = "/obj/item/id_sticker";
const ADVANCED_CARD: &str = "/obj/item/card/id/advanced";
const RIP_SOUND: &str = "sound/items/poster/poster_ripped.ogg";
/// `do_after(user, 2 SECONDS)`, `do_after(user, 5 SECONDS)`.
const APPLY_DS: f32 = 20.0;
const PEEL_DS: f32 = 50.0;

/// `id_card_desc` of the stickers that have one, most specific first.
const CARD_DESCS: &[(&str, &str)] = &[
    ("/donut", "С глазурью!"),
    ("/clown", "HONK!"),
    ("/colored/neon", "Кажется будто она светится."),
    ("/neon", "Кажется будто она светится."),
    ("/missing", "А где?"),
    ("/ouija", "Умеет ли он общаться с призраками?"),
    ("/rainbow", "Одобряемо."),
    (
        "/space",
        "Есть 3 вещи на которые можно смотреть вечно. Это четвёртая.",
    ),
    (
        "/colored/kitty",
        "Так и хочется погладить, жаль это всего-лишь наклейка...",
    ),
    (
        "/kitty",
        "Так и хочется погладить, жаль это всего-лишь наклейка...",
    ),
    ("/cursedmiku", "AYAYA!"),
    ("/colored/snake", "Бегает и бегает..."),
    ("/magic", "Кто-то до сих пор девственник..."),
    ("/terminal", "Эта карта похожа на терминал."),
    ("/jokerge", "Jokerge."),
    ("/boykisser", "Он любит целовать мальчиков."),
];

/// `applied_sticker` of an advanced card: the sticker's type, name and sprite.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct AppliedSticker {
    pub path: String,
    pub name: String,
    pub icon_state: IconStateId,
}

/// The `id_card_desc` of a sticker type.
pub fn card_desc(path: &str) -> Option<&'static str> {
    let rest = path.strip_prefix(STICKER)?;
    CARD_DESCS
        .iter()
        .find(|(sub, _)| is_under(rest, sub))
        .map(|&(_, desc)| desc)
}

fn is_sticker(world: &World, item: Entity) -> bool {
    is_under(&item_path(world, item), STICKER)
}

fn is_advanced_card(world: &World, item: Entity) -> bool {
    is_under(&item_path(world, item), ADVANCED_CARD)
}

/// The `do_after` target: the user for a card in its hands (held things have no tile).
fn work_target(world: &World, mob: Entity, card: Entity) -> Entity {
    if world
        .get::<crate::inventory::CarriedBy>(card)
        .is_some_and(|c| c.0 == mob)
    {
        mob
    } else {
        card
    }
}

/// `card/id/advanced/item_interaction` with a sticker. True when it was one on such a card.
pub(super) fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if !is_sticker(world, held) || !is_advanced_card(world, target) {
        return false;
    }
    if world.get::<AppliedSticker>(target).is_some() {
        tell(
            world,
            client,
            Span::Warning,
            "На ID карте уже есть наклейка, сначала снимите её!",
        );
        return true;
    }
    tell(
        world,
        client,
        Span::Notice,
        "Вы начинаете наносить наклейку на ID карту.",
    );
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    let at = work_target(world, mob, target);
    crate::tools::do_after(
        world,
        actor,
        at,
        APPLY_DS,
        0,
        crate::construction::Job::Art(super::ArtJob::ApplySticker { card: target }),
    );
    true
}

/// `apply_sticker`, after the `do_after`: checked again.
pub(super) fn apply(
    world: &mut World,
    client: Entity,
    mob: Entity,
    card: Entity,
    sticker: Option<Entity>,
) {
    let Some(sticker) = sticker.filter(|&s| world.get_entity(s).is_ok() && is_sticker(world, s))
    else {
        return;
    };
    if world.get_entity(card).is_err()
        || world.get::<AppliedSticker>(card).is_some()
        || !reachable(world, mob, card)
    {
        return;
    }
    let Some(icon_state) = world.get::<Appearance>(sticker).map(|a| a.icon_state) else {
        return;
    };
    let applied = AppliedSticker {
        path: item_path(world, sticker),
        name: crate::chem::name_of(world, sticker),
        icon_state,
    };
    let name = applied.name.clone();
    if let Some(mut inventory) = world.get_mut::<crate::inventory::Inventory>(mob) {
        for hand in inventory.hands.iter_mut() {
            if *hand == Some(sticker) {
                *hand = None;
            }
        }
    }
    world.despawn(sticker);
    if let Some(mut look) = world.get_mut::<Appearance>(card) {
        look.overlays.push(Overlay::new(icon_state));
    }
    world.entity_mut(card).insert(applied);
    refresh_holder(world, card);
    crate::inventory::refresh_look(world, mob);
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы наклеили {name} на ID карту."),
    );
}

fn reachable(world: &World, mob: Entity, card: Entity) -> bool {
    world
        .get::<crate::inventory::CarriedBy>(card)
        .is_some_and(|c| c.0 == mob)
        || crate::storage::in_reach(world, mob, card)
}

fn refresh_holder(world: &mut World, card: Entity) {
    if let Some(holder) = world.get::<crate::inventory::CarriedBy>(card).map(|c| c.0) {
        crate::inventory::refresh_look(world, holder);
    }
}

/// `remove_sticker`: the overlay off the card; what was on it.
fn remove(world: &mut World, card: Entity) -> Option<AppliedSticker> {
    let applied = world.get::<AppliedSticker>(card).cloned()?;
    world.entity_mut(card).remove::<AppliedSticker>();
    if let Some(mut look) = world.get_mut::<Appearance>(card)
        && let Some(i) = look
            .overlays
            .iter()
            .rposition(|o| o.icon_state == applied.icon_state)
    {
        look.overlays.remove(i);
    }
    refresh_holder(world, card);
    Some(applied)
}

/// `card/id/advanced/click_ctrl_shift` from [`crate::click::GiveClick`].
pub(super) fn ctrl_shift_clicks(
    world: &mut World,
    mut clicks: Local<MessageCursor<crate::click::GiveClick>>,
) {
    let Some(messages) = world.get_resource::<Messages<crate::click::GiveClick>>() else {
        return;
    };
    let clicks: Vec<crate::click::GiveClick> = clicks.read(messages).copied().collect();
    for click in clicks {
        if is_advanced_card(world, click.target) && reachable(world, click.mob, click.target) {
            peel(world, click.client, click.mob, click.target);
        }
    }
}

fn peel(world: &mut World, client: Entity, mob: Entity, card: Entity) {
    if crate::status_effects::incapacitation(world, mob).hands_blocked {
        tell(
            world,
            client,
            Span::Warning,
            "Ваши руки должны быть свободны, чтобы сделать это!",
        );
        return;
    }
    if world.get::<AppliedSticker>(card).is_none() {
        tell(world, client, Span::Warning, "На ID карте нет наклейки!");
        return;
    }
    if world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0)
    {
        tell(
            world,
            client,
            Span::Notice,
            "Вы сдираете наклейку с ID карты!",
        );
        super::server::sound_at(world, card, RIP_SOUND, 100.0, true);
        remove(world, card);
        return;
    }
    tell(
        world,
        client,
        Span::Notice,
        "Вы пытаетесь снять наклейку с ID карты...",
    );
    let item = world
        .get::<crate::inventory::Inventory>(mob)
        .and_then(crate::inventory::Inventory::active_item);
    let actor = Actor { client, mob, item };
    let at = work_target(world, mob, card);
    crate::tools::do_after(
        world,
        actor,
        at,
        PEEL_DS,
        0,
        crate::construction::Job::Art(super::ArtJob::PeelSticker { card }),
    );
}

/// The peel's `do_after` is done: the sticker into the user's hands.
pub(super) fn peeled(world: &mut World, client: Entity, mob: Entity, card: Entity) {
    if world.get_entity(card).is_err() || !reachable(world, mob, card) {
        return;
    }
    let Some(applied) = remove(world, card) else {
        return;
    };
    tell(world, client, Span::Notice, "Вы сняли наклейку с ID карты.");
    let Some(pos) = crate::chem::map_pos(world, mob) else {
        return;
    };
    let Some(sticker) = crate::construction::spawn_obj(world, &applied.path, pos) else {
        return;
    };
    if let Some(inventory) = world.get::<crate::inventory::Inventory>(mob).cloned() {
        crate::storage::put_in_hands(world, mob, &inventory, sticker);
    }
}

/// `card/id/advanced/examine` and `update_desc` with a sticker on.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(Span, String)>) {
    let Some(applied) = world.get::<AppliedSticker>(target) else {
        return;
    };
    if let Some(desc) = card_desc(&applied.path) {
        lines.push((Span::Plain, desc.to_owned()));
    }
    lines.push((Span::Plain, format!("На ней {}", applied.name)));
    lines.push((
        Span::Info,
        "Вы можете снять наклейку, используя Ctrl-Shift-Click.".to_owned(),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sticker_lines_follow_the_type_tree() {
        assert_eq!(card_desc("/obj/item/id_sticker/donut"), Some("С глазурью!"));
        assert_eq!(
            card_desc("/obj/item/id_sticker/colored/snake"),
            Some("Бегает и бегает...")
        );
        assert_eq!(card_desc("/obj/item/id_sticker/gold"), None);
        assert_eq!(card_desc("/obj/item/id_sticker/colored"), None);
    }
}
