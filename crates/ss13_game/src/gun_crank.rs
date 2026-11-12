//! Hand-cranked energy guns (server): tg `/datum/component/crank_recharge`
//! (`code/datums/components/crank_recharge.dm`) on the guns of `guns/energy/crank_guns.dm`:
//! the laser musket (and the heroic one), the smoothbore disabler (and the elite one), the
//! thermal pistols (nanite, inferno, cryo) and the iconoclast's repeater. Using the gun in hand
//! (Z) cranks it: a `do_after` of `cooldown_time` adds `charge_amount` to the cell. The musket
//! and the smoothbore crank on the move; the thermal pistols must be spun, which needs a holster
//! (`TRAIT_GUNFLIP`, [`crate::clothing::holsters::gunflip`]).
// shortcut: no spin animation of the thermal pistols and no charged in-hand sprite of the musket;
// add with item animations.

use bevy::prelude::*;
use ss13_mapload::DmGun;
use ss13_mapload::fidelity::is_under;

use crate::guns::Gun;

/// tg `STANDARD_CELL_CHARGE`, joules.
const STANDARD_CELL_CHARGE: f64 = 10_000.0;

/// One gun family's `crank_recharge` arguments.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crank {
    pub root: &'static str,
    pub spin_to_win: bool,
    pub charge_amount: f64,
    /// `cooldown_time`, deciseconds.
    pub cooldown_ds: f32,
    pub sound: &'static str,
    /// `charge_sound_cooldown_time`, seconds.
    pub sound_cooldown: f64,
    /// `charge_move = IGNORE_USER_LOC_CHANGE`.
    pub moving: bool,
}

/// Most specific first.
pub static CRANKS: [Crank; 4] = [
    Crank {
        root: "/obj/item/gun/energy/laser/musket/repeater",
        spin_to_win: false,
        // `LASER_SHOTS(6, STANDARD_CELL_CHARGE)`.
        charge_amount: (100.0 * STANDARD_CELL_CHARGE - 4.0) / 600.0,
        cooldown_ds: 4.0,
        sound: "sound/machines/clockcult/integration_cog_install.ogg",
        sound_cooldown: 3.0,
        moving: false,
    },
    Crank {
        root: "/obj/item/gun/energy/laser/musket",
        spin_to_win: false,
        charge_amount: STANDARD_CELL_CHARGE * 0.5,
        cooldown_ds: 20.0,
        sound: "sound/items/weapons/laser_crank.ogg",
        sound_cooldown: 1.8,
        moving: true,
    },
    Crank {
        root: "/obj/item/gun/energy/disabler/smoothbore",
        spin_to_win: false,
        charge_amount: STANDARD_CELL_CHARGE,
        cooldown_ds: 20.0,
        sound: "sound/items/weapons/laser_crank.ogg",
        sound_cooldown: 1.8,
        moving: true,
    },
    Crank {
        root: "/obj/item/gun/energy/laser/thermal",
        spin_to_win: true,
        // `LASER_SHOTS(8, STANDARD_CELL_CHARGE)`.
        charge_amount: STANDARD_CELL_CHARGE / 8.0,
        // BandaStation's thermal pistol balance: 1.5 s (tg 0.8 s).
        cooldown_ds: 15.0,
        sound: "sound/items/weapons/kinetic_reload.ogg",
        sound_cooldown: 1.5,
        moving: false,
    },
];

pub fn crank_of(path: &str) -> Option<&'static Crank> {
    CRANKS.iter().find(|c| is_under(path, c.root))
}

/// `COOLDOWN_DECLARE(charge_sound_cooldown)`: when the gun may play its sound again.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct CrankSoundAt(pub f64);

/// `on_attack_self` -> `crank`: true when the gun is a crank gun (the attack chain ends).
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let Some(path) = world.get::<DmGun>(item).map(|d| d.path.clone()) else {
        return false;
    };
    let Some(crank) = crank_of(&path) else {
        return false;
    };
    let full = world
        .get::<Gun>(item)
        .zip(world.get::<DmGun>(item))
        .is_none_or(|(g, d)| g.charge >= d.cell_maxcharge);
    if full {
        crate::balloon::balloon_alert(world, mob, item, "уже заряжено!");
        return true;
    }
    // `is_charging`: one crank at a time.
    if world.get::<crate::tools::DoAfter>(mob).is_some() {
        return true;
    }
    if crank.spin_to_win && !crate::clothing::holsters::gunflip(world, mob) {
        crate::balloon::balloon_alert(world, mob, item, "нужна кобура, чтобы крутить!");
        return true;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    if world.get::<CrankSoundAt>(item).is_none_or(|c| now >= c.0) {
        world
            .entity_mut(item)
            .insert(CrankSoundAt(now + crank.sound_cooldown));
        if let Some(&pos) = world.get::<ss13_shared::TilePos>(mob) {
            crate::sound::playsound(
                world,
                crate::sound::PlaySound::new(pos, crank.sound, 40.0, true),
            );
        }
    }
    crate::balloon::balloon_alert(world, mob, item, "заряжается...");
    let actor = crate::tools::Actor {
        client,
        mob,
        item: Some(item),
    };
    crate::tools::do_after(
        world,
        actor,
        mob,
        crank.cooldown_ds,
        0,
        crate::construction::Job::Crank {
            moving: crank.moving,
        },
    );
    true
}

/// The crank's `do_after` finished: the cell gets `charge_amount` (`give`, up to its maximum).
pub fn complete(world: &mut World, mob: Entity, action: &crate::tools::DoAfter) {
    let Some(item) = action.item else {
        return;
    };
    let Some(crank) = world.get::<DmGun>(item).and_then(|d| crank_of(&d.path)) else {
        return;
    };
    let Some(max) = world.get::<DmGun>(item).map(|d| d.cell_maxcharge) else {
        return;
    };
    if let Some(mut gun) = world.get_mut::<Gun>(item) {
        gun.charge = (gun.charge + crank.charge_amount).min(max);
    }
    crate::balloon::balloon_alert(world, mob, item, "заряжено");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crank_families_match_tg() {
        assert_eq!(
            crank_of("/obj/item/gun/energy/laser/musket/repeater").map(|c| c.cooldown_ds),
            Some(4.0)
        );
        let musket = crank_of("/obj/item/gun/energy/laser/musket/prime").unwrap();
        assert_eq!(musket.charge_amount, 5000.0);
        assert!(musket.moving);
        let cryo = crank_of("/obj/item/gun/energy/laser/thermal/cryo").unwrap();
        assert!(cryo.spin_to_win);
        assert_eq!(cryo.charge_amount, 1250.0);
        assert!(crank_of("/obj/item/gun/energy/laser").is_none());
        // Six repeater shots fill the cell.
        let repeater = &CRANKS[0];
        assert!((repeater.charge_amount * 6.0 - STANDARD_CELL_CHARGE).abs() < 1.0);
    }
}
