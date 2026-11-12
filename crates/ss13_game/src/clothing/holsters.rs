//! Accessories with their own storage (server): BandaStation's accessory holsters
//! (`modular_bandastation/objects/code/items/clothing/accessories/holsters.dm`), its ammo vest
//! (`accessories.dm`) and tg's pocket protector (`under/accessories/badges.dm`), with the
//! `/datum/storage` limits and `set_holdable` lists their `Initialize` gives them
//! (`code/datums/storage/subtypes/holsters.dm`, `pockets.dm`).
//!
//! The storage is created in `Initialize` (`create_storage`), which the converter cannot read, so
//! the limits live in [`SPECS`]. The accessory keeps its own [`Storage`]: in a hand or on the
//! floor it opens like any storage item; pinned to a jumpsuit (tg `try_attach` ->
//! `clone_storage` with the accessory as `real_location`), clicking the jumpsuit opens it or
//! puts the held item in ([`crate::storage::storage_of`]). A jumpsuit takes one such accessory
//! (`can_attach_accessory`: "isn't compatible!"). BandaStation's `update_accessory_weight`: the
//! jumpsuit weighs as much as its heaviest accessory of normal weight or more.
// shortcut: no quick-draw keybind (`quick_accessory_draw`: the port has no keybindings), no
// gun spinning of `TRAIT_GUNFLIP` ([`gunflip`] only lets thermal pistols crank), holster sounds or "tries to rip the holster off" message; the detective
// holster refuses a second gun instead of handing it back with "В кобуре уже есть оружие!".

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, DmWeight};

use super::accessories::{AccessoryOf, attached};
use crate::storage::{Storage, Stored};

const TINY: u8 = 1;
const NORMAL: u8 = 3;
const BULKY: u8 = 4;

/// One storage accessory family: the most specific root first in [`SPECS`].
pub struct Spec {
    pub root: &'static str,
    pub storage: Storage,
    /// `set_holdable`: what goes in (type roots).
    pub holdable: &'static [&'static str],
    /// What `Initialize` itself spawns inside (not `PopulateContents`, which the converter
    /// lowers).
    pub initialize: &'static [&'static str],
}

const fn limits(max_slots: u32, max_specific_storage: u8, max_total_storage: u32) -> Storage {
    Storage {
        max_slots,
        max_specific_storage,
        max_total_storage,
    }
}

/// `/datum/storage/holster/New`.
const HOLSTER: &[&str] = &[
    "/obj/item/gun/ballistic/automatic/pistol",
    "/obj/item/gun/ballistic/revolver",
    "/obj/item/gun/energy/e_gun/mini",
    "/obj/item/gun/energy/disabler",
    "/obj/item/gun/energy/dueling",
    "/obj/item/food/grown/banana",
    "/obj/item/gun/energy/laser/thermal",
    "/obj/item/gun/energy/laser/pistol",
    "/obj/item/gun/ballistic/rifle/boltaction",
    "/obj/item/gun/energy/laser/captain",
    "/obj/item/gun/energy/e_gun/hos",
    "/obj/item/gun/energy/eg_14",
];

/// `/datum/storage/holster/energy/New`.
const ENERGY: &[&str] = &[
    "/obj/item/gun/energy/e_gun/mini",
    "/obj/item/gun/energy/disabler",
    "/obj/item/gun/energy/dueling",
    "/obj/item/food/grown/banana",
    "/obj/item/gun/energy/laser/thermal",
    "/obj/item/gun/energy/recharge/ebow",
    "/obj/item/gun/energy/laser/captain",
    "/obj/item/gun/energy/laser/pistol",
    "/obj/item/gun/energy/e_gun/hos",
    "/obj/item/gun/energy/eg_14",
    "/obj/item/gun/energy/disabler/smoothbore",
    "/obj/item/gun/energy/pulse/pistol",
];

