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
//!
//! BandaStation's gloves accessories (rings, `gloves/gloves.dm`, `gloves_accessories.dm`) go on
//! gloves the same way, one per pair, drawn over the worn gloves; other accessories do not, nor
//! do rings go on jumpsuits.
// Accessories with storage (holsters, the ammo vest) and the jumpsuit's weight: `super::holsters`.
// An outfit's `accessory` is pinned by `crate::outfit` ([`pin`]).
// shortcut: no `above_suit` toggle, no accessory effects or traits while worn
// (`accessory_equipped`), no minimized icon on the jumpsuit's own sprite.

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
pub const GLOVES_ROOT: &str = "/obj/item/clothing/gloves";
pub const GLOVES_ACCESSORY_ROOT: &str = "/obj/item/clothing/accessory/gloves_accessory";
/// `max_number_of_accessories` of a jumpsuit (gloves: 1).
pub const MAX_ACCESSORIES: usize = 5;

/// What an accessory is pinned to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Host {
    Jumpsuit,
    /// BandaStation: gloves take gloves accessories (rings).
    Gloves,
}

impl Host {
    fn max(self) -> usize {
        match self {
            Self::Jumpsuit => MAX_ACCESSORIES,
            Self::Gloves => 1,
        }
    }
}

/// What `target` is as an accessory host.
pub fn host(world: &World, target: Entity) -> Option<Host> {
    let path = path_of(world, target)?;
    if is_under(path, UNDER_ROOT) {
        Some(Host::Jumpsuit)
    } else if is_under(path, GLOVES_ROOT) {
        Some(Host::Gloves)
    } else {
        None
    }
}

/// Whether `accessory` goes on a host of that kind (rings only on gloves, the rest only on
/// jumpsuits).
fn goes_on(world: &World, accessory: Entity, host: Host) -> bool {
    let ring = path_of(world, accessory).is_some_and(|p| is_under(p, GLOVES_ACCESSORY_ROOT));
    ring == (host == Host::Gloves)
}
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

/// tg `can_attach_accessory`: one storage accessory per jumpsuit, the `attachment_slot` covered,
/// at most [`Host::max`]; the refusal is the balloon alert (BandaStation's for gloves).
fn can_attach(
    world: &World,
    accessory: Entity,
    uniform: Entity,
    host: Host,
) -> Result<(), &'static str> {
    let text = |jumpsuit: &'static str, gloves: &'static str| match host {
        Host::Jumpsuit => jumpsuit,
        Host::Gloves => gloves,
    };
    let slot = world
        .get::<DmClothing>(accessory)
        .map_or(0, |c| c.attachment_slot);
    let covered = world
        .get::<DmClothing>(uniform)
        .map_or(0, |c| c.body_parts_covered);
    if world.get::<crate::storage::Storage>(accessory).is_some()
        && super::holsters::storage_accessory(world, uniform).is_some()
    {
        Err(text("isn't compatible!", "несовместимо с этим!"))
    } else if slot != 0 && covered & slot == 0 {
        Err(text("can't attach there!", "нельзя надеть сюда!"))
    } else if attached(world, uniform).len() >= host.max() {
        Err(text("too many accessories!", "слишком много аксессуаров!"))
    } else {
        Ok(())
    }
}

