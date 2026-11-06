//! Explorer gear (server), after tg `code/modules/mining/equipment/explorer_gear.dm` and
//! `/datum/component/armor_plate` (`code/datums/components/armor_plate.dm`). The suits, hoods
//! and masks themselves are ordinary clothing (`crate::clothing`: armour, hoods, visors).
//!
//! - Armour plates: goliath hide (a stack, one unit each) strengthens an explorer suit or hood,
//!   a talisman the bone armour or the skull helmet: +10 melee each, up to 3, the first one
//!   prefixes the name with "reinforced"; examine shows the count.
//! - The goliath cloak, held and not worn, alt-clicks between defensive use (suit slot, its
//!   armour and temperature protection) and ceremonial use (neck slot, none).
//! - The explorer gas mask fits in boxes while pushed up (`WEIGHT_CLASS_SMALL`, else
//!   `NORMAL`).
// shortcut: no drake-remains empowerment of the berserker armour (lavaland loot) and no ripley
// hide plates (mechs); the folded explorer mask starts down.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmArmor, DmClothing, DmItem, DmName, DmWeight};
use ss13_shared::chat::Span;

use crate::clothing::toggles::AdjustedUp;
use crate::examine::Line;
use crate::tools::{stack_amount, tell, use_stack};

const GOLIATH_HIDE: &str = "/obj/item/stack/sheet/animalhide/goliath_hide";
const TALISMAN: &str = "/obj/item/clothing/accessory/talisman";
const GOLIATH_CLOAK: &str = "/obj/item/clothing/suit/hooded/cloak/goliath";
const EXPLORER_MASK: &str = "/obj/item/clothing/mask/gas/explorer";
/// `ITEM_SLOT_OCLOTHING`, `ITEM_SLOT_NECK`.
const SLOT_OCLOTHING: u32 = 1 << 0;
const SLOT_NECK: u32 = 1 << 11;
/// `WEIGHT_CLASS_SMALL`, `WEIGHT_CLASS_NORMAL`.
const SMALL: u8 = 2;
const NORMAL: u8 = 3;
/// `maxamount`.
pub const MAX_PLATES: u32 = 3;
/// `/datum/armor/armor_plate`.
pub const PLATE_MELEE: f32 = 10.0;

/// The `armor_plate` component of an item type: its upgrade item and its name.
pub fn plating(path: &str) -> Option<(&'static str, &'static str)> {
    const HIDE: (&str, &str) = (GOLIATH_HIDE, "goliath hide plates");
    const CHARM: (&str, &str) = (TALISMAN, "bone talisman");
    if is_under(path, "/obj/item/clothing/suit/hooded/explorer")
        || is_under(path, "/obj/item/clothing/head/hooded/explorer")
    {
        Some(HIDE)
    } else if is_under(path, "/obj/item/clothing/suit/armor/bone")
        || is_under(path, "/obj/item/clothing/head/helmet/skull")
    {
        Some(CHARM)
    } else {
        None
    }
}

/// `amount` of the armour plates applied.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArmorPlates(pub u32);

/// A goliath cloak in ceremonial use, with what it gave up.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Ceremonial {
    armor: Option<DmArmor>,
    slot_flags: u32,
    cold: u32,
    heat: u32,
}

pub(super) struct ExplorerPlugin;

impl Plugin for ExplorerPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(mask_up).add_observer(mask_down);
    }
}

fn item_path(world: &World, e: Entity) -> Option<String> {
    world.get::<DmItem>(e).map(|i| i.path.clone())
}

fn set_w_class(world: &mut World, item: Entity, w_class: u8) {
    if let Some(mut w) = world.get_mut::<DmWeight>(item) {
        w.w_class = w_class;
    }
}

fn is_explorer_mask(world: &World, e: Entity) -> bool {
    item_path(world, e).is_some_and(|p| is_under(&p, EXPLORER_MASK))
}

fn mask_up(add: On<Add<AdjustedUp>>, mut commands: Commands) {
    let e = add.entity;
    commands.queue(move |world: &mut World| {
        if world.get_entity(e).is_ok() && is_explorer_mask(world, e) {
            set_w_class(world, e, SMALL);
        }
    });
}