/// `/datum/storage/holster/detective/New`.
const DETECTIVE: &[&str] = &[
    "/obj/item/gun/ballistic/automatic/pistol",
    "/obj/item/ammo_box/magazine/m9mm",
    "/obj/item/ammo_box/magazine/m9mm_aps",
    "/obj/item/ammo_box/magazine/m10mm",
    "/obj/item/ammo_box/magazine/m45",
    "/obj/item/ammo_box/magazine/m50",
    "/obj/item/gun/ballistic/revolver",
    "/obj/item/ammo_box/speedloader",
    "/obj/item/ammo_box/magazine/toy/pistol",
    "/obj/item/gun/energy/e_gun/mini",
    "/obj/item/gun/energy/disabler",
    "/obj/item/gun/energy/dueling",
    "/obj/item/gun/energy/laser/thermal",
    "/obj/item/gun/energy/laser/captain",
    "/obj/item/gun/energy/laser/pistol",
    "/obj/item/gun/energy/e_gun/hos",
    "/obj/item/gun/ballistic/rifle/boltaction",
    "/obj/item/gun/energy/eg_14",
];

/// `/datum/storage/pockets/holster/traitor/New` (the chameleon holster).
const TRAITOR: &[&str] = &[
    "/obj/item/gun/ballistic/automatic/pistol",
    "/obj/item/gun/ballistic/revolver",
    "/obj/item/food/grown/banana",
    "/obj/item/gun/energy/disabler",
    "/obj/item/gun/energy/laser/pistol",
    "/obj/item/gun/energy/laser/thermal",
    "/obj/item/gun/energy/laser/captain",
    "/obj/item/gun/energy/disabler/smoothbore",
    "/obj/item/gun/energy/pulse/pistol",
    "/obj/item/gun/energy/e_gun/hos",
    "/obj/item/gun/energy/eg_14",
    "/obj/item/gun/energy/dueling",
    "/obj/item/gun/energy/e_gun/mini",
];

/// `/datum/storage/pockets/holster/psyker/New`: the traitor list and ammo.
const PSYKER: &[&str] = &[
    "/obj/item/gun/ballistic/automatic/pistol",
    "/obj/item/gun/ballistic/revolver",
    "/obj/item/food/grown/banana",
    "/obj/item/gun/energy/disabler",
    "/obj/item/gun/energy/laser/pistol",
    "/obj/item/gun/energy/laser/thermal",
    "/obj/item/gun/energy/laser/captain",
    "/obj/item/gun/energy/disabler/smoothbore",
    "/obj/item/gun/energy/pulse/pistol",
    "/obj/item/gun/energy/e_gun/hos",
    "/obj/item/gun/energy/eg_14",
    "/obj/item/gun/energy/dueling",
    "/obj/item/gun/energy/e_gun/mini",
    "/obj/item/ammo_box/magazine",
    "/obj/item/ammo_box/speedloader",
];

/// `/datum/storage/holster/nukie/New` (BandaStation's list for the tacticool holster).
const NUKIE: &[&str] = &[
    "/obj/item/gun/ballistic/automatic/pistol",
    "/obj/item/gun/ballistic/revolver",
    "/obj/item/gun/ballistic/shotgun/automatic/combat/compact",
    "/obj/item/gun/ballistic/automatic/cm5/compact",
    "/obj/item/gun/ballistic/automatic/proto/unrestricted",
    "/obj/item/gun/ballistic/automatic/mini_uzi",
    "/obj/item/gun/energy/disabler",
    "/obj/item/gun/energy/laser/pistol",
    "/obj/item/gun/energy/laser/thermal",
    "/obj/item/gun/energy/laser/captain",
    "/obj/item/gun/energy/e_gun/hos",
    "/obj/item/gun/energy/eg_14",
    "/obj/item/gun/energy/dueling",
    "/obj/item/gun/energy/e_gun/mini",
    "/obj/item/knife/combat",
    "/obj/item/ammo_box/magazine",
    "/obj/item/ammo_box/speedloader",
    "/obj/item/ammo_casing",
    "/obj/item/grenade",
];

