//! Equipping a job's outfit (server), after tg `/datum/outfit/proc/equip` and
//! `/datum/outfit/job/post_equip` (the ID card: trim access, registered name, label).
//!
//! Every outfit slot but `back` is equipped directly (tg `equip_to_slot_or_del`); the job's
//! backpack comes from `crate::jobs::join` and gets `box` and `backpack_contents`
//! ([`fill_backpack`]).
// The outfit's `accessory` is pinned to its uniform (`crate::clothing::accessories::pin`).
// The outfit's `implants` go in last, silently ([`implant_outfit`]).
// shortcut: no `can_equip` check (tg deletes an item that does not fit),
// no `internals_slot` (tg opens that tank at spawn).

use std::collections::HashMap;

use bevy::prelude::*;
use ss13_mapload::jobs::{Job, Outfit, OutfitSlot};
use ss13_mapload::{DmName, Template};
use ss13_net::server::Networked;

use crate::id_access::{AddMode, CardAccess, Trim, add_access};
use crate::inventory::{CarriedBy, IdCard, Inventory};

/// Templates of every outfit item that has a prototype, by type path.
#[derive(Resource, Default)]
pub struct OutfitTemplates(pub HashMap<String, Template>);

/// Who an ID card belongs to (tg `registered_name`, `assignment`, the trim's job).
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct IdRegistration {
    pub registered_name: String,
    pub assignment: String,
    /// The job title (`trim.job.title`).
    pub job: String,
}

/// tg `/obj/item/card/id/update_label` without honorifics (a card starts at
/// `HONORIFIC_POSITION_NONE`): `ID-карта «Name» (Assignment)`.
pub fn id_label(registered_name: &str, assignment: &str) -> String {
    format!("ID-карта «{registered_name}» ({assignment})")
}

/// `SSid_access.apply_trim_to_card` of `job`'s trim on a card of type `path`. A card whose
/// slots cannot hold the trim's wildcards still gets them, as forced ("Hacked") wildcards.
pub fn job_card_access(job: &Job, path: &str) -> (IdCard, CardAccess) {
    let mut card = IdCard { access: Vec::new() };
    let mut data = CardAccess::of_type(path);
    let trim = Trim::of_job(job);
    if !crate::id_access::apply_trim(&mut card, &mut data, trim.clone()) {
        data.trim = Some(trim);
        add_access(
            &mut card,
            &mut data,
            &job.access,
            None,
            AddMode::ForceAddAll,
        );
    }
    (card, data)
}

/// Spawns `job`'s outfit into `mob`'s slots and returns the inventory. Items replicate once
/// they are dropped. An item without a prototype is skipped with a warning.
pub fn equip(world: &mut World, mob: Entity, job: &Job, name: &str) -> Inventory {
    let Some(outfit) = &job.outfit else {
        return Inventory::default();
    };
    let inventory = world.resource_scope(|world, templates: Mut<OutfitTemplates>| {
        equip_outfit(world, mob, outfit, &templates.0)
    });
    if let Some(card) = inventory.id {
        let label = id_label(name, &job.assignment);
        let path = world
            .get::<ss13_mapload::DmItem>(card)
            .map(|i| i.path.clone())
            .unwrap_or_default();
        let (access, data) = job_card_access(job, &path);
        let mut card = world.entity_mut(card);
        card.insert((
            access,
            data,
            IdRegistration {
                registered_name: name.to_owned(),
                assignment: job.assignment.clone(),
                job: job.title.clone(),
            },
        ));
        if let Some(mut dm_name) = card.get_mut::<DmName>() {
            dm_name.name = label;
        } else {
            card.insert(DmName {
                name: label,
                desc: String::new(),
            });
        }
    }
    implant_outfit(world, mob, outfit);
    inventory
}

/// The end of tg `equip`: every `implants` type goes in silently (`implant(user, null, TRUE)`).
/// A type the port has no implant for is skipped.
pub fn implant_outfit(world: &mut World, mob: Entity, outfit: &Outfit) {
    for path in &outfit.implants {
        let id = crate::implants::next_id(world);
        let Some(imp) = crate::implants::Implant::new(path, id) else {
            warn!("{}: no implant {path}", outfit.path);
            continue;
        };
        let _ = crate::implants::implant_silent(world, mob, imp);
    }
}

