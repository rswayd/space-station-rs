//! The inventory HUD's data, shared by both halves. It travels as a tgui interface without a
//! window ([`InventoryHudUi`]): the server keeps one per player open on its own mob, the client
//! draws it as screen slots (`crate::inventory_client`), and slot clicks go back as
//! `ClientIntent::ClickSlot`, not as tgui acts.

use serde::{Deserialize, Serialize};
use ss13_shared::IconStateId;
use ss13_shared::protocol::InventorySlot;
use ss13_tgui::UiInterface;

use crate::zone::{BodyZone, TargetZone};

/// What a slot shows: the item's own icon (overlays are not drawn on the HUD).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HudIcon {
    pub icon_state: IconStateId,
    pub color: Option<[u8; 4]>,
}

/// The HUD of one mob. `None`: the slot is empty, or its item has no icon.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(bevy::prelude::Component))]
pub struct HudData {
    /// Left, right (tg held indexes 1, 2).
    pub hands: [Option<HudIcon>; 2],
    /// Index into `hands`.
    pub active: u8,
    pub id: Option<HudIcon>,
    pub uniform: Option<HudIcon>,
    pub shoes: Option<HudIcon>,
    pub back: Option<HudIcon>,
    pub head: Option<HudIcon>,
    pub mask: Option<HudIcon>,
    pub neck: Option<HudIcon>,
    pub eyes: Option<HudIcon>,
    pub ears: Option<HudIcon>,
    pub gloves: Option<HudIcon>,
    pub suit: Option<HudIcon>,
    pub suit_storage: Option<HudIcon>,
    pub belt: Option<HudIcon>,
    /// Left, right.
    pub pockets: [Option<HudIcon>; 2],
}

impl HudData {
    /// What `slot` shows.
    pub fn slot(&self, slot: InventorySlot) -> Option<HudIcon> {
        match slot {
            InventorySlot::LeftHand => self.hands[0],
            InventorySlot::RightHand => self.hands[1],
            InventorySlot::Id => self.id,
            InventorySlot::Uniform => self.uniform,
            InventorySlot::Shoes => self.shoes,
            InventorySlot::Back => self.back,
            InventorySlot::Head => self.head,
            InventorySlot::Mask => self.mask,
            InventorySlot::Neck => self.neck,
            InventorySlot::Eyes => self.eyes,
            InventorySlot::Ears => self.ears,
            InventorySlot::Gloves => self.gloves,
            InventorySlot::Suit => self.suit,
            InventorySlot::SuitStorage => self.suit_storage,
            InventorySlot::Belt => self.belt,
            InventorySlot::LeftPocket => self.pockets[0],
            InventorySlot::RightPocket => self.pockets[1],
        }
    }
}

pub struct InventoryHudUi;

impl UiInterface for InventoryHudUi {
    const NAME: &'static str = "InventoryHud";
    type Data = HudData;
    type StaticData = ();
}

/// The storage the player has open (tg `/datum/storage/show_contents`): its contents in order,
/// drawn as a grid of screen slots (`crate::storage_client`). Acts: `take` (`key`: that item goes
/// to the hands), `insert` (the active hand's item goes in).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(bevy::prelude::Component))]
pub struct StorageData {
    pub title: String,
    pub max_slots: u8,
    pub items: Vec<StoredItem>,
}

/// One item of an open storage. `take` names it by `key` (tg passes the item's ref), so a click
/// still means that item after an earlier act in the same tick shifted the contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredItem {
    pub key: u64,
    /// `None`: an item without an icon.
    pub icon: Option<HudIcon>,
}

/// A tgui interface without a window, kept open on the player's mob while a storage is shown.
pub struct StorageUi;

impl UiInterface for StorageUi {
    const NAME: &'static str = "Storage";
    type Data = StorageData;
    type StaticData = ();
}

/// tg `/datum/storage::screen_max_columns`.
pub const STORAGE_COLUMNS: usize = 7;

/// tg `orient_storage`: columns and rows of the storage grid for `items` contents.
pub fn storage_grid(max_slots: usize, items: usize) -> (usize, usize) {
    let columns = max_slots.clamp(1, STORAGE_COLUMNS);
    let additional_row = usize::from(items.is_multiple_of(STORAGE_COLUMNS) && items < max_slots);
    let rows = (items.div_ceil(columns) + additional_row).max(1);
    (columns, rows)
}

/// The owner's health HUD (tg `/atom/movable/screen/healths`, the health doll and the combat mode
/// toggle), sent like [`InventoryHudUi`]: a headless tgui interface the server keeps open on the
/// client's controlled entity. Only the owner gets these numbers; others see the body lying.
pub struct HealthHudUi;

impl UiInterface for HealthHudUi {
    const NAME: &'static str = "HealthHud";
    type Data = HealthHudData;
    type StaticData = ();
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(bevy::prelude::Component))]
pub struct HealthHudData {
    /// `None`: not a living mob (a ghost).
    pub health: Option<HealthReadout>,
    pub combat_mode: bool,
    /// Seconds per tile of the controlled entity, for client-side prediction.
    pub move_delay: f32,
    /// The controlled mob cannot move at all (crit, stuns, stamcrit): no steps are predicted.
    pub immobile: bool,
    /// tg `zone_selected`, drawn on the zone selector.
    pub zone_selected: TargetZone,
}

/// The health doll's limbs in draw order: tg layers the chest above the others.
pub const DOLL_LIMBS: [BodyZone; 6] = [
    BodyZone::Head,
    BodyZone::LArm,
    BodyZone::RArm,
    BodyZone::LLeg,
    BodyZone::RLeg,
    BodyZone::Chest,
];

