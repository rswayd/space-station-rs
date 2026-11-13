//! Hands on a basic mob (server), after tg `/datum/element/dextrous` and
//! `/datum/component/basic_inhands` (`code/datums/elements/dextrous.dm`,
//! `code/datums/components/basic_inhands.dm`).
//!
//! A [`Dextrous`] mob has an [`Inventory`] like a human, so `crate::inventory` runs its clicks,
//! hand swaps, drops and slot clicks; what differs is here:
//! - its slots are its HUD's ([`HudLayout`]): hands and, for drones, the internal storage
//!   (`ITEM_SLOT_SUITSTORE`: any item) and the head (head or mask items) — [`can_equip`];
//! - its look: the held items drawn over its own sprite (`basic_inhands`, raised by
//!   [`Dextrous::inhand_y`]) and the worn hat 15 pixels down (tg drone `update_worn_head`);
//! - death drops what its hands hold (`on_death`), the examine lists it (`on_examined`).
// shortcut: no throwing (`can_throw`), no `TRAIT_ADVANCEDTOOLUSER` checks (the port has none);
// the human-only keys (quick equip, smart equip) are refused as tg `ishuman` does.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmItem, IconIndex};
use ss13_shared::chat::Span;
use ss13_shared::protocol::InventorySlot;
use ss13_shared::{Appearance, Overlay, TilePos};

use crate::health::Stat;
use crate::hud::HudLayout;
use crate::inventory::{Inventory, no_drop, put_down};

/// `ITEM_SLOT_MASK`, `ITEM_SLOT_HEAD` (`code/__DEFINES/inventory.dm`).
const ITEM_SLOT_MASK: u32 = 1 << 5;
const ITEM_SLOT_HEAD: u32 = 1 << 6;
/// `ABSTRACT`, `HAND_ITEM` (`item_flags`): not listed on examine.
const ABSTRACT: u32 = 1 << 7;
const HAND_ITEM: u32 = 1 << 12;
/// tg drone `update_worn_head`: `head_overlay.pixel_z -= 15`.
const HEAD_PIXEL_Y: i16 = -15;
/// Its default files: a mask is drawn from the masks' worn file.
const HEAD_WORN_DMI: &str = "icons/mob/clothing/head/utility.dmi";
const MASK_WORN_DMI: &str = "icons/mob/clothing/mask.dmi";
const MASK_ROOT: &str = "/obj/item/clothing/mask";

/// tg `/datum/element/dextrous` with `basic_inhands`, on a basic mob with an [`Inventory`].
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[require(DextrousOverlays)]
pub struct Dextrous {
    /// Its `hud_type`: the slots it has besides the hands.
    pub layout: HudLayout,
    /// `basic_inhands` `y_offset`, in pixels.
    pub inhand_y: i16,
}

/// The overlays [`refresh_look`] put on the mob last, so the next refresh replaces only them.
#[derive(Component, Clone, Debug, Default)]
pub struct DextrousOverlays(Vec<Overlay>);

pub struct DextrousPlugin;

impl Plugin for DextrousPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (refresh_look, drop_on_death));
    }
}

/// tg drone `can_equip` into an empty `slot` of `layout`: the head takes head and mask items,
/// the internal storage anything; there are no other slots.
pub fn can_equip(
    layout: HudLayout,
    inventory: &Inventory,
    item: &DmItem,
    slot: InventorySlot,
) -> bool {
    if layout == HudLayout::Human || !layout.has(slot) || inventory.slot(slot).is_some() {
        return false;
    }
    match slot {
        InventorySlot::Head => item.slot_flags & (ITEM_SLOT_HEAD | ITEM_SLOT_MASK) != 0,
        InventorySlot::SuitStorage => true,
        _ => false,
    }
}

/// The held and worn sprites over the mob's own, when its inventory changed.
fn refresh_look(
    mut mobs: Query<
        (
            &Inventory,
            &Dextrous,
            &mut Appearance,
            &mut DextrousOverlays,
        ),
        Changed<Inventory>,
    >,
    items: Query<(&DmItem, Option<&DmAtom>)>,
    icons: Option<Res<IconIndex>>,
) {
    let Some(icons) = icons else {
        return;
    };
    for (inventory, dextrous, mut appearance, mut drawn) in &mut mobs {
        let wanted = overlays(inventory, dextrous, &items, &icons);
        if wanted == drawn.0 {
            continue;
        }
        let old = std::mem::replace(&mut drawn.0, wanted.clone());
        let mut list: Vec<Overlay> = appearance.overlays.clone();
        list.retain(|o| !old.contains(o));
        list.extend(wanted);
        appearance.overlays = list;
    }
}

