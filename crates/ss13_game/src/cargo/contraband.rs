//! Contraband (server): tg `TRAIT_CONTRABAND` and `/obj/item/proc/is_contraband()`
//! (`code/game/objects/items.dm`), with BandaStation's `modular_bandastation/contraband/`.
//!
//! - [`Contraband`] is the trait given at runtime: black market purchases from an illegal
//!   market (`crate::cargo::black_market`).
//! - The types that add it in their own `Initialize` (upstream's drugs, syndicate gear, cult
//!   blades, contraband posters, the black market uplink; BandaStation's 9 mm magazines, SMGs
//!   and pistols, energy weapons, the energy katana, bomb collar, EMP grenade, boarding axe,
//!   combat defibrillator, MOD suits...) count with their subtypes, except the null rod's
//!   energy sword, which removes it.
//! - Else an item of a traitor uplink item's type counts when that item trips contraband
//!   scanners (`SYNDIE_TRIPS_CONTRABAND`).
// shortcut: the readers of `is_contraband` (N-spect scanners, the security contraband bounty)
// and the trait given to traitor, heretic, cult and wizard purchases at runtime belong to
// their own groups; until they land, those items count by type only.

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;

/// `TRAIT_CONTRABAND` given at runtime.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Contraband;

/// Types whose `Initialize` adds `TRAIT_CONTRABAND` (`INNATE_TRAIT`).
pub const INNATE_CONTRABAND: &[&str] = &[
    // Upstream.
    "/obj/item/ai_module/combat",
    "/obj/item/ai_module/upgrade/surveillance",
    "/obj/item/autosurgeon/syndicate",
    "/obj/item/borg/upgrade/syndicate",
    "/obj/item/food/drug/meth_crystal",
    "/obj/item/food/drug/moon_rock",
    "/obj/item/food/drug/opium",
    "/obj/item/food/drug/saturnx",
    "/obj/item/food/grown/kronkus",
    "/obj/item/jammer/makeshift",
    "/obj/item/market_uplink/blackmarket",
    "/obj/item/melee/cultblade",
    "/obj/item/poster/random_contraband",
    "/obj/item/reagent_containers/cup/beaker/unholywater",
    "/obj/item/reagent_containers/cup/blastoff_ampoule",
    "/obj/item/reagent_containers/syringe/contraband",
    "/obj/item/seeds/kronkus",
    "/obj/item/skillchip/self_surgery",
    "/obj/item/sticker/syndicate",
    "/obj/item/survivalcapsule/fishing",
    // BandaStation (`modular_bandastation/contraband/`).
    "/obj/item/ammo_box/magazine/m9mm",
    "/obj/item/ammo_box/magazine/m9mm_aps",
    "/obj/item/ammo_box/magazine/uzim9mm",
    "/obj/item/claymore/cutlass",
    "/obj/item/clothing/neck/collar_bomb",
    "/obj/item/defibrillator/compact/combat/loaded",
    "/obj/item/energy_katana",
    "/obj/item/fireaxe/boardingaxe",
    "/obj/item/grenade/c4/ninja",
    "/obj/item/grenade/empgrenade",
    "/obj/item/gun/ballistic/automatic/as32",
    "/obj/item/gun/ballistic/automatic/bison",
    "/obj/item/gun/ballistic/automatic/carwo",
    "/obj/item/gun/ballistic/automatic/dshk",
    "/obj/item/gun/ballistic/automatic/fn18",
    "/obj/item/gun/ballistic/automatic/fn4",
    "/obj/item/gun/ballistic/automatic/lanca",
    "/obj/item/gun/ballistic/automatic/miecz",
    "/obj/item/gun/ballistic/automatic/pmk",
    "/obj/item/gun/ballistic/automatic/sabel",
    "/obj/item/gun/ballistic/automatic/sindano",
    "/obj/item/gun/ballistic/automatic/vityaz",
    "/obj/item/gun/ballistic/automatic/wylom",
    "/obj/item/gun/ballistic/rifle/hlrm",
    "/obj/item/gun/ballistic/shotgun/riot/renoster",
    "/obj/item/implanter/emp",
    "/obj/item/melee/energy",
    "/obj/item/mod/control/pre_equipped/cosmohonk",
    "/obj/item/mod/control/pre_equipped/elite",
    "/obj/item/mod/control/pre_equipped/enchanted",
    "/obj/item/mod/control/pre_equipped/interdyne",
    "/obj/item/mod/control/pre_equipped/ninja",
    "/obj/item/mod/control/pre_equipped/nuclear",
    "/obj/item/mod/control/pre_equipped/responsory/inquisitory/syndie",
    "/obj/item/mod/control/pre_equipped/traitor",
    "/obj/item/mod/control/pre_equipped/traitor_elite",
    "/obj/item/restraints/legcuffs/bola/tactical",
];

/// Subtypes whose `Initialize` removes the trait again.
const NOT_CONTRABAND: [&str; 1] = ["/obj/item/melee/energy/sword/nullrod"];

/// The item has `TRAIT_CONTRABAND` (runtime or innate).
pub fn has_trait(world: &World, item: Entity) -> bool {
    if world.get::<Contraband>(item).is_some() {
        return true;
    }
    world.get::<DmItem>(item).is_some_and(|i| innate(&i.path))
}

/// `Initialize` of `path` leaves `TRAIT_CONTRABAND` on it.
pub fn innate(path: &str) -> bool {
    INNATE_CONTRABAND.iter().any(|root| is_under(path, root))
        && !NOT_CONTRABAND.iter().any(|root| is_under(path, root))
}

/// tg `is_contraband`: the trait, else the first traitor uplink item of its type decides.
pub fn is_contraband(world: &World, item: Entity) -> bool {
    if has_trait(world, item) {
        return true;
    }
    let Some(path) = world.get::<DmItem>(item).map(|i| i.path.as_str()) else {
        return false;
    };
    let Some(catalog) = world.get_resource::<crate::antag::uplink::UplinkCatalog>() else {
        return false;
    };
    catalog
        .data
        .uplink_items
        .iter()
        .find(|u| !u.item.is_empty() && is_under(path, &u.item))
        .is_some_and(|u| u.uplink_item_flags & ss13_mapload::antag::SYNDIE_TRIPS_CONTRABAND != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn innate_types_count_with_subtypes_but_the_null_rod() {
        assert!(innate("/obj/item/melee/energy/sword/saber/blue"));
        assert!(innate("/obj/item/ammo_box/magazine/m9mm/fire"));
        assert!(innate("/obj/item/mod/control/pre_equipped/traitor"));
        assert!(!innate("/obj/item/melee/energy/sword/nullrod"));
        assert!(!innate("/obj/item/ammo_box/magazine/m10mm"));
        assert!(!innate("/obj/item/toy/sword"));
    }
}
