//! Clothing mechanics after tg `code/modules/clothing/`: what worn clothing hides ([`cover`]),
//! clothing that changes while worn ([`toggles`]: masks, jumpsuit styles, toggle suits, hoods),
//! clothing damage ([`damage`]), suit sensors and the crew monitoring console ([`sensors`]),
//! accessories on jumpsuits ([`accessories`]) and the ones with storage, holsters ([`holsters`]).
//!
//! This module holds what both halves share (the crew console window's data, tgui
//! `CrewConsole`), the server plugin and the entry points other modules call: the click chain
//! ([`attack_self`], [`item_interaction`], [`click_alt`], [`click_alt_secondary`]), examine
//! ([`examine_lines`]) and the worn look ([`WornLook`]).

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

#[cfg(feature = "server")]
pub mod ablative;
#[cfg(feature = "server")]
pub mod accessories;
#[cfg(feature = "server")]
pub mod chameleon;
#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod cover;
#[cfg(feature = "server")]
pub mod damage;
#[cfg(feature = "server")]
pub mod envirosuit;
#[cfg(feature = "server")]
pub mod holsters;
#[cfg(feature = "server")]
pub mod lasertag;
#[cfg(feature = "server")]
pub mod reactive;
#[cfg(feature = "server")]
pub mod reskin;
#[cfg(feature = "server")]
pub mod sensors;
#[cfg(feature = "server")]
pub mod slowdown;
#[cfg(feature = "server")]
pub mod toggles;
#[cfg(feature = "server")]
pub mod traits;

#[cfg(feature = "server")]
pub use server::*;

/// tgui `CrewConsole` (`tgui/packages/tgui/interfaces/CrewConsole.tsx`).
pub struct CrewConsoleUi;

impl UiInterface for CrewConsoleUi {
    const NAME: &'static str = "CrewConsole";
    type Data = CrewConsoleData;
    type StaticData = ();
}

/// `/datum/crewmonitor/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CrewConsoleData {
    pub sensors: Vec<CrewSensor>,
    /// `HAS_AI_ACCESS(user)`: the tracking column (not ported: always false).
    pub link_allowed: bool,
}

/// One entry of `update_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CrewSensor {
    /// `ref`: the mob, for the list's keys.
    pub key: u64,
    /// The ID's `registered_name`, "Unknown" without one.
    pub name: String,
    pub assignment: Option<String>,
    /// The job's sort id (`jobs`); heads of staff are multiples of 10.
    pub ijob: u32,
    /// tg `stat` (`0` conscious .. `4` dead); "alive/dead" sensors give only 0 or 4.
    pub life_status: u8,
    /// Damage from "vitals" sensors and up.
    pub vitals: Option<CrewVitals>,
    /// The area from "tracking" sensors.
    pub area: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrewVitals {
    pub oxy: i32,
    pub tox: i32,
    pub burn: i32,
    pub brute: i32,
    pub health: i32,
}

#[cfg(feature = "server")]
mod server {
    use bevy::ecs::system::SystemParam;
    use bevy::prelude::*;
    use ss13_mapload::{DmAtom, DmClothing, DmItem, IconIndex};
    use ss13_shared::IconStateId;

    use super::accessories::{self, Accessories};
    use super::{
        chameleon, cover, damage, envirosuit, reactive, reskin, sensors, slowdown, toggles,
    };
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    use crate::human::layers::{FACEMASK_LAYER, UNIFORM_LAYER};
    use crate::inventory::Inventory;

    pub use super::damage::ClothingJob;

    /// `icons/effects/item_damage.dmi`: the worn damage overlays.
    const ITEM_DAMAGE_DMI: &str = "icons/effects/item_damage.dmi";

    pub struct ClothingPlugin;

