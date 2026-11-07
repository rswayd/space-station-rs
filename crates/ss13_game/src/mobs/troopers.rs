//! NPC troopers (server), after tg `/mob/living/basic/trooper` (`code/modules/mob/living/basic/
//! trooper/`): Syndicate, Nanotrasen, Russian, pirate and abductor soldiers. Their trees are
//! `crate::mobs::ai::trees` (`trooper*.bt.json`); here are their guns (`ranged_attacks` with a
//! casing, its sound, cooldown and burst) and their `death_drops` (`loot` plus `corpse`).
// shortcut: a `corpse` that is a `/obj/effect/mob_spawn/corpse/human/*` (a dressed human body)
// drops nothing, as no corpse spawner is ported; the gib spawner and the item loot drop.
// Left out: `apply_dynamic_human_appearance` (they keep their `simple_human.dmi` sprite), the
// shield's projectile deflection of sword troopers, the space troopers' light and spacewalk, the
// pirates' plundering, `ranged_mob_full_auto` (only for player-controlled troopers).

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::mobs::MobDef;
use ss13_mapload::{CasingData, DmCasing};
use ss13_shared::TilePos;

use super::ranged::RangedAttacks;
use crate::effects::gibs::{Gibspawner, spawn_gibs};

pub const ROOT: &str = "/mob/living/basic/trooper";

/// A trooper's `ranged_attacks`: `casingtype`, `projectilesound`, `ranged_cooldown` (seconds),
/// `burst_shots`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gun {
    pub casing: &'static str,
    pub sound: &'static str,
    pub cooldown: f64,
    pub burst: u32,
}

const C9MM: &str = "/obj/item/ammo_casing/c9mm";
const C45: &str = "/obj/item/ammo_casing/c45";
const C46X30: &str = "/obj/item/ammo_casing/c46x30mm";
const A223_WEAK: &str = "/obj/item/ammo_casing/a223/weak";
const BUCKSHOT: &str = "/obj/item/ammo_casing/shotgun/buckshot";
const N762: &str = "/obj/item/ammo_casing/n762";
const LASER: &str = "/obj/item/ammo_casing/energy/laser";
const LASERGUN: &str = "/obj/item/ammo_casing/energy/lasergun";

const PISTOL: &str = "sound/items/weapons/gun/pistol/shot.ogg";
const PISTOL_ALT: &str = "sound/items/weapons/gun/pistol/shot_alt.ogg";
const SMG: &str = "sound/items/weapons/gun/smg/shot.ogg";
const SMG_SUPPRESSED: &str = "sound/items/weapons/gun/smg/shot_suppressed.ogg";
const REVOLVER: &str = "sound/items/weapons/gun/revolver/shot.ogg";
const LASER_SOUND: &str = "sound/items/weapons/laser.ogg";
const LASER2_SOUND: &str = "sound/items/weapons/laser2.ogg";

/// The casings troopers fire (`GAME_RUNTIME_TYPES` keeps their prototypes).
pub const CASINGS: &[&str] = &[
    C9MM, C45, C46X30, A223_WEAK, BUCKSHOT, N762, LASER, LASERGUN,
];

const fn gun(casing: &'static str, sound: &'static str, cooldown: f64, burst: u32) -> Gun {
    Gun {
        casing,
        sound,
        cooldown,
        burst,
    }
}

/// The armed troopers, most specific first. Pirates fire `/obj/projectile/beam/laser` directly
/// (`projectile_type`): the laser casing fires the same projectile.
const GUNS: &[(&str, Gun)] = &[
    (
        "syndicate/ranged/infiltrator",
        gun(C9MM, SMG_SUPPRESSED, 1.0, 1),
    ),
    ("syndicate/ranged/smg", gun(C45, SMG, 3.0, 3)),
    // `syndicate/ranged`'s `projectilesound`: "buckshot ... fired in a two-round burst".
    ("syndicate/ranged/shotgun", gun(BUCKSHOT, PISTOL, 3.0, 2)),
    ("syndicate/ranged", gun(C9MM, PISTOL, 1.0, 1)),
    ("nanotrasen/ranged/smg", gun(C46X30, SMG, 3.0, 3)),
    ("nanotrasen/ranged/assault", gun(A223_WEAK, SMG, 3.0, 4)),
    ("nanotrasen/ranged/elite", gun(LASER, LASER_SOUND, 5.0, 3)),
    ("nanotrasen/ranged", gun(C45, PISTOL_ALT, 1.0, 1)),
    ("russian/ranged", gun(N762, REVOLVER, 1.0, 1)),
    ("pirate/ranged", gun(LASER, LASER_SOUND, 6.0, 2)),
    ("abductor/ranged", gun(LASERGUN, LASER2_SOUND, 5.0, 1)),
];

fn sub(path: &str, rel: &str) -> bool {
    is_under(path, &format!("{ROOT}/{rel}"))
}

/// The gun of trooper type `path`, if it shoots.
pub fn gun_of(path: &str) -> Option<Gun> {
    GUNS.iter().find(|(rel, _)| sub(path, rel)).map(|&(_, g)| g)
}

/// What a dead trooper leaves (`loot` with its `corpse`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Drops {
    /// `corpse = /obj/effect/gibspawner/human`.
    pub gibs: bool,
    pub items: Vec<&'static str>,
}

/// The `loot` items troopers drop (`GAME_RUNTIME_TYPES`).
pub const LOOT: &[&str] = &[
    "/obj/item/knife/kitchen",
    "/obj/item/gun/ballistic/revolver/nagant",
    "/obj/item/melee/energy/sword/pirate",
];