fn mask_down(remove: On<Remove<AdjustedUp>>, mut commands: Commands) {
    let e = remove.entity;
    commands.queue(move |world: &mut World| {
        if world.get_entity(e).is_ok() && is_explorer_mask(world, e) {
            set_w_class(world, e, NORMAL);
        }
    });
}

/// `applyplate`: `held` onto the plated item `target`. True when handled.
pub fn item_interaction(world: &mut World, client: Entity, held: Entity, target: Entity) -> bool {
    let Some((upgrade, _)) = item_path(world, target).and_then(|p| plating(&p)) else {
        return false;
    };
    let Some(held_path) = item_path(world, held) else {
        return false;
    };
    if !is_under(&held_path, upgrade) {
        return false;
    }
    let amount = world.get::<ArmorPlates>(target).map_or(0, |p| p.0);
    let name = crate::examine::name_of(world, target);
    if amount >= MAX_PLATES {
        tell(
            world,
            client,
            Span::Warning,
            format!("You can't improve {name} any further!"),
        );
        return true;
    }
    if world.get::<crate::tools::Stack>(held).is_some() {
        if stack_amount(world, held) < 1 || !use_stack(world, held, 1) {
            return true;
        }
    } else {
        world.despawn(held);
    }
    world.entity_mut(target).insert(ArmorPlates(amount + 1));
    let mut armor = world.get::<DmArmor>(target).copied().unwrap_or_default();
    armor.melee += PLATE_MELEE;
    world.entity_mut(target).insert(armor);
    if amount == 0 {
        // `upgrade_prefix`.
        let renamed = format!("reinforced {name}");
        if let Some(mut dm) = world.get_mut::<DmName>(target) {
            dm.name = renamed.clone();
        }
        world.entity_mut(target).insert(Name::new(renamed.clone()));
        tell(
            world,
            client,
            Span::Info,
            format!("You strengthen {renamed}, improving its resistance against attacks."),
        );
    } else {
        tell(
            world,
            client,
            Span::Info,
            format!("You strengthen {name}, improving its resistance against attacks."),
        );
    }
    true
}

/// The goliath cloak's `click_alt`. True when `target` is one.
pub fn click_alt(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if !item_path(world, target).is_some_and(|p| is_under(&p, GOLIATH_CLOAK)) {
        return false;
    }
    let Some(inventory) = world.get::<crate::inventory::Inventory>(mob).cloned() else {
        return false;
    };
    use ss13_shared::protocol::InventorySlot;
    if inventory.slot(InventorySlot::Neck) == Some(target)
        || inventory.slot(InventorySlot::Suit) == Some(target)
    {
        tell(
            world,
            client,
            Span::Warning,
            "You can't adjust goliath cloak while wearing it!",
        );
        return true;
    }
    if !inventory.hands.contains(&Some(target)) {
        tell(
            world,
            client,
            Span::Warning,
            "You must be holding goliath cloak in order to adjust it!",
        );
        return true;
    }
    let name = crate::examine::name_of(world, target);
    let user = crate::examine::name_of(world, mob);
    if let Some(old) = world.get::<Ceremonial>(target).cloned() {
        world.entity_mut(target).remove::<Ceremonial>();
        if let Some(mut item) = world.get_mut::<DmItem>(target) {
            item.slot_flags = old.slot_flags;
        }
        if let Some(mut c) = world.get_mut::<DmClothing>(target) {
            c.cold_protection = old.cold;
            c.heat_protection = old.heat;
        }
        if let Some(armor) = old.armor {
            world.entity_mut(target).insert(armor);
        }
        say_adjust(world, client, mob, &user, &name, "defensive");
        return true;
    }
    let slot_flags = world.get::<DmItem>(target).map_or(0, |i| i.slot_flags);
    if slot_flags & SLOT_OCLOTHING == 0 {
        return true;
    }
    let (cold, heat) = world
        .get::<DmClothing>(target)
        .map_or((0, 0), |c| (c.cold_protection, c.heat_protection));
    let armor = world.get::<DmArmor>(target).copied();
    world.entity_mut(target).insert(Ceremonial {
        armor,
        slot_flags,
        cold,
        heat,
    });
    if let Some(mut item) = world.get_mut::<DmItem>(target) {
        item.slot_flags = SLOT_NECK;
    }
    if let Some(mut c) = world.get_mut::<DmClothing>(target) {
        c.cold_protection = 0;
        c.heat_protection = 0;
    }
    // `set_armor(/datum/armor/none)`.
    world.entity_mut(target).insert(DmArmor::default());
    say_adjust(world, client, mob, &user, &name, "ceremonial");
    true
}