/// The slot loop of tg `/datum/outfit/proc/equip`: `outfit`'s items (all but `back`) from
/// `templates` into a new inventory of `mob`. An item without a template is skipped.
pub fn equip_outfit(
    world: &mut World,
    mob: Entity,
    outfit: &Outfit,
    templates: &HashMap<String, Template>,
) -> Inventory {
    let mut inventory = Inventory::default();
    for (slot, path) in &outfit.items {
        let target = match slot {
            OutfitSlot::Uniform => &mut inventory.uniform,
            OutfitSlot::Suit => &mut inventory.suit,
            OutfitSlot::Belt => &mut inventory.belt,
            OutfitSlot::Gloves => &mut inventory.gloves,
            OutfitSlot::Shoes => &mut inventory.shoes,
            OutfitSlot::Head => &mut inventory.head,
            OutfitSlot::Mask => &mut inventory.mask,
            OutfitSlot::Neck => &mut inventory.neck,
            OutfitSlot::Ears => &mut inventory.ears,
            OutfitSlot::Glasses => &mut inventory.eyes,
            OutfitSlot::Id => &mut inventory.id,
            OutfitSlot::SuitStore => &mut inventory.suit_storage,
            OutfitSlot::LHand => &mut inventory.hands[0],
            OutfitSlot::RHand => &mut inventory.hands[1],
            OutfitSlot::LPocket => &mut inventory.pockets[0],
            OutfitSlot::RPocket => &mut inventory.pockets[1],
            // The caller gives the backpack.
            OutfitSlot::Back => continue,
        };
        if target.is_some() {
            continue;
        }
        let Some(template) = templates.get(path) else {
            warn!("{}: no prototype for {path}; slot left empty", outfit.path);
            continue;
        };
        let item = template.spawn(world, None);
        world.entity_mut(item).insert((Networked, CarriedBy(mob)));
        crate::storage::finish_item(world, item, 0);
        *target = Some(item);
    }
    // `if(accessory)`: pinned to the uniform; without one (or where it cannot go) it is deleted
    // (tg warns "No uniform present!").
    if let Some(path) = &outfit.accessory {
        let accessory = templates.get(path).map(|t| t.spawn(world, None));
        match (accessory, inventory.uniform) {
            (Some(accessory), Some(uniform)) => {
                world.entity_mut(accessory).insert(Networked);
                crate::storage::finish_item(world, accessory, 0);
                if !crate::clothing::accessories::pin(world, accessory, uniform) {
                    world.despawn(accessory);
                }
            }
            (Some(accessory), None) => {
                warn!("{}: accessory {path} without a uniform", outfit.path);
                world.despawn(accessory);
            }
            (None, _) => warn!("{}: no prototype for {path}; no accessory", outfit.path),
        }
    }
    inventory
}

/// The outfit's `back` item of type `path` on `mob`'s back: a storage (a backpack, a satchel).
/// `None` without its prototype or when it is not one.
pub fn spawn_back(world: &mut World, mob: Entity, path: &str) -> Option<Entity> {
    let pack = world.resource_scope(|world, templates: Mut<OutfitTemplates>| {
        Some(templates.0.get(path)?.spawn(world, None))
    })?;
    crate::storage::finish_item(world, pack, 0);
    if world.get::<crate::storage::Storage>(pack).is_none() {
        world.despawn(pack);
        return None;
    }
    world
        .entity_mut(pack)
        .insert((Networked, crate::inventory::CarriedBy(mob)));
    Some(pack)
}

/// tg `equip`: `box` first, then every `backpack_contents` entry (`count` times) into the
/// backpack (`equip_to_storage(..., del_on_fail = TRUE)`: what does not fit is deleted).
pub fn fill_backpack(world: &mut World, pack: Entity, outfit: &Outfit) {
    fill_storage(world, pack, &outfit.backpack_contents, &outfit.path);
}

/// `contents` (type, count) into `pack`, as [`fill_backpack`]; a `pack` without storage takes
/// nothing.
pub fn fill_storage(world: &mut World, pack: Entity, contents: &[(String, u32)], outfit: &str) {
    if world.get::<crate::storage::Storage>(pack).is_none() {
        return;
    }
    for (path, count) in contents {
        for _ in 0..*count {
            let item = world.resource_scope(|world, templates: Mut<OutfitTemplates>| {
                Some(templates.0.get(path)?.spawn(world, None))
            });
            let Some(item) = item else {
                warn!("{outfit}: no prototype for {path}; not in the storage");
                break;
            };
            crate::storage::finish_item(world, item, 1);
            if crate::storage::can_insert(world, pack, item).is_err() {
                world.despawn(item);
                continue;
            }
            world.entity_mut(item).insert(Networked);
            crate::storage::put_inside(world, item, pack);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_matches_tg_update_label() {
        assert_eq!(
            id_label("Аарон Адамс", "Офицер"),
            "ID-карта «Аарон Адамс» (Офицер)"
        );
    }
}