/// tg `attach_accessory` from a held accessory on `uniform` (a jumpsuit, or gloves for a ring).
/// False when `held` is no accessory for it.
pub fn attach(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    uniform: Entity,
) -> bool {
    if held == uniform || !is_accessory(world, held) {
        return false;
    }
    let Some(host) = host(world, uniform).filter(|&h| goes_on(world, held, h)) else {
        return false;
    };
    if let Err(text) = can_attach(world, held, uniform, host) {
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
    let done = match host {
        Host::Jumpsuit => "accessory attached",
        Host::Gloves => "аксессуар прикреплён",
    };
    tell(world, client, Span::Notice, done);
    super::holsters::update_weight(world, uniform);
    refresh_wearer(world, uniform);
    crate::inventory::refresh_look(world, mob);
    true
}

/// tg `/datum/outfit/equip` -> `attach_accessory` without a user: `accessory` (out of the world)
/// onto `uniform` if it can go there, else false and nothing changes.
pub fn pin(world: &mut World, accessory: Entity, uniform: Entity) -> bool {
    if !is_accessory(world, accessory) {
        return false;
    }
    let Some(host) = host(world, uniform).filter(|&h| goes_on(world, accessory, h)) else {
        return false;
    };
    if can_attach(world, accessory, uniform, host).is_err() {
        return false;
    }
    world
        .entity_mut(accessory)
        .remove::<(CarriedBy, TilePos)>()
        .insert(AccessoryOf(uniform));
    super::holsters::update_weight(world, uniform);
    true
}

/// tg `click_alt_secondary` -> `pop_accessory`: the first accessory into the user's hands.
/// False when `uniform` is no jumpsuit or gloves.
pub fn pop(world: &mut World, client: Entity, mob: Entity, uniform: Entity) -> bool {
    let Some(host) = host(world, uniform) else {
        return false;
    };
    let Some(&first) = attached(world, uniform).first() else {
        let text = match host {
            Host::Jumpsuit => "нет аксессуаров для снятия!",
            Host::Gloves => "нет аксессуара чтобы снять!",
        };
        tell(world, client, Span::Warning, text);
        return true;
    };
    world.entity_mut(first).remove::<AccessoryOf>();
    if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
        crate::storage::put_in_hands(world, mob, &inventory, first);
    }
    tell(world, client, Span::Notice, "аксессуар снят");
    super::holsters::update_weight(world, uniform);
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

/// tg `/obj/item/clothing/under/examine` (BandaStation `gloves/examine`): the attached
/// accessories.
pub fn examine_lines(world: &World, uniform: Entity, lines: &mut Vec<crate::examine::Line>) {
    let names = listed(world, uniform);
    if names.is_empty() {
        return;
    }
    if host(world, uniform) == Some(Host::Gloves) {
        lines.push((
            Span::Plain,
            format!("Имеет прикрепленное: {}.", english_list(&names)),
        ));
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

    #[test]
    fn rings_go_on_gloves_one_at_a_time() {
        const HANDS: u32 = 1 << 9;
        let mut world = World::new();
        world.init_resource::<Messages<ss13_net::chat::server::SendChat>>();
        let gloves = item(
            &mut world,
            "/obj/item/clothing/gloves/color/black",
            DmClothing {
                body_parts_covered: HANDS,
                ..default()
            },
        );
        let uniform = item(
            &mut world,
            "/obj/item/clothing/under/color/grey",
            DmClothing::default(),
        );
        let ring = item(
            &mut world,
            "/obj/item/clothing/accessory/gloves_accessory/ring",
            DmClothing {
                attachment_slot: HANDS,
                ..default()
            },
        );
        let diamond = item(
            &mut world,
            "/obj/item/clothing/accessory/gloves_accessory/ring/diamond",
            DmClothing {
                attachment_slot: HANDS,
                ..default()
            },
        );
        let medal = item(
            &mut world,
            "/obj/item/clothing/accessory/medal",
            DmClothing::default(),
        );
        let mob = world
            .spawn(Inventory {
                hands: [Some(ring), Some(diamond)],
                ..default()
            })
            .id();
        world.entity_mut(ring).insert(CarriedBy(mob));
        world.entity_mut(diamond).insert(CarriedBy(mob));
        let client = world.spawn_empty().id();
        // A ring is no jumpsuit accessory, a medal no gloves accessory.
        assert!(!attach(&mut world, client, mob, ring, uniform));
        assert!(!attach(&mut world, client, mob, medal, gloves));
        assert!(attach(&mut world, client, mob, ring, gloves));
        assert_eq!(attached(&world, gloves), [ring]);
        // One per pair: the diamond ring stays in the hand.
        assert!(attach(&mut world, client, mob, diamond, gloves));
        assert_eq!(attached(&world, gloves), [ring]);
        assert!(world.get::<CarriedBy>(diamond).is_some());
    }
}
