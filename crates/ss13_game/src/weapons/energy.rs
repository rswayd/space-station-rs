//! The double-bladed energy sword (`/obj/item/dualsaber`,
//! `code/game/objects/items/weaponry/melee/dualsaber.dm`): wielded (`crate::weapons::two_handed`,
//! 3 -> 40 force) its blades come out (`on_wield`): `WEIGHT_CLASS_BULKY`, `blade1.ogg`, edged,
//! `dualsaber<colour>1`; unwielded it is a small dull hilt again (`on_unwield`,
//! `get_sharpness`). The colour is the subtype's or a random one (`possible_colors`). The
//! one-handed energy weapons (swords, the axe) toggle in `crate::antag::syndie_items`.
// shortcut: no jedi spin, clumsy impaling, hulk refusal, light or multitool rainbow.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, DmWeight, DmWounding};

use crate::combat::CombatRng;

pub const DUALSABER: &str = "/obj/item/dualsaber";
/// `possible_colors`.
const COLORS: [&str; 4] = ["red", "blue", "green", "purple"];
/// `w_class_on`, `SHARP_EDGED`, `hit_wield`.
const WEIGHT_CLASS_BULKY: u8 = 4;
const SHARP_EDGED: u32 = 1;
const HIT_WIELD: &str = "sound/items/weapons/blade1.ogg";

/// A double-bladed sword's colour and the vars its unwielded hilt had.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct DualSaber {
    pub color: String,
    hitsound: String,
    w_class: u8,
    sharpness: u32,
}

/// tg `/obj/item/dualsaber/on_wield`, `on_unwield` and `update_icon_state`.
pub fn on_wield(world: &mut World, item: Entity, wielded: bool) {
    let Some(path) = world.get::<DmItem>(item).map(|d| d.path.clone()) else {
        return;
    };
    if !is_under(&path, DUALSABER) {
        return;
    }
    if world.get::<DualSaber>(item).is_none() {
        let color = COLORS
            .iter()
            .find(|c| path.ends_with(&format!("/{c}")))
            .map(|c| (*c).to_owned())
            .unwrap_or_else(|| {
                let i = world
                    .get_resource_mut::<CombatRng>()
                    .map_or(2, |mut r| r.range(0, 3));
                COLORS[i as usize].to_owned()
            });
        let saber = DualSaber {
            color,
            hitsound: world
                .get::<DmItem>(item)
                .map(|d| d.hitsound.clone())
                .unwrap_or_default(),
            w_class: world.get::<DmWeight>(item).map_or(2, |w| w.w_class),
            sharpness: world
                .get::<DmWounding>(item)
                .map_or(SHARP_EDGED, |w| w.sharpness),
        };
        world.entity_mut(item).insert(saber);
    }
    let saber = world.get::<DualSaber>(item).cloned().expect("inserted");
    let state = if wielded {
        format!("dualsaber{}1", saber.color)
    } else {
        "dualsaber0".to_owned()
    };
    if let Some(mut dm) = world.get_mut::<DmItem>(item) {
        dm.hitsound = if wielded {
            HIT_WIELD.to_owned()
        } else {
            saber.hitsound.clone()
        };
        dm.inhand_icon_state = state.clone();
    }
    let w_class = if wielded {
        WEIGHT_CLASS_BULKY
    } else {
        saber.w_class
    };
    world.entity_mut(item).insert(DmWeight { w_class });
    // `get_sharpness`: NONE unless wielded.
    let sharpness = if wielded { saber.sharpness } else { 0 };
    match world.get_mut::<DmWounding>(item) {
        Some(mut w) => w.sharpness = sharpness,
        None => {
            world.entity_mut(item).insert(DmWounding {
                sharpness,
                ..default()
            });
        }
    }
    crate::obj_damage::set_icon_state(world, item, &state);
}

/// Whether an energy weapon is out: a wielded double-bladed sword, a switched-on blade
/// (`TRAIT_TRANSFORM_ACTIVE`). Their `hit_reaction` blocks nothing otherwise.
pub fn is_on(world: &World, item: Entity) -> bool {
    let Some(path) = world.get::<DmItem>(item).map(|d| d.path.as_str()) else {
        return false;
    };
    if is_under(path, DUALSABER) {
        crate::weapons::two_handed::is_wielded(world, item)
    } else if crate::antag::syndie_items::is_blade(path) {
        world
            .get::<crate::antag::syndie_items::EnergyBlade>(item)
            .is_some_and(|b| b.active)
    } else {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::{CarriedBy, Inventory};
    use ss13_net::chat::server::SendChat;
    use ss13_shared::TilePos;

    /// Wielded, the red double-bladed sword hits for 40 with its blade sound, bulky and edged;
    /// let go, it is a 3-force small hilt and blocks nothing.
    #[test]
    fn dualsaber_blades_come_out_when_wielded() {
        let mut world = World::new();
        world.init_resource::<Messages<SendChat>>();
        world.init_resource::<Messages<crate::sound::PlaySound>>();
        let saber = world
            .spawn((
                DmItem {
                    path: "/obj/item/dualsaber/red".into(),
                    force: 3.0,
                    hitsound: "swing_hit".into(),
                    ..default()
                },
                ss13_mapload::DmMelee {
                    two_hand_force: 40.0,
                    block_chance: 75.0,
                    ..default()
                },
                DmWeight { w_class: 2 },
                DmWounding {
                    sharpness: SHARP_EDGED,
                    ..default()
                },
            ))
            .id();
        let mob = world
            .spawn((
                Inventory {
                    hands: [Some(saber), None],
                    ..default()
                },
                TilePos::new(1, 1, 1),
            ))
            .id();
        world.entity_mut(saber).insert(CarriedBy(mob));
        assert!(!is_on(&world, saber));
        assert!(crate::weapons::two_handed::wield(
            &mut world, None, mob, saber
        ));
        let dm = world.get::<DmItem>(saber).unwrap();
        assert_eq!((dm.force, dm.hitsound.as_str()), (40.0, HIT_WIELD));
        assert_eq!(dm.inhand_icon_state, "dualsaberred1");
        assert_eq!(
            world.get::<DmWeight>(saber).unwrap().w_class,
            WEIGHT_CLASS_BULKY
        );
        assert_eq!(
            world.get::<DmWounding>(saber).unwrap().sharpness,
            SHARP_EDGED
        );
        assert!(is_on(&world, saber));
        crate::weapons::two_handed::unwield(&mut world, saber, None, true);
        let dm = world.get::<DmItem>(saber).unwrap();
        assert_eq!((dm.force, dm.hitsound.as_str()), (3.0, "swing_hit"));
        assert_eq!(world.get::<DmWeight>(saber).unwrap().w_class, 2);
        assert_eq!(world.get::<DmWounding>(saber).unwrap().sharpness, 0);
        assert!(!is_on(&world, saber));
    }
}