fn say_adjust(world: &mut World, client: Entity, mob: Entity, user: &str, name: &str, use_: &str) {
    let Some(&pos) = world.get::<ss13_shared::TilePos>(mob) else {
        return;
    };
    let others = crate::obj_damage::clients_in_range(world, pos, 7, &[mob]);
    for other in others {
        tell(
            world,
            other,
            Span::Notice,
            format!("{user} adjusts their {name} for {use_} use."),
        );
    }
    tell(
        world,
        client,
        Span::Notice,
        format!("You adjust your {name} for {use_} use."),
    );
}

/// The plates' `examine` and the explorer mask's box hint.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<Line>) {
    let Some(path) = item_path(world, target) else {
        return;
    };
    if let Some((_, upgrade_name)) = plating(&path) {
        let amount = world.get::<ArmorPlates>(target).map_or(0, |p| p.0);
        let text = if amount > 0 {
            format!("It has been strengthened with {amount}/{MAX_PLATES} {upgrade_name}.")
        } else {
            format!("It can be strengthened with up to {MAX_PLATES} {upgrade_name}.")
        };
        lines.push((Span::Notice, text));
    }
    if is_under(&path, EXPLORER_MASK) {
        let small = world
            .get::<DmWeight>(target)
            .is_some_and(|w| w.w_class <= SMALL);
        if world.get::<AdjustedUp>(target).is_none() && !small {
            lines.push((
                Span::Notice,
                "You could fit this into a box if you adjusted it.".to_owned(),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spawn_item(world: &mut World, path: &str) -> Entity {
        world
            .spawn((
                DmItem {
                    path: path.to_owned(),
                    ..default()
                },
                DmName {
                    name: "explorer suit".to_owned(),
                    desc: String::new(),
                },
            ))
            .id()
    }

    #[test]
    fn plated_types_take_their_upgrade() {
        assert_eq!(
            plating("/obj/item/clothing/suit/hooded/explorer/syndicate").map(|p| p.0),
            Some(GOLIATH_HIDE)
        );
        assert_eq!(
            plating("/obj/item/clothing/head/helmet/skull").map(|p| p.0),
            Some(TALISMAN)
        );
        assert_eq!(
            plating("/obj/item/clothing/suit/hooded/cloak/goliath"),
            None
        );
    }

    #[test]
    fn three_hides_at_most_each_adding_melee() {
        let mut world = World::new();
        world.init_resource::<bevy::ecs::message::Messages<ss13_net::chat::server::SendChat>>();
        let client = world.spawn_empty().id();
        let suit = spawn_item(&mut world, "/obj/item/clothing/suit/hooded/explorer");
        world.entity_mut(suit).insert(DmArmor {
            melee: 30.0,
            ..default()
        });
        let hide = spawn_item(&mut world, GOLIATH_HIDE);
        world
            .entity_mut(hide)
            .insert(crate::tools::Stack { amount: 5 });
        for _ in 0..4 {
            assert!(item_interaction(&mut world, client, hide, suit));
        }
        assert_eq!(world.get::<ArmorPlates>(suit), Some(&ArmorPlates(3)));
        assert_eq!(world.get::<DmArmor>(suit).unwrap().melee, 60.0);
        assert_eq!(world.get::<crate::tools::Stack>(hide).unwrap().amount, 2);
        assert_eq!(
            world.get::<DmName>(suit).unwrap().name,
            "reinforced explorer suit"
        );
        // A talisman does not fit an explorer suit.
        let charm = spawn_item(&mut world, TALISMAN);
        assert!(!item_interaction(&mut world, client, charm, suit));
    }
}
