//! Worn gear with buttons and modes of its own (server): night vision goggles
//! (`/datum/action/item_action/toggle_nv`, `code/datums/actions/items/toggles.dm`), magboots
//! (`code/modules/clothing/shoes/magboots.dm`), hard hat lamps and gas jetpacks' item actions
//! ([`buttons`]; `crate::clothing::toggles` draws and validates them).
//!
//! - Goggles with the night vision action store their `color_cutoffs` when switched off (no
//!   `flash_protect`, the `..._off` sprite) and get them back when switched on.
//! - Magboots toggle in hand or by their button: `magpulse` attaches `active_traits` (no slipping
//!   on water and ice, `TRAIT_NEGATES_GRAVITY`, see `crate::spacemove`) and adds `slowdown_active`
//!   to the `slowdown`.
// shortcut: no footstep sounds (`shoe_footstep`), no speed potion's `slowdown_active = 0`, no
// fishing modifiers; NVG helmets' visors stay `toggles`' plain toggle.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmClothing, DmItem};
use ss13_shared::chat::Span;
use ss13_shared::protocol::InventorySlot;

use super::traits::{NEGATES_GRAVITY, NO_SLIP_ICE, NO_SLIP_SLIDE, NO_SLIP_WATER};
use crate::devices::flashlight::{self, Flashlight, HARDHAT};
use crate::inventory::slot_bit;
use crate::jetpack::{self, JETPACK};
use crate::sound::{PlaySound, playsound};
use crate::tools::tell;

const TOGGLE_NV_ACTION: &str = "/datum/action/item_action/toggle_nv";
const MESON: &str = "/obj/item/clothing/glasses/meson";
const MAGBOOTS: &str = "/obj/item/clothing/shoes/magboots";
const ADVANCED_MAGBOOTS: &str = "/obj/item/clothing/shoes/magboots/advance";
const HARDHAT_WELDING: &str = "/obj/item/clothing/head/utility/hardhat/welding";
/// `slowdown_active` of magboots (advanced: none).
const SLOWDOWN_ACTIVE: f32 = 2.0;
/// `FLASH_PROTECTION_SENSITIVE`.
const FLASH_PROTECTION_SENSITIVE: i32 = -1;
const NV_ON_SOUND: &str = "sound/items/night_vision_on.ogg";
const NV_OFF_SOUND: &str = "sound/machines/click.ogg";
/// `active_traits`.
const MAGBOOT_TRAITS: [&str; 4] = [NO_SLIP_WATER, NO_SLIP_ICE, NO_SLIP_SLIDE, NEGATES_GRAVITY];

pub const TOGGLE_NIGHT_VISION: &str = "Toggle Night Vision";
pub const TOGGLE_MAGBOOTS: &str = "Toggle Magboots";
pub const TOGGLE_HELMET_LIGHT: &str = "Toggle Helmet Light";
pub const TOGGLE_WELDING_SCREEN: &str = "Toggle Welding Screen";
pub const TOGGLE_JETPACK: &str = "Toggle Jetpack";
pub const JETPACK_STABILIZATION: &str = "Jetpack Stabilization";
pub const SET_INTERNALS: &str = "Set Internals";

/// Night vision switched off: the `color_cutoffs` it had (`stored_cutoffs`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct NightVisionOff([f32; 3]);

/// Magboots with `magpulse` on.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Magpulse;

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    app.add_click_handler(ClickPhase::AttackSelf, 1767, |w, c| {
        toggle_magboots(w, c.client, c.target)
    });
}

fn has_nv_toggle(world: &World, item: Entity) -> bool {
    world
        .get::<DmClothing>(item)
        .is_some_and(|c| c.actions.iter().any(|a| is_under(a, TOGGLE_NV_ACTION)))
}

