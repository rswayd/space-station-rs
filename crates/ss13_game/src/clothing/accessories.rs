//! Accessories on jumpsuits (server), after tg `/obj/item/clothing/accessory`
//! (`under/accessories/_accessories.dm`: `can_attach_accessory`, `attach`, `detach`,
//! `generate_accessory_overlay`) and the jumpsuit's `attach_accessory`, `pop_accessory`,
//! `dump_attachments`, `list_accessories_with_icon` (`under/_under.dm`).
//!
//! An accessory used on a jumpsuit (held, in the other hand, worn or on the floor) is pinned to
//! it, up to five, if the jumpsuit covers its `attachment_slot`; it leaves the world and follows
//! the jumpsuit ([`AccessoryOf`], deleted with it). Alt+right click on the jumpsuit takes the
//! first one off into a hand. Worn, each is drawn over the jumpsuit from the accessories' worn
//! icon; BandaStation hides the ones that are not `above_suit` under a suit that hides the belt.
// shortcut: no accessory storage (holsters, webbing), no `above_suit` toggle, no accessory
// effects or traits while worn (`accessory_equipped`), no minimized icon on the jumpsuit's own
// sprite, no outfit `accessory` (the outfit data does not carry it); BandaStation's accessory
// weight is not applied.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmClothing, DmItem};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use super::cover::HIDEBELT;
use crate::inventory::{CarriedBy, Inventory};
use crate::tools::tell;

pub const ACCESSORY_ROOT: &str = "/obj/item/clothing/accessory";
pub const UNDER_ROOT: &str = "/obj/item/clothing/under";
/// `max_number_of_accessories`.
pub const MAX_ACCESSORIES: usize = 5;
/// `/obj/item/clothing/accessory` `worn_icon`.
pub const ACCESSORIES_WORN_DMI: &str = "icons/mob/clothing/accessories.dmi";

/// On an accessory pinned to a jumpsuit (tg: in its `attached_accessories`, inside it).
#[derive(Component, Debug)]
#[relationship(relationship_target = Accessories)]
pub struct AccessoryOf(pub Entity);

/// A jumpsuit's accessories, in the order they were pinned (tg `attached_accessories`).
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = AccessoryOf, linked_spawn)]
pub struct Accessories(Vec<Entity>);

impl Accessories {
    pub fn items(&self) -> &[Entity] {
        &self.0
    }
}

fn path_of(world: &World, e: Entity) -> Option<&str> {
    world.get::<DmItem>(e).map(|i| i.path.as_str())
}

pub fn is_accessory(world: &World, e: Entity) -> bool {
    path_of(world, e).is_some_and(|p| is_under(p, ACCESSORY_ROOT))
}

pub fn is_jumpsuit(world: &World, e: Entity) -> bool {
    path_of(world, e).is_some_and(|p| is_under(p, UNDER_ROOT))
}

/// The accessories on `uniform`.
pub fn attached(world: &World, uniform: Entity) -> Vec<Entity> {
    world
        .get::<Accessories>(uniform)
        .map(|a| a.items().to_vec())
        .unwrap_or_default()
}

/// The mob wearing or holding `item`, and where that is.
fn holder(world: &World, item: Entity) -> Option<(Entity, TilePos)> {
    let mob = world.get::<CarriedBy>(item)?.0;
    Some((mob, *world.get::<TilePos>(mob)?))
}