/// The owner's pull icon, throw and move intent buttons (tg `/atom/movable/screen/pull`,
/// `throw_catch`, `mov_intent`), a
/// headless tgui interface like [`HealthHudUi`] kept open on the client's living mob.
pub struct InteractHudUi;

impl UiInterface for InteractHudUi {
    const NAME: &'static str = "InteractHud";
    type Data = InteractHudData;
    type StaticData = ();
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(bevy::prelude::Component))]
pub struct InteractHudData {
    /// tg `throw_mode`: `act_throw_on` instead of `act_throw`.
    pub throw_mode: bool,
    /// tg `pulling`: `pull` instead of `pull0`.
    pub pulling: bool,
    /// tg `move_intent == MOVE_INTENT_WALK`: `walking` instead of `running`.
    pub walking: bool,
    /// tg `resting`: `act_rest_on` instead of `act_rest`.
    pub resting: bool,
}

/// The owner's screen alerts (tg `throw_alert`, `icons/hud/screen_alert.dmi`), a headless tgui
/// interface like [`HealthHudUi`], in the order tg's `alerts` list keeps them.
pub struct AlertHudUi;

impl UiInterface for AlertHudUi {
    const NAME: &'static str = "AlertHud";
    type Data = AlertHudData;
    type StaticData = ();
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(bevy::prelude::Component))]
pub struct AlertHudData {
    pub alerts: Vec<HudAlert>,
}

/// One alert: its `icon_state` (`None`: missing from the assets), `name` and `desc`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HudAlert {
    pub icon: Option<IconStateId>,
    pub name: String,
    pub desc: String,
}

/// The owner's hunger bar (tg `/atom/movable/screen/hunger`, `ui_hunger`), a headless tgui
/// interface like [`HealthHudUi`].
pub struct HungerHudUi;

impl UiInterface for HungerHudUi {
    const NAME: &'static str = "HungerHud";
    type Data = HungerHudData;
    type StaticData = ();
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(bevy::prelude::Component))]
pub struct HungerHudData {
    /// `HUNGER_STATE_*`: 0 starving (red outline), 2 hungry, 3 fine, 4 full, 5 fat (dark food).
    pub state: u8,
    /// The bar's fill, `fullness / NUTRITION_LEVEL_FULL` in steps of 0.05 (above 1: overfed).
    pub bar: f32,
    /// tg fades the bar out while "fine" with nothing left to digest.
    pub shown: bool,
}

/// tg `get_ui_alert_placement`: only the first 5 alerts are shown.
pub const MAX_SHOWN_ALERTS: usize = 5;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HealthReadout {
    pub health: f32,
    pub max: f32,
    pub brute: f32,
    pub burn: f32,
    pub tox: f32,
    pub oxy: f32,
    pub dead: bool,
    /// Limb states `0..=5` in [`DOLL_LIMBS`] order (tg `ceil(damage / max_damage * 5)`).
    pub limbs: [u8; 6],
}

impl HealthReadout {
    /// The `health0`..`health7` state of tg `/mob/living/carbon/update_health_hud`.
    pub fn health_state(&self) -> u8 {
        if self.dead {
            7
        } else if self.health >= self.max {
            0
        } else if self.health > 0.0 && self.max > 0.0 {
            (6.0 - (self.health / (self.max * 0.2)).ceil()).clamp(0.0, 6.0) as u8
        } else {
            6
        }
    }

    /// The state (`[zone]0`..`[zone]5`) of the doll limb `i` of [`DOLL_LIMBS`] (tg
    /// `/atom/movable/screen/healthdoll/human`); `None` = `[zone]DEAD`.
    pub fn doll_state(&self, i: usize) -> Option<u8> {
        (!self.dead).then(|| self.limbs.get(i).copied().unwrap_or(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_follows_orient_storage() {
        // A backpack (21 slots): one row until it is full, then a free row below.
        assert_eq!(storage_grid(21, 0), (7, 1));
        assert_eq!(storage_grid(21, 3), (7, 1));
        assert_eq!(storage_grid(21, 7), (7, 2));
        assert_eq!(storage_grid(21, 21), (7, 3));
        // A box (7 slots) never grows a row past full.
        assert_eq!(storage_grid(7, 7), (7, 1));
        assert_eq!(storage_grid(5, 2), (5, 1));
    }

    fn at(health: f32) -> HealthReadout {
        HealthReadout {
            health,
            max: 100.0,
            brute: 100.0 - health,
            ..Default::default()
        }
    }

    #[test]
    fn health_states_match_tg_update_health_hud() {
        let state = |h| at(h).health_state();
        assert_eq!(state(100.0), 0);
        // 6 - ceil(h / 20).
        assert_eq!(state(99.0), 1);
        assert_eq!(state(80.0), 2);
        assert_eq!(state(79.9), 2);
        assert_eq!(state(60.0), 3);
        assert_eq!(state(20.0), 5);
        assert_eq!(state(0.1), 5);
        assert_eq!(state(0.0), 6);
        assert_eq!(state(-50.0), 6);
        let dead = HealthReadout {
            dead: true,
            ..at(-100.0)
        };
        assert_eq!(dead.health_state(), 7);
        assert_eq!(dead.doll_state(0), None);
        let hurt = HealthReadout {
            limbs: [0, 1, 0, 0, 0, 5],
            ..at(50.0)
        };
        assert_eq!(hurt.doll_state(1), Some(1));
        assert_eq!(hurt.doll_state(5), Some(5));
    }
}