/// `/datum/storage/pockets/ammo_webbing/New`.
const AMMO: &[&str] = &[
    "/obj/item/ammo_box/magazine",
    "/obj/item/ammo_box/speedloader",
];

/// `/datum/storage/pockets/pocketprotector/New`.
const POCKET_PROTECTOR: &[&str] = &[
    "/obj/item/pen",
    "/obj/item/toy/crayon",
    "/obj/item/lipstick",
    "/obj/item/flashlight/pen",
];

pub const HOLSTER_ROOT: &str = "/obj/item/clothing/accessory/holster";
const DETECTIVE_ROOT: &str = "/obj/item/clothing/accessory/holster/detective";
const GUN_ROOT: &str = "/obj/item/gun";

/// Every storage accessory, most specific root first.
pub static SPECS: [Spec; 9] = [
    Spec {
        root: "/obj/item/clothing/accessory/holster/energy",
        storage: limits(2, NORMAL, 16),
        holdable: ENERGY,
        initialize: &[],
    },
    Spec {
        root: DETECTIVE_ROOT,
        storage: limits(3, NORMAL, 16),
        holdable: DETECTIVE,
        initialize: &[],
    },
    // `/datum/storage/pockets` limits under the psyker's own.
    Spec {
        root: "/obj/item/clothing/accessory/holster/psyker",
        storage: limits(4, NORMAL, 50),
        holdable: PSYKER,
        initialize: &[
            "/obj/item/gun/ballistic/revolver/c38",
            "/obj/item/ammo_box/speedloader/c38",
            "/obj/item/ammo_box/speedloader/c38",
            "/obj/item/ammo_box/speedloader/c38",
        ],
    },
    Spec {
        root: "/obj/item/clothing/accessory/holster/chameleon",
        storage: limits(1, NORMAL, 50),
        holdable: TRAITOR,
        initialize: &[],
    },
    Spec {
        root: "/obj/item/clothing/accessory/holster/tacticool",
        storage: limits(2, BULKY, 16),
        holdable: NUKIE,
        initialize: &[],
    },
    Spec {
        root: HOLSTER_ROOT,
        storage: limits(1, NORMAL, 16),
        holdable: HOLSTER,
        initialize: &[],
    },
    Spec {
        root: "/obj/item/clothing/accessory/ammo_vest",
        storage: limits(4, NORMAL, 9),
        holdable: AMMO,
        initialize: &[],
    },
    Spec {
        root: "/obj/item/clothing/accessory/pocketprotector/full",
        storage: limits(3, TINY, 50),
        holdable: POCKET_PROTECTOR,
        initialize: &["/obj/item/pen/red", "/obj/item/pen", "/obj/item/pen/blue"],
    },
    Spec {
        root: "/obj/item/clothing/accessory/pocketprotector",
        storage: limits(3, TINY, 50),
        holdable: POCKET_PROTECTOR,
        initialize: &[],
    },
];

/// The storage accessory family of `path`, if it is one.
pub fn spec(path: &str) -> Option<&'static Spec> {
    SPECS.iter().find(|s| is_under(path, s.root))
}

fn spec_of(world: &World, item: Entity) -> Option<&'static Spec> {
    spec(&world.get::<DmItem>(item)?.path)
}

/// The types `Initialize` spawns into storage accessories (their templates are built at start).
pub fn content_paths() -> impl Iterator<Item = &'static str> {
    SPECS.iter().flat_map(|s| s.initialize.iter().copied())
}

/// `create_storage` + the `Initialize` contents of a storage accessory at `path`.
pub fn initialize_contents(path: &str) -> &'static [&'static str] {
    spec(path).map_or(&[], |s| s.initialize)
}