    impl Plugin for ClothingPlugin {
        fn build(&self, app: &mut App) {
            // Click chain hooks (`crate::click_handlers`).
            app.add_click_handler(ClickPhase::AttackSelf, 3155, |w, c| {
                toggles::attack_self(w, c.client, c.mob, c.target)
            })
            // Before the core storage step: a jumpsuit with a holster takes accessories too.
            .add_click_handler(ClickPhase::OtherHand, 480, |w, c| {
                c.held
                    .is_some_and(|held| accessories::attach(w, c.client, c.mob, held, c.target))
            })
            .add_click_handler(ClickPhase::OtherHand, 1255, |w, c| {
                c.held
                    .is_some_and(|held| item_interaction(w, c.client, c.mob, held, c.target))
            })
            .add_click_handler(ClickPhase::HeldOnTarget, 3455, |w, c| {
                c.held
                    .is_some_and(|held| item_interaction(w, c.client, c.mob, held, c.target))
            })
            .add_click_handler(ClickPhase::EmptyHand, 3455, |w, c| {
                sensors::interact(w, c.client, c.mob, c.target)
            })
            .add_click_handler(ClickPhase::AttackSelf, 3159, |w, c| {
                reactive::attack_self(w, c.client, c.mob, c.target)
            })
            .add_click_handler(ClickPhase::OtherHand, 1259, |w, c| {
                c.held.is_some_and(|held| {
                    reactive::shell_interaction(w, c.client, c.mob, held, c.target)
                })
            })
            // Before attacks: the chameleon scanner scans a human in range.
            .add_click_handler(ClickPhase::HeldOnTarget, 219, chameleon::scanner_click)
            .add_click_handler(ClickPhase::HeldOnTarget, 1579, |w, c| {
                c.held.is_some_and(|held| {
                    reactive::shell_interaction(w, c.client, c.mob, held, c.target)
                })
            });
            reactive::build(app);
            envirosuit::build(app);
            sensors::build(app);
            slowdown::build(app);
            chameleon::build(app);
            toggles::build(app);
            reskin::build(app);
        }
    }

    /// tg `item_interaction` of clothing with `held`: an accessory pinned to a jumpsuit, a cable
    /// coil on broken sensors, cloth mending damage.
    pub fn item_interaction(
        world: &mut World,
        client: Entity,
        mob: Entity,
        held: Entity,
        target: Entity,
    ) -> bool {
        accessories::attach(world, client, mob, held, target)
            || sensors::item_interaction(world, client, mob, held, target)
            || damage::item_interaction(world, client, mob, held, target)
            || envirosuit::item_interaction(world, client, mob, held, target)
    }

    /// tg `click_alt` of clothing: a jumpsuit's style, a toggle suit, a breath mask.
    pub fn click_alt(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
        reskin::click_alt(world, client, mob, item)
            || toggles::rolldown(world, client, mob, item)
            || toggles::toggle_suit(world, client, mob, item)
            || toggles::breath_alt(world, client, mob, item)
    }

    /// tg `click_alt_secondary` of a jumpsuit: its first accessory comes off.
    pub fn click_alt_secondary(
        world: &mut World,
        client: Entity,
        mob: Entity,
        item: Entity,
    ) -> bool {
        accessories::pop(world, client, mob, item)
    }

