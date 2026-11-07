//! Flatpacks beyond the flatpacker's own (`crate::eng_machines::flatpacker` packs, deploys and
//! names them), after tg `code/game/objects/items/flatpacks.dm`:
//!
//! - The typed flatpacks (`/obj/item/flatpack/flatpacker`, `/shuttle_engine`, `/mailsorter`)
//!   hold their `board` from the start, mapped or spawned later.
//! - The flatpack cart (`/obj/structure/flatpack_cart`) holds up to three flatpacks: one clicked
//!   on it goes in (out of combat mode), an empty hand takes the topmost; each shows as a box on
//!   the cart, and examining it lists them bottom to top.
// shortcut: no two-handed carrying (BandaStation's `require_twohands`), in-hand or drag slowdown
// and no cart noise; the cart's contents are not dropped when it is destroyed. They come with
// the generic two-handed, slowdown and structure deconstruction mechanics.

use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmAtom, DmItem, DmObj};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay};

use crate::eng_machines::flatpacker::{FLATPACK, Flatpack, pack_board};
use crate::inventory::Inventory;
use crate::storage::{StoredIn, put_in_hands, put_inside};
use crate::tools::tell;

pub const FLATPACK_CART: &str = "/obj/structure/flatpack_cart";
/// `MAX_FLAT_PACKS`.
const MAX_FLAT_PACKS: usize = 3;
/// The typed flatpacks' `board`.
pub const TYPED_FLATPACKS: [(&str, &str); 3] = [
    (
        "/obj/item/flatpack/flatpacker",
        "/obj/item/circuitboard/machine/flatpacker",
    ),
    (
        "/obj/item/flatpack/shuttle_engine",
        "/obj/item/circuitboard/machine/engine/propulsion",
    ),
    (
        "/obj/item/flatpack/mailsorter",
        "/obj/item/circuitboard/machine/mailsorter",
    ),
];

/// The flatpacks in a cart, bottom first (tg `contents`).
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct FlatpackCart(pub Vec<Entity>);

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // `flatpack_cart/item_interaction`.
    app.add_click_handler(ClickPhase::HeldOnTarget, 4467, |w, c| {
        c.held
            .is_some_and(|h| load(w, c.client, c.mob, h, c.target))
    });
    // `flatpack_cart/attack_hand`.
    app.add_click_handler(ClickPhase::EmptyHand, 2027, |w, c| {
        unload(w, c.mob, c.target)
    });
    app.add_systems(Update, fill_typed);
}

/// `flatpack/Initialize`: `board = new board(src)` of a typed flatpack.
// Cost: per tick, the items spawned that tick.
fn fill_typed(
    added: Query<(Entity, &DmItem), (Added<DmItem>, Without<Flatpack>)>,
    mut commands: Commands,
) {
    let typed: Vec<(Entity, &'static str)> = added
        .iter()
        .filter_map(|(e, i)| {
            TYPED_FLATPACKS
                .iter()
                .find(|(pack, _)| is_under(&i.path, pack))
                .map(|&(_, board)| (e, board))
        })
        .collect();
    if typed.is_empty() {
        return;
    }
    commands.queue(move |world: &mut World| {
        for (pack, board_path) in typed {
            if world.get_entity(pack).is_err() || world.get::<Flatpack>(pack).is_some() {
                continue;
            }
            let Some(board) = crate::construction::spawn_detached(world, board_path) else {
                continue;
            };
            pack_board(world, pack, board);
        }
    });
}

fn is_cart(world: &World, e: Entity) -> bool {
    world
        .get::<DmObj>(e)
        .is_some_and(|o| is_under(&o.path, FLATPACK_CART))
}

/// The packs still in `cart`.
fn packs(world: &World, cart: Entity) -> Vec<Entity> {
    world
        .get::<FlatpackCart>(cart)
        .map(|c| {
            c.0.iter()
                .copied()
                .filter(|&p| world.get::<StoredIn>(p).is_some_and(|s| s.0 == cart))
                .collect()
        })
        .unwrap_or_default()
}

/// `flatpack_cart/item_interaction`: a flatpack goes on top. True when it was a flatpack on a
/// cart out of combat mode.
fn load(world: &mut World, client: Entity, mob: Entity, held: Entity, cart: Entity) -> bool {
    let is_pack = world
        .get::<DmItem>(held)
        .is_some_and(|i| is_under(&i.path, FLATPACK));
    let combat = world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0);
    if !is_pack || combat || !is_cart(world, cart) {
        return false;
    }
    let mut loaded = packs(world, cart);
    if loaded.len() >= MAX_FLAT_PACKS {
        tell(world, client, Span::Warning, "Тележка заполнена!");
        return true;
    }
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        for hand in inventory.hands.iter_mut() {
            if *hand == Some(held) {
                *hand = None;
            }
        }
    }
    put_inside(world, held, cart);
    loaded.push(held);
    world.entity_mut(cart).insert(FlatpackCart(loaded));
    refresh(world, cart);
    true
}

/// `flatpack_cart/attack_hand`: the topmost pack into the hands. True when `cart` is a cart.
fn unload(world: &mut World, mob: Entity, cart: Entity) -> bool {
    if !is_cart(world, cart) {
        return false;
    }
    let mut loaded = packs(world, cart);
    let Some(top) = loaded.pop() else {
        return true;
    };
    if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
        put_in_hands(world, mob, &inventory, top);
    }
    world.entity_mut(cart).insert(FlatpackCart(loaded));
    refresh(world, cart);
    true
}

/// `flatpack_cart/update_overlays`: a `flatcart_flat` box per pack, 4 px above the last.
fn refresh(world: &mut World, cart: Entity) {
    let count = packs(world, cart).len();
    let icon = world.get::<DmAtom>(cart).map(|a| a.icon.clone());
    let Some(id) = icon.and_then(|icon| {
        world
            .get_resource::<IconIndex>()?
            .get(&icon, "flatcart_flat")
    }) else {
        return;
    };
    let wanted: Vec<Overlay> = (0..count)
        .map(|i| {
            let mut box_ = Overlay::new(id);
            box_.pixel_offset = [0, 4 * i as i16];
            box_
        })
        .collect();
    if let Some(mut look) = world.get_mut::<Appearance>(cart) {
        look.overlays.retain(|o| o.icon_state != id);
        look.overlays.extend(wanted);
    }
}

/// `flatpack_cart/examine`: the packs bottom to top.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    if !is_cart(world, target) {
        return;
    }
    lines.push((Span::Plain, "Снизу вверх в тележке лежат:".to_owned()));
    for pack in packs(world, target) {
        lines.push((Span::Plain, crate::examine::name_of(world, pack)));
    }
}