/// `can_hold` of a storage accessory: true when it refuses `item`. False for other storages.
pub fn refuses(world: &World, storage: Entity, item: Entity) -> bool {
    let Some(spec) = spec_of(world, storage) else {
        return false;
    };
    let Some(path) = world.get::<DmItem>(item).map(|i| i.path.as_str()) else {
        return true;
    };
    if !spec.holdable.iter().any(|root| is_under(path, root)) {
        return true;
    }
    // BandaStation's detective holster holds one gun (`Entered`).
    is_under(spec.root, DETECTIVE_ROOT)
        && is_under(path, GUN_ROOT)
        && world.get::<Stored>(storage).is_some_and(|s| {
            s.items().iter().any(|&e| {
                e != item
                    && world
                        .get::<DmItem>(e)
                        .is_some_and(|i| is_under(&i.path, GUN_ROOT))
            })
        })
}

/// The storage accessory pinned to `uniform` (tg: the jumpsuit's cloned `atom_storage`).
pub fn storage_accessory(world: &World, uniform: Entity) -> Option<Entity> {
    attached(world, uniform)
        .into_iter()
        .find(|&a| world.get::<Storage>(a).is_some())
}

/// tg `TRAIT_GUNFLIP`: a belt holster worn on the belt or the suit storage, a wand bandolier on
/// the suit storage, or BandaStation's holster accessory pinned to the worn jumpsuit.
pub fn gunflip(world: &World, mob: Entity) -> bool {
    use ss13_shared::protocol::InventorySlot;
    let Some(inv) = world.get::<crate::inventory::Inventory>(mob) else {
        return false;
    };
    let path = |slot| {
        inv.slot(slot)
            .and_then(|e| world.get::<DmItem>(e))
            .map(|i| i.path.as_str())
    };
    let holster =
        |p: Option<&str>| p.is_some_and(|p| is_under(p, "/obj/item/storage/belt/holster"));
    holster(path(InventorySlot::Belt))
        || holster(path(InventorySlot::SuitStorage))
        || path(InventorySlot::SuitStorage)
            .is_some_and(|p| is_under(p, "/obj/item/storage/belt/wand_bandolier"))
        || inv.slot(InventorySlot::Uniform).is_some_and(|u| {
            attached(world, u).into_iter().any(|a| {
                world
                    .get::<DmItem>(a)
                    .is_some_and(|i| is_under(&i.path, HOLSTER_ROOT))
            })
        })
}

/// The jumpsuit a pinned accessory is on.
pub fn jumpsuit_of(world: &World, accessory: Entity) -> Option<Entity> {
    world.get::<AccessoryOf>(accessory).map(|a| a.0)
}

/// The jumpsuit's `w_class` before accessories weighed on it.
#[derive(Component, Clone, Copy, Debug)]
pub struct BaseWeight(pub u8);

