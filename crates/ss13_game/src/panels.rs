//! Data of the strip menu and loot panel windows, shared by the server halves (`crate::strip`,
//! `crate::loot`) and the client views (`crate::panels_client`).

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

use crate::hud::HudIcon;

/// tgui `StripMenu` (`/datum/strip_menu`).
pub struct StripMenuUi;

impl UiInterface for StripMenuUi {
    const NAME: &'static str = "StripMenu";
    type Data = StripMenuData;
    type StaticData = ();
}

/// `STRIPPABLE_OBSCURING_*` (`code/__DEFINES/strippable.dm`).
pub const OBSCURING_NONE: u8 = 0;
pub const OBSCURING_COMPLETELY: u8 = 1;
pub const OBSCURING_HIDDEN: u8 = 2;
pub const OBSCURING_INACCESSIBLE: u8 = 3;

/// `/datum/strip_menu/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StripMenuData {
    /// `"\the [owner]"`.
    pub name: String,
    /// The slots shown, in tg's `strippable_human_items` order.
    pub items: Vec<StripSlot>,
}

/// One entry of `items`: tg's `null` is an empty slot nobody works on (`item` `None`,
/// `obscured` none, not `interacting`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StripSlot {
    /// `STRIPPABLE_ITEM_*` key (`"head"`, `"left_pocket"`, `"handcuffs"`, ...).
    pub key: String,
    /// The item's name and icon; `None` when empty or hidden.
    pub item: Option<StripItem>,
    pub obscured: u8,
    /// The viewer is putting on or taking off this slot right now.
    pub interacting: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StripItem {
    pub name: String,
    pub icon: Option<HudIcon>,
}

/// `act('use', { key })`.
pub const STRIP_USE: &str = "use";

/// tgui `LootPanel` (`/datum/lootpanel`): the contents of a turf, tg's alt-click.
pub struct LootPanelUi;

impl UiInterface for LootPanelUi {
    const NAME: &'static str = "LootPanel";
    type Data = LootPanelData;
    type StaticData = ();
}

/// `/datum/lootpanel/ui_data`: the turf first, then what is on it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LootPanelData {
    pub contents: Vec<LootEntry>,
}

/// A `/datum/search_object`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LootEntry {
    pub name: String,
    /// Type path of an obj (groups equal items); empty for turfs and mobs.
    pub path: String,
    pub icon: Option<HudIcon>,
    /// The thing, as the `grab` act names it back; the server checks it is still on the turf.
    pub key: u64,
}

/// `act('grab', { ref, ctrl, shift, alt, right })`.
pub const LOOT_GRAB: &str = "grab";