/// The buttons `item` worn in `slot` gives (tg `actions_types` of this module's gear).
pub fn buttons(world: &World, slot: InventorySlot, item: Entity) -> Vec<&'static str> {
    let Some(path) = world.get::<DmItem>(item).map(|i| i.path.as_str()) else {
        return Vec::new();
    };
    match slot {
        InventorySlot::Eyes if has_nv_toggle(world, item) => vec![TOGGLE_NIGHT_VISION],
        InventorySlot::Shoes if is_under(path, MAGBOOTS) => vec![TOGGLE_MAGBOOTS],
        InventorySlot::Head
            if is_under(path, HARDHAT) && world.get::<Flashlight>(item).is_some() =>
        {
            let mut buttons = vec![TOGGLE_HELMET_LIGHT];
            if is_under(path, HARDHAT_WELDING) {
                buttons.push(TOGGLE_WELDING_SCREEN);
            }
            buttons
        }
        _ if is_under(path, JETPACK) && worn_in_its_slot(world, slot, item) => {
            vec![TOGGLE_JETPACK, JETPACK_STABILIZATION, SET_INTERNALS]
        }
        _ => Vec::new(),
    }
}

fn worn_in_its_slot(world: &World, slot: InventorySlot, item: Entity) -> bool {
    world
        .get::<DmItem>(item)
        .is_some_and(|dm| dm.slot_flags & slot_bit(slot) != 0)
}

/// One of [`buttons`] clicked, validated by the caller. True when it was this module's.
pub fn trigger(world: &mut World, client: Entity, mob: Entity, item: Entity, name: &str) -> bool {
    match name {
        TOGGLE_NIGHT_VISION => toggle_night_vision(world, item),
        TOGGLE_MAGBOOTS => {
            toggle_magboots(world, client, item);
        }
        TOGGLE_HELMET_LIGHT => {
            flashlight::toggle_light(world, Some(client), item);
        }
        TOGGLE_WELDING_SCREEN => {
            crate::vision::toggle_hardhat_visor(world, client, mob, item);
        }
        TOGGLE_JETPACK => jetpack::cycle(world, client, mob, item),
        JETPACK_STABILIZATION => jetpack::toggle_stabilization(world, client, mob, item),
        SET_INTERNALS => crate::internals::toggle(world, client, mob, item),
        _ => return false,
    }
    true
}

/// tg `/datum/action/item_action/toggle_nv/do_effect`.
pub fn toggle_night_vision(world: &mut World, item: Entity) {
    let Some(atom_state) = world.get::<DmAtom>(item).map(|a| a.icon_state.clone()) else {
        return;
    };
    let meson = world
        .get::<DmItem>(item)
        .is_some_and(|i| is_under(&i.path, MESON));
    let (state, sound) = match world.get::<NightVisionOff>(item).copied() {
        Some(NightVisionOff(cutoffs)) => {
            world.entity_mut(item).remove::<NightVisionOff>();
            if let Some(mut c) = world.get_mut::<DmClothing>(item) {
                c.color_cutoffs = Some(cutoffs);
                c.flash_protect = FLASH_PROTECTION_SENSITIVE;
            }
            (atom_state, NV_ON_SOUND)
        }
        None => {
            let Some(cutoffs) = world.get::<DmClothing>(item).and_then(|c| c.color_cutoffs) else {
                return;
            };
            world.entity_mut(item).insert(NightVisionOff(cutoffs));
            if let Some(mut c) = world.get_mut::<DmClothing>(item) {
                c.color_cutoffs = None;
                c.flash_protect = 0;
            }
            let off = if meson { "nvgmeson_off" } else { "night_off" };
            (off.to_owned(), NV_OFF_SOUND)
        }
    };
    if let Some(pos) = crate::modpc::pda::turf_of(world, item) {
        playsound(world, PlaySound::new(pos, sound, 30.0, true));
    }
    super::toggles::set_states(world, item, &state, state.clone());
}