/// BandaStation `update_accessory_weight`: the heaviest accessory of `WEIGHT_CLASS_NORMAL` or
/// more sets the jumpsuit's weight, back to its own without one.
pub fn update_weight(world: &mut World, uniform: Entity) {
    let weight = |world: &World, e: Entity| world.get::<DmWeight>(e).map(|w| w.w_class);
    let Some(current) = weight(world, uniform) else {
        return;
    };
    let base = match world.get::<BaseWeight>(uniform) {
        Some(b) => b.0,
        None => {
            world.entity_mut(uniform).insert(BaseWeight(current));
            current
        }
    };
    let heaviest = attached(world, uniform)
        .into_iter()
        .filter_map(|a| weight(world, a))
        .filter(|&w| w >= NORMAL)
        .max()
        .unwrap_or(0);
    let wanted = base.max(heaviest);
    if wanted != current
        && let Some(mut w) = world.get_mut::<DmWeight>(uniform)
    {
        w.w_class = wanted;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(world: &mut World, path: &str, w_class: u8) -> Entity {
        world
            .spawn((
                DmItem {
                    path: path.into(),
                    ..default()
                },
                DmWeight { w_class },
            ))
            .id()
    }

    #[test]
    fn the_most_specific_holster_wins() {
        assert_eq!(
            spec("/obj/item/clothing/accessory/holster/energy/disabler")
                .unwrap()
                .storage
                .max_slots,
            2
        );
        assert_eq!(
            spec("/obj/item/clothing/accessory/holster/tacticool/cowboy")
                .unwrap()
                .storage
                .max_specific_storage,
            BULKY
        );
        assert_eq!(
            spec("/obj/item/clothing/accessory/holster").unwrap().root,
            HOLSTER_ROOT
        );
        assert!(spec("/obj/item/clothing/accessory/medal").is_none());
        assert_eq!(
            initialize_contents("/obj/item/clothing/accessory/pocketprotector/full").len(),
            3
        );
    }

    #[test]
    fn holsters_hold_guns_only_and_detectives_one_gun() {
        let mut world = World::new();
        let holster = item(&mut world, HOLSTER_ROOT, BULKY);
        let pistol = item(
            &mut world,
            "/obj/item/gun/ballistic/automatic/pistol/m1911",
            2,
        );
        let wrench = item(&mut world, "/obj/item/wrench", 2);
        assert!(!refuses(&world, holster, pistol));
        assert!(refuses(&world, holster, wrench));
        let detective = item(
            &mut world,
            "/obj/item/clothing/accessory/holster/detective/full",
            BULKY,
        );
        let revolver = item(
            &mut world,
            "/obj/item/gun/ballistic/revolver/c38/detective",
            2,
        );
        let loader = item(&mut world, "/obj/item/ammo_box/speedloader/c38", 1);
        world
            .entity_mut(revolver)
            .insert(crate::storage::StoredIn(detective));
        assert!(refuses(&world, detective, pistol));
        assert!(!refuses(&world, detective, loader));
    }

    #[test]
    fn a_heavy_accessory_weighs_on_the_jumpsuit() {
        let mut world = World::new();
        let uniform = item(
            &mut world,
            "/obj/item/clothing/under/rank/security/officer",
            3,
        );
        let holster = item(&mut world, HOLSTER_ROOT, BULKY);
        world.entity_mut(holster).insert(AccessoryOf(uniform));
        update_weight(&mut world, uniform);
        assert_eq!(world.get::<DmWeight>(uniform).unwrap().w_class, BULKY);
        world.entity_mut(holster).remove::<AccessoryOf>();
        update_weight(&mut world, uniform);
        assert_eq!(world.get::<DmWeight>(uniform).unwrap().w_class, 3);
    }

    #[test]
    fn a_pinned_holster_opens_from_its_jumpsuit() {
        let mut world = World::new();
        let uniform = item(
            &mut world,
            "/obj/item/clothing/under/rank/security/officer",
            3,
        );
        let holster = item(&mut world, HOLSTER_ROOT, BULKY);
        world
            .entity_mut(holster)
            .insert((spec(HOLSTER_ROOT).unwrap().storage, AccessoryOf(uniform)));
        let mob = world.spawn_empty().id();
        world
            .entity_mut(uniform)
            .insert(crate::inventory::CarriedBy(mob));
        assert_eq!(crate::storage::storage_of(&world, uniform), Some(holster));
        assert_eq!(crate::storage::storage_of(&world, holster), Some(holster));
        assert!(crate::storage::reachable(&world, mob, holster));
        let pistol = item(&mut world, "/obj/item/gun/ballistic/automatic/pistol", 2);
        assert!(crate::storage::can_insert(&world, holster, pistol).is_ok());
        world
            .entity_mut(pistol)
            .insert(crate::storage::StoredIn(holster));
        let other = item(&mut world, "/obj/item/gun/ballistic/revolver", 2);
        assert_eq!(
            crate::storage::can_insert(&world, holster, other),
            Err(crate::storage::Refusal::Full)
        );
    }
}