/// tg `attach_accessory` from a held accessory on `uniform`. False when `held` is no accessory
/// or `uniform` no jumpsuit.
pub fn attach(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    uniform: Entity,
) -> bool {
    if held == uniform || !is_accessory(world, held) || !is_jumpsuit(world, uniform) {
        return false;
    }
    // `can_attach_accessory`.
    let slot = world
        .get::<DmClothing>(held)
        .map_or(0, |c| c.attachment_slot);
    let covered = world
        .get::<DmClothing>(uniform)
        .map_or(0, |c| c.body_parts_covered);
    let refusal = if slot != 0 && covered & slot == 0 {
        Some("can't attach there!")
    } else if attached(world, uniform).len() >= MAX_ACCESSORIES {
        Some("too many accessories!")
    } else {
        None
    };
    if let Some(text) = refusal {
        tell(world, client, Span::Warning, text);
        return true;
    }
    // `temporarilyRemoveItemFromInventory`: only from the user's own hands.
    let in_hand = world
        .get::<Inventory>(mob)
        .is_some_and(|i| i.hands.contains(&Some(held)));
    if !in_hand || crate::inventory::no_drop(world, held) {
        return true;
    }
    world
        .entity_mut(held)
        .remove::<(CarriedBy, TilePos)>()
        .insert(AccessoryOf(uniform));
    tell(world, client, Span::Notice, "accessory attached");
    refresh_wearer(world, uniform);
    crate::inventory::refresh_look(world, mob);
    true
}

/// tg `click_alt_secondary` -> `pop_accessory`: the first accessory into the user's hands.
/// False when `uniform` is no jumpsuit.
pub fn pop(world: &mut World, client: Entity, mob: Entity, uniform: Entity) -> bool {
    if !is_jumpsuit(world, uniform) {
        return false;
    }
    let Some(&first) = attached(world, uniform).first() else {
        tell(world, client, Span::Warning, "нет аксессуаров для снятия!");
        return true;
    };
    world.entity_mut(first).remove::<AccessoryOf>();
    if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
        crate::storage::put_in_hands(world, mob, &inventory, first);
    }
    tell(world, client, Span::Notice, "аксессуар снят");
    refresh_wearer(world, uniform);
    true
}

/// tg `dump_attachments`: every accessory falls where the jumpsuit is.
pub fn dump(world: &mut World, uniform: Entity) {
    let pos = world
        .get::<TilePos>(uniform)
        .copied()
        .or_else(|| holder(world, uniform).map(|(_, p)| p));
    for accessory in attached(world, uniform) {
        world.entity_mut(accessory).remove::<AccessoryOf>();
        if let Some(pos) = pos {
            crate::inventory::put_down(world, accessory, pos);
        }
    }
}

fn refresh_wearer(world: &mut World, uniform: Entity) {
    if let Some((mob, _)) = holder(world, uniform) {
        crate::inventory::refresh_look(world, mob);
    }
}

/// Whether BandaStation hides `accessory` under the worn `suit` (`HIDEBELT`, not `above_suit`).
pub fn hidden_by(suit_flags_inv: u32, accessory: &DmClothing) -> bool {
    suit_flags_inv & HIDEBELT != 0 && !accessory.above_suit
}

/// The names `list_accessories_with_icon` lists: on a worn jumpsuit, not the hidden ones.
pub fn listed(world: &World, uniform: Entity) -> Vec<String> {
    let suit_flags = holder(world, uniform)
        .and_then(|(mob, _)| world.get::<Inventory>(mob))
        .filter(|i| i.uniform == Some(uniform))
        .and_then(|i| i.suit)
        .and_then(|s| world.get::<DmClothing>(s))
        .map_or(0, |c| c.flags_inv);
    attached(world, uniform)
        .into_iter()
        .filter(|&a| {
            world
                .get::<DmClothing>(a)
                .is_none_or(|c| !hidden_by(suit_flags, c))
        })
        .map(|a| crate::security::name_of(world, a))
        .collect()
}

/// `english_list` in BandaStation's Russian: "a, b и c".
pub fn english_list(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} и {last}", rest.join(", ")),
    }
}

/// tg `/obj/item/clothing/under/examine`: the attached accessories.
pub fn examine_lines(world: &World, uniform: Entity, lines: &mut Vec<crate::examine::Line>) {
    let names = listed(world, uniform);
    if names.is_empty() {
        return;
    }
    lines.push((
        Span::Plain,
        format!("Имеет прикрепленные: {}.", english_list(&names)),
    ));
    if let Some(&first) = attached(world, uniform).first() {
        let name = crate::security::name_of(world, first);
        lines.push((Span::Plain, format!("Альт-ПКМ для снятия {name}.")));
    }
}