    /// tg `attack_self` of clothing.
    pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
        toggles::attack_self(world, client, mob, item)
    }

    /// tg `attack_self_secondary` / `attack_hand_secondary` of a jumpsuit: the sensor mode.
    pub fn right_click_slot(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
        sensors::toggle_prompt(world, client, mob, item)
    }

    /// The examine lines of clothing (`/obj/item/clothing/examine` and its subtypes').
    pub fn examine_lines(world: &World, item: Entity, lines: &mut Vec<crate::examine::Line>) {
        if world.get::<DmClothing>(item).is_none() {
            return;
        }
        damage::examine_lines(world, item, lines);
        toggles::examine_lines(world, item, lines);
        if accessories::is_jumpsuit(world, item) {
            sensors::examine_lines(world, item, lines);
        }
        accessories::examine_lines(world, item, lines);
        envirosuit::examine_lines(world, item, lines);
    }

    /// What the worn look needs from the clothing (`crate::inventory`'s `refresh_mobs`).
    #[derive(SystemParam)]
    pub struct WornLook<'w, 's> {
        clothing: Query<'w, 's, &'static DmClothing>,
        accessories: Query<'w, 's, &'static Accessories>,
        items: Query<'w, 's, (&'static DmItem, Option<&'static DmAtom>)>,
        wear: Query<'w, 's, &'static damage::ClothingWear>,
        mod_overlays: Query<'w, 's, &'static crate::modsuit::modules::ModOverlays>,
    }

    impl WornLook<'_, '_> {
        /// The mob's `obscured_slots` from what it wears.
        pub fn obscured(&self, inventory: &Inventory) -> u32 {
            cover::obscured_by(inventory, |e| {
                self.clothing.get(e).map_or(0, |c| c.flags_inv)
            })
        }

        /// Overlays on top of the worn items of `shown` (the inventory as drawn): the
        /// jumpsuit's accessories and damage, rings on the gloves, a damaged mask.
        pub fn overlays(&self, shown: &Inventory, icons: &IconIndex) -> Vec<(f32, IconStateId)> {
            let mut out = Vec::new();
            let damaged = |e: Entity| {
                self.wear
                    .get(e)
                    .is_ok_and(|w| w.state != damage::Wear::Pristine)
            };
            if let Some(uniform) = shown.uniform {
                if damaged(uniform)
                    && let Some(id) = icons.get(ITEM_DAMAGE_DMI, "damageduniform")
                {
                    out.push((UNIFORM_LAYER - 0.2, id));
                }
                let suit_flags = shown
                    .suit
                    .and_then(|s| self.clothing.get(s).ok())
                    .map_or(0, |c| c.flags_inv);
                let pinned = self
                    .accessories
                    .get(uniform)
                    .map(|a| a.items().to_vec())
                    .unwrap_or_default();
                let states = accessories::worn_states(&pinned, suit_flags, |a| {
                    let (dm, atom) = self.items.get(a).ok()?;
                    Some((dm, atom, self.clothing.get(a).ok()))
                });
                for (dmi, state) in states {
                    if let Some(id) = icons.get(&dmi, &state) {
                        out.push((UNIFORM_LAYER - 0.5, id));
                    }
                }
            }
            // BandaStation `gloves/worn_overlays`: the accessory overlay.
            if let Some(gloves) = shown.gloves {
                let pinned = self
                    .accessories
                    .get(gloves)
                    .map(|a| a.items().to_vec())
                    .unwrap_or_default();
                let states = accessories::worn_states(&pinned, 0, |a| {
                    let (dm, atom) = self.items.get(a).ok()?;
                    Some((dm, atom, self.clothing.get(a).ok()))
                });
                for (dmi, state) in states {
                    if let Some(id) = icons.get(&dmi, &state) {
                        out.push((GLOVES_LAYER - 0.5, id));
                    }
                }
            }
            if let Some(mask) = shown.mask
                && damaged(mask)
                && let Some(id) = icons.get(ITEM_DAMAGE_DMI, "damagedmask")
            {
                out.push((FACEMASK_LAYER - 0.2, id));
            }
            // MOD module overlays over their part or unit (`standing.layer + 0.1`).
            use crate::human::layers::{
                BACK_LAYER, BELT_LAYER, GLOVES_LAYER, HEAD_LAYER, SHOES_LAYER, SUIT_LAYER,
            };
            for (item, layer) in [
                (shown.back, BACK_LAYER),
                (shown.belt, BELT_LAYER),
                (shown.suit, SUIT_LAYER),
                (shown.head, HEAD_LAYER),
                (shown.gloves, GLOVES_LAYER),
                (shown.shoes, SHOES_LAYER),
            ] {
                if let Some(o) = item.and_then(|e| self.mod_overlays.get(e).ok()) {
                    out.extend(o.0.iter().map(|&id| (layer - 0.1, id)));
                }
            }
            out
        }
    }
}