/// The hat (tg `DRONE_HEAD_LAYER`) and the held items (`basic_inhands`), in drawing order.
fn overlays(
    inventory: &Inventory,
    dextrous: &Dextrous,
    items: &Query<(&DmItem, Option<&DmAtom>)>,
    icons: &IconIndex,
) -> Vec<Overlay> {
    let state = |atom: Option<&DmAtom>, own: &str| -> String {
        if own.is_empty() {
            atom.map(|a| a.icon_state.clone()).unwrap_or_default()
        } else {
            own.to_owned()
        }
    };
    let mut out = Vec::new();
    if let Some((item, atom)) = inventory.head.and_then(|e| items.get(e).ok()) {
        let dmi = if !item.worn_icon.is_empty() {
            item.worn_icon.as_str()
        } else if is_under(&item.path, MASK_ROOT) {
            MASK_WORN_DMI
        } else {
            HEAD_WORN_DMI
        };
        if let Some(id) = icons.get(dmi, &state(atom, &item.worn_icon_state)) {
            out.push(Overlay {
                pixel_offset: [0, HEAD_PIXEL_Y],
                ..Overlay::new(id)
            });
        }
    }
    for (hand, held) in inventory.hands.iter().enumerate() {
        let Some((item, atom)) = held.and_then(|e| items.get(e).ok()) else {
            continue;
        };
        let dmi = if hand == 0 {
            &item.lefthand_file
        } else {
            &item.righthand_file
        };
        if let Some(id) = icons.get(dmi, &state(atom, &item.inhand_icon_state)) {
            out.push(Overlay {
                pixel_offset: [0, dextrous.inhand_y],
                ..Overlay::new(id)
            });
        }
    }
    out
}

/// `/datum/element/dextrous/on_death`: `drop_all_held_items`.
fn drop_on_death(world: &mut World) {
    let died: Vec<Entity> = world
        .query_filtered::<(Entity, Ref<Stat>), With<Dextrous>>()
        .iter(world)
        .filter(|(_, s)| s.is_changed() && **s == Stat::Dead)
        .map(|(e, _)| e)
        .collect();
    for mob in died {
        drop_held(world, mob);
    }
}

/// tg `drop_all_held_items`: what is not stuck to the hands falls on the mob's tile.
pub fn drop_held(world: &mut World, mob: Entity) {
    let (Some(inventory), Some(&pos)) = (world.get::<Inventory>(mob), world.get::<TilePos>(mob))
    else {
        return;
    };
    for item in inventory.hands.into_iter().flatten() {
        if !no_drop(world, item) {
            put_down(world, item, pos);
        }
    }
}

/// `/datum/element/dextrous/on_examined`: "It has X in its left hand."
pub fn examine_lines(world: &World, mob: Entity, lines: &mut Vec<crate::examine::Line>) {
    if world.get::<Dextrous>(mob).is_none() {
        return;
    }
    let Some(inventory) = world.get::<Inventory>(mob) else {
        return;
    };
    for (hand, held) in inventory.hands.iter().enumerate() {
        let Some(item) = *held else {
            continue;
        };
        if world
            .get::<DmItem>(item)
            .is_some_and(|i| i.item_flags & (ABSTRACT | HAND_ITEM) != 0)
        {
            continue;
        }
        let name = crate::security::name_of(world, item);
        let where_ = if hand == 0 {
            "в левой руке"
        } else {
            "в правой руке"
        };
        lines.push((Span::Info, format!("У него {name} {where_}.")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(slot_flags: u32) -> DmItem {
        DmItem {
            slot_flags,
            ..default()
        }
    }

    #[test]
    fn a_drone_wears_hats_and_masks_and_stores_anything() {
        let empty = Inventory::default();
        let hat = item(ITEM_SLOT_HEAD);
        let mask = item(ITEM_SLOT_MASK);
        let toolbox = item(0);
        let drone = HudLayout::Drone;
        assert!(can_equip(drone, &empty, &hat, InventorySlot::Head));
        assert!(can_equip(drone, &empty, &mask, InventorySlot::Head));
        assert!(!can_equip(drone, &empty, &toolbox, InventorySlot::Head));
        assert!(can_equip(
            drone,
            &empty,
            &toolbox,
            InventorySlot::SuitStorage
        ));
        // No human slots, no hands through equipping.
        for slot in [
            InventorySlot::Uniform,
            InventorySlot::Back,
            InventorySlot::Mask,
            InventorySlot::LeftHand,
            InventorySlot::LeftPocket,
        ] {
            assert!(!can_equip(drone, &empty, &hat, slot), "{slot:?}");
        }
    }

    #[test]
    fn a_taken_slot_takes_nothing() {
        let mut world = World::new();
        let worn = world.spawn_empty().id();
        let inventory = Inventory {
            head: Some(worn),
            suit_storage: Some(worn),
            ..default()
        };
        let hat = item(ITEM_SLOT_HEAD);
        assert!(!can_equip(
            HudLayout::Drone,
            &inventory,
            &hat,
            InventorySlot::Head
        ));
        assert!(!can_equip(
            HudLayout::Drone,
            &inventory,
            &hat,
            InventorySlot::SuitStorage
        ));
    }

    #[test]
    fn death_drops_the_hands_but_not_a_stuck_item() {
        let mut world = World::new();
        let pos = TilePos::new(1, 1, 1);
        let mob = world
            .spawn((
                Dextrous {
                    layout: HudLayout::Drone,
                    inhand_y: 0,
                },
                pos,
            ))
            .id();
        let tool = world.spawn(crate::inventory::CarriedBy(mob)).id();
        let stuck = world
            .spawn((crate::inventory::CarriedBy(mob), crate::inventory::NoDrop))
            .id();
        world.entity_mut(mob).insert(Inventory {
            hands: [Some(tool), Some(stuck)],
            ..default()
        });
        drop_held(&mut world, mob);
        assert_eq!(world.get::<TilePos>(tool), Some(&pos));
        assert!(world.get::<TilePos>(stuck).is_none());
    }
}