/// `loot` and `corpse` of trooper type `path`, as its DM type chain sets them.
pub fn drops_of(path: &str) -> Drops {
    let gibs = (sub(path, "syndicate/space")
        || sub(path, "syndicate/melee")
        || sub(path, "syndicate/ranged")
        || sub(path, "nanotrasen/ranged/elite")
        || sub(path, "abductor/melee")
        || sub(path, "abductor/ranged"))
        // A human corpse spawner again, or none at all.
        && !sub(path, "syndicate/ranged/infiltrator")
        && !sub(path, "syndicate/ranged/smg/orion")
        && !sub(path, "syndicate/ranged/smg/pilot");
    let items = if sub(path, "russian/ranged/lootless") || sub(path, "pirate/melee/space") {
        Vec::new()
    } else if sub(path, "russian/ranged") {
        vec![LOOT[1]]
    } else if sub(path, "russian") {
        vec![LOOT[0]]
    } else if sub(path, "pirate/melee") {
        vec![LOOT[2]]
    } else {
        Vec::new()
    };
    Drops { gibs, items }
}

/// The casing prototype's data (its projectile, pellets and spread).
fn casing_data(world: &World, path: &str) -> Option<CasingData> {
    let proto = world.get_resource::<ss13_proto::Prototypes>()?.get(path)?;
    proto.get::<DmCasing>().map(|c| c.data.clone())
}

/// `ranged_attacks` of an armed trooper from its casing's prototype.
pub fn ranged_attacks(gun: Gun, casing: &CasingData) -> RangedAttacks {
    RangedAttacks {
        sound: gun.sound.to_owned(),
        burst: gun.burst.max(1),
        pellets: casing.pellets.max(1),
        variance: casing.variance,
        randomspread: casing.randomspread,
        ..RangedAttacks::new(casing.projectile.clone(), gun.cooldown)
    }
}

pub(super) fn attach(world: &mut World, mob: Entity, def: &MobDef) {
    let Some(gun) = gun_of(&def.path) else {
        return;
    };
    let Some(casing) = casing_data(world, gun.casing) else {
        warn!("trooper {}: no casing prototype {}", def.path, gun.casing);
        return;
    };
    world.entity_mut(mob).insert(ranged_attacks(gun, &casing));
}

/// `death_drops` of a dead trooper at `pos` (before it is deleted, `DEL_ON_DEATH`).
pub(super) fn on_death(world: &mut World, def: &MobDef, pos: TilePos) {
    if !is_under(&def.path, ROOT) {
        return;
    }
    let drops = drops_of(&def.path);
    for item in drops.items {
        crate::construction::spawn_obj(world, item, pos);
    }
    if drops.gibs {
        spawn_gibs(world, Gibspawner::Human, pos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guns_follow_the_type_chain() {
        let p = |rel: &str| format!("{ROOT}/{rel}");
        assert_eq!(gun_of(&p("syndicate/melee/space")), None);
        let space = gun_of(&p("syndicate/ranged/space/stormtrooper")).unwrap();
        assert_eq!((space.casing, space.cooldown, space.burst), (C9MM, 1.0, 1));
        let smg = gun_of(&p("syndicate/ranged/smg/space")).unwrap();
        assert_eq!((smg.casing, smg.burst, smg.cooldown), (C45, 3, 3.0));
        assert_eq!(
            gun_of(&p("syndicate/ranged/shotgun")).unwrap().casing,
            BUCKSHOT
        );
        assert_eq!(gun_of(&p("nanotrasen/ranged/elite")).unwrap().burst, 3);
        assert_eq!(gun_of(&p("russian/ranged/lootless")).unwrap().casing, N762);
        assert_eq!(gun_of(&p("pirate/ranged/space")).unwrap().cooldown, 6.0);
    }

    #[test]
    fn drops_follow_loot_and_corpse() {
        let p = |rel: &str| format!("{ROOT}/{rel}");
        assert!(drops_of(&p("syndicate/melee/space/stormtrooper")).gibs);
        assert!(!drops_of(&p("syndicate")).gibs, "a corpse spawner");
        assert!(!drops_of(&p("syndicate/ranged/infiltrator")).gibs);
        assert_eq!(drops_of(&p("russian")).items, vec![LOOT[0]]);
        assert_eq!(drops_of(&p("russian/ranged")).items, vec![LOOT[1]]);
        assert!(drops_of(&p("russian/ranged/lootless")).items.is_empty());
        assert_eq!(drops_of(&p("pirate/melee")).items, vec![LOOT[2]]);
        assert!(drops_of(&p("pirate/melee/space")).items.is_empty());
    }

    #[test]
    fn casings_and_loot_are_runtime_types() {
        let registry = ss13_mapload::runtime::runtime_types();
        let missing: Vec<&str> = CASINGS
            .iter()
            .chain(LOOT)
            .copied()
            .filter(|p| !registry.contains(p))
            .collect();
        assert!(
            missing.is_empty(),
            "not in ss13_mapload::runtime: {missing:?}"
        );
        assert!(GUNS.iter().all(|(_, g)| CASINGS.contains(&g.casing)));
    }

    #[test]
    fn buckshot_bursts_carry_their_pellets() {
        let casing = CasingData {
            pellets: 6,
            variance: 25.0,
            ..default()
        };
        let r = ranged_attacks(
            gun_of(&format!("{ROOT}/syndicate/ranged/shotgun")).unwrap(),
            &casing,
        );
        assert_eq!((r.pellets, r.burst, r.cooldown), (6, 2, 3.0));
        assert_eq!(r.sound, PISTOL);
    }
}