/// The accessory part of the human examine's jumpsuit line (" с прикрепленными: ...").
pub fn examine_suffix(world: &World, uniform: Entity) -> String {
    let names = listed(world, uniform);
    if names.is_empty() {
        String::new()
    } else {
        format!(" с прикрепленными: {}", english_list(&names))
    }
}

/// tg `generate_accessory_overlay` of every accessory on a worn jumpsuit: the accessory's worn
/// state (`worn_icon_state`, else `icon_state`) from its `worn_icon`.
pub fn worn_states<'a>(
    accessories: &[Entity],
    suit_flags_inv: u32,
    item: impl Fn(Entity) -> Option<(&'a DmItem, Option<&'a DmAtom>, Option<&'a DmClothing>)>,
) -> Vec<(String, String)> {
    accessories
        .iter()
        .filter_map(|&a| item(a))
        .filter(|(_, _, c)| c.is_none_or(|c| !hidden_by(suit_flags_inv, c)))
        .map(|(dm, atom, _)| {
            let dmi = if dm.worn_icon.is_empty() {
                ACCESSORIES_WORN_DMI.to_owned()
            } else {
                dm.worn_icon.clone()
            };
            let state = if dm.worn_icon_state.is_empty() {
                atom.map(|a| a.icon_state.clone()).unwrap_or_default()
            } else {
                dm.worn_icon_state.clone()
            };
            (dmi, state)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(world: &mut World, path: &str, clothing: DmClothing) -> Entity {
        world
            .spawn((
                DmItem {
                    path: path.into(),
                    ..default()
                },
                clothing,
            ))
            .id()
    }

    #[test]
    fn lists_read_like_english_list() {
        let names: Vec<String> = ["a", "b", "c"].iter().map(|s| (*s).to_owned()).collect();
        assert_eq!(english_list(&names[..1]), "a");
        assert_eq!(english_list(&names[..2]), "a и b");
        assert_eq!(english_list(&names), "a, b и c");
    }

    #[test]
    fn pinned_accessories_follow_the_jumpsuit() {
        let mut world = World::new();
        let uniform = item(
            &mut world,
            "/obj/item/clothing/under/rank/security/officer",
            DmClothing {
                body_parts_covered: 1 | 2,
                ..default()
            },
        );
        let medal = item(
            &mut world,
            "/obj/item/clothing/accessory/medal",
            DmClothing {
                attachment_slot: 1,
                above_suit: true,
                ..default()
            },
        );
        let armband = item(
            &mut world,
            "/obj/item/clothing/accessory/armband",
            DmClothing::default(),
        );
        let mob = world
            .spawn(Inventory {
                hands: [Some(medal), Some(armband)],
                ..default()
            })
            .id();
        world.entity_mut(medal).insert(CarriedBy(mob));
        world.entity_mut(armband).insert(CarriedBy(mob));
        let client = world.spawn_empty().id();
        // tell() writes chat messages.
        world.init_resource::<Messages<ss13_net::chat::server::SendChat>>();
        assert!(attach(&mut world, client, mob, medal, uniform));
        assert!(attach(&mut world, client, mob, armband, uniform));
        assert_eq!(attached(&world, uniform), [medal, armband]);
        assert!(world.get::<CarriedBy>(medal).is_none());
        // Under a suit hiding the belt, the armband (not `above_suit`) is not listed.
        assert_eq!(listed(&world, uniform).len(), 2);
        let states = worn_states(&[medal, armband], HIDEBELT, |e| {
            Some((
                world.get::<DmItem>(e)?,
                world.get::<DmAtom>(e),
                world.get::<DmClothing>(e),
            ))
        });
        assert_eq!(states.len(), 1);
        // The jumpsuit goes, its accessories with it.
        world.despawn(uniform);
        assert!(world.get_entity(medal).is_err());
    }
}