/// `/obj/item/clothing/shoes/magboots/attack_self`; false for other items.
pub fn toggle_magboots(world: &mut World, client: Entity, item: Entity) -> bool {
    let Some(dm) = world.get::<DmItem>(item) else {
        return false;
    };
    if !is_under(&dm.path, MAGBOOTS) {
        return false;
    }
    let active_slowdown = if is_under(&dm.path, ADVANCED_MAGBOOTS) {
        0.0
    } else {
        SLOWDOWN_ACTIVE
    };
    let turn_on = world.get::<Magpulse>(item).is_none();
    let Some(mut clothing) = world.get_mut::<DmClothing>(item) else {
        return false;
    };
    // `attach_clothing_traits` / `detach_clothing_traits`.
    if turn_on {
        clothing
            .clothing_traits
            .extend(MAGBOOT_TRAITS.iter().map(|t| (*t).to_owned()));
    } else {
        clothing
            .clothing_traits
            .retain(|t| !MAGBOOT_TRAITS.contains(&t.as_str()));
    }
    if let Some(mut dm) = world.get_mut::<DmItem>(item) {
        dm.slowdown += if turn_on {
            active_slowdown
        } else {
            -active_slowdown
        };
    }
    let mut entity = world.entity_mut(item);
    if turn_on {
        entity.insert(Magpulse);
    } else {
        entity.remove::<Magpulse>();
    }
    let base = world
        .get::<DmAtom>(item)
        .map(|a| a.icon_state.trim_end_matches(['0', '1']).to_owned())
        .unwrap_or_default();
    let state = format!("{base}{}", u8::from(turn_on));
    super::toggles::set_states(world, item, &state, state.clone());
    tell(
        world,
        client,
        Span::Notice,
        if turn_on {
            "Магнитные подошвы включены."
        } else {
            "Магнитные подошвы выключены."
        },
    );
    true
}

/// `/obj/item/clothing/shoes/magboots/examine`.
pub fn examine_lines(world: &World, item: Entity, lines: &mut Vec<crate::examine::Line>) {
    if world
        .get::<DmItem>(item)
        .is_some_and(|i| is_under(&i.path, MAGBOOTS))
    {
        let state = if world.get::<Magpulse>(item).is_some() {
            "включены"
        } else {
            "выключены"
        };
        lines.push((Span::Plain, format!("Магнитные подошвы {state}.")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::Inventory;

    fn client(world: &mut World) -> Entity {
        world.init_resource::<Messages<ss13_net::chat::server::SendChat>>();
        world.spawn_empty().id()
    }

    #[test]
    fn night_vision_stores_its_cutoffs_when_off() {
        let mut world = World::new();
        let goggles = world
            .spawn((
                DmItem::default(),
                DmAtom {
                    icon_state: "night".into(),
                    ..default()
                },
                DmClothing {
                    color_cutoffs: Some([10.0, 25.0, 10.0]),
                    flash_protect: FLASH_PROTECTION_SENSITIVE,
                    ..default()
                },
            ))
            .id();
        toggle_night_vision(&mut world, goggles);
        let c = world.get::<DmClothing>(goggles).unwrap();
        assert_eq!((c.color_cutoffs, c.flash_protect), (None, 0));
        toggle_night_vision(&mut world, goggles);
        let c = world.get::<DmClothing>(goggles).unwrap();
        assert_eq!(c.color_cutoffs, Some([10.0, 25.0, 10.0]));
        assert_eq!(c.flash_protect, FLASH_PROTECTION_SENSITIVE);
    }

    #[test]
    fn magboots_gain_traits_and_slowdown() {
        let mut world = World::new();
        let client = client(&mut world);
        let boots = world
            .spawn((
                DmItem {
                    path: MAGBOOTS.to_owned(),
                    slowdown: 0.5,
                    slot_flags: slot_bit(InventorySlot::Shoes),
                    ..default()
                },
                DmAtom {
                    icon_state: "magboots0".into(),
                    ..default()
                },
                DmClothing::default(),
            ))
            .id();
        let mob = world
            .spawn(Inventory {
                shoes: Some(boots),
                ..default()
            })
            .id();
        use crate::clothing::traits::has_trait;
        assert!(!has_trait(&world, mob, NEGATES_GRAVITY));
        assert!(toggle_magboots(&mut world, client, boots));
        assert!(has_trait(&world, mob, NEGATES_GRAVITY));
        assert_eq!(world.get::<DmItem>(boots).unwrap().slowdown, 2.5);
        assert!(toggle_magboots(&mut world, client, boots));
        assert!(!has_trait(&world, mob, NEGATES_GRAVITY));
        assert_eq!(world.get::<DmItem>(boots).unwrap().slowdown, 0.5);
    }

    #[test]
    fn gear_buttons_follow_the_slot() {
        let mut world = World::new();
        let boots = world
            .spawn(DmItem {
                path: MAGBOOTS.to_owned(),
                ..default()
            })
            .id();
        assert_eq!(
            buttons(&world, InventorySlot::Shoes, boots),
            [TOGGLE_MAGBOOTS]
        );
        assert!(buttons(&world, InventorySlot::Back, boots).is_empty());
    }
}
