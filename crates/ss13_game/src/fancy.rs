//! Fancy storage (server), after tg `/obj/item/storage/fancy`
//! (`code/game/objects/items/storage/fancy.dm`): donut, egg, candle and cigarette boxes whose
//! sprite shows how many items are left.
//!
//! - Used in hand, a box opens or closes; taking something out or putting something in opens
//!   a closed one (`Entered`/`Exited`).
//! - Open, its sprite is `[base_icon_state][contents.len]` (`update_icon_state`) and the
//!   examine counts what is left; closed, `base_icon_state`.
//! - An empty box used in hand folds into its `foldable_result` (cardboard), which lands in
//!   the same hands; so does a plain cardboard box (`/obj/item/storage/box/attack_self`).
// shortcut: no content overlays (the donuts by flavour, the cigarettes and the lighter of a
// pack, the coffee condiments), no pulling a cigarette out with a right click or into the
// mouth; a box that is always open keeps its mapped sprite.

use bevy::prelude::*;
use ss13_mapload::DmFancy;
use ss13_shared::chat::Span;

use crate::inventory::Inventory;
use crate::storage::Stored;
use crate::tools::tell;

/// `open_status` of a box that is not `FANCY_CONTAINER_ALWAYS_OPEN`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FancyOpen(pub bool);

const ALWAYS_OPEN: u8 = 2;

pub struct FancyPlugin;

impl Plugin for FancyPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::AttackSelf, 2993, |w, c| {
            attack_self(w, c.client, c.mob, c.target)
        })
        .add_systems(Update, contents_changed);
    }
}

fn count(world: &World, item: Entity) -> usize {
    world.get::<Stored>(item).map_or(0, |s| s.items().len())
}

/// `update_icon_state`.
fn refresh(world: &mut World, item: Entity) {
    let Some(dm) = world.get::<DmFancy>(item).cloned() else {
        return;
    };
    if dm.base_icon_state.is_empty() || dm.open_status == ALWAYS_OPEN {
        return;
    }
    let open = world.get::<FancyOpen>(item).is_some_and(|o| o.0);
    let state = if dm.has_open_closed_states && open {
        format!("{}{}", dm.base_icon_state, count(world, item))
    } else {
        dm.base_icon_state
    };
    crate::obj_damage::set_icon_state(world, item, &state);
}

/// `Entered`/`Exited`: a closed box opens, the sprite follows the count.
fn contents_changed(world: &mut World) {
    let mut changed = world.query_filtered::<Entity, (With<DmFancy>, Changed<Stored>)>();
    let boxes: Vec<Entity> = changed.iter(world).collect();
    for item in boxes {
        // The first sight of a mapped box is its `PopulateContents`, not a hand in it.
        let Some(open) = world.get::<FancyOpen>(item).copied() else {
            let start = world
                .get::<DmFancy>(item)
                .is_some_and(|d| d.open_status == 1);
            world.entity_mut(item).insert(FancyOpen(start));
            refresh(world, item);
            continue;
        };
        if !open.0 {
            world.entity_mut(item).insert(FancyOpen(true));
        }
        refresh(world, item);
    }
}

/// `/obj/item/storage/fancy/attack_self`. True when `item` is a fancy box.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let Some(dm) = world.get::<DmFancy>(item).cloned() else {
        return false;
    };
    if dm.open_status != ALWAYS_OPEN {
        let open = world
            .get::<FancyOpen>(item)
            .map_or(dm.open_status == 1, |o| o.0);
        world.entity_mut(item).insert(FancyOpen(!open));
        refresh(world, item);
    }
    // `/obj/item/storage/box/attack_self`: a plain box says why it does not fold.
    let plain_box = dm.base_icon_state.is_empty() && dm.open_status == ALWAYS_OPEN;
    if dm.foldable_result.is_empty() {
        return !plain_box;
    }
    if count(world, item) > 0 {
        if plain_box {
            tell(world, client, Span::Warning, "внутри предметы!");
        }
        return true;
    }
    // Empty: it folds, and the cardboard takes its place in the hands.
    let Some(pos) = crate::explosions::turf_of(world, mob) else {
        return true;
    };
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        for hand in &mut inventory.hands {
            if *hand == Some(item) {
                *hand = None;
            }
        }
    }
    world.entity_mut(item).despawn();
    if let Some(result) = crate::construction::spawn_stack(world, &dm.foldable_result, pos, 1)
        && let Some(inventory) = world.get::<Inventory>(mob).cloned()
    {
        crate::storage::put_in_hands(world, mob, &inventory, result);
    }
    tell(world, client, Span::Notice, "сложено");
    true
}

/// `/obj/item/storage/fancy/examine`: what is left in an open box.
pub fn examine_lines(world: &World, item: Entity, lines: &mut Vec<(Span, String)>) {
    let Some(dm) = world.get::<DmFancy>(item) else {
        return;
    };
    let open = dm.open_status == ALWAYS_OPEN
        || world
            .get::<FancyOpen>(item)
            .map_or(dm.open_status == 1, |o| o.0);
    if !open || dm.contents_tag.is_empty() {
        return;
    }
    let text = match count(world, item) {
        0 => format!("There are no {}s left.", dm.contents_tag),
        1 => format!("There is one {} left.", dm.contents_tag),
        n => format!("There are {n} {}s left.", dm.contents_tag),
    };
    lines.push((Span::Plain, text));
}
