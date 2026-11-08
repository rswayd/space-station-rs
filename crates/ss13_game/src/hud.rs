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
    /// It walks through anything (a ghost, the AI's eye): prediction ignores what blocks.
    pub phasing: bool,
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
/// interface like [`HealthHudUi`], in the order tg's `alerts` list keeps them. Its one act,
/// [`ALERT_CLICK`], is a click on the alert at `index` (0-based) named `name` (tg
/// `/atom/movable/screen/alert/Click`): the server ignores it when that alert is not there.
pub struct AlertHudUi;

/// [`AlertHudUi`]'s act.
pub const ALERT_CLICK: &str = "click";

/// The part of an alert's name an [`ALERT_CLICK`] carries: at most 128 bytes, cut at a
/// character (a tgui param is capped at 256).
pub fn alert_click_name(name: &str) -> &str {
    let mut end = name.len().min(128);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    &name[..end]
}

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

/// The owner's ammo counter (BandaStation `/atom/movable/screen/gunhud_screen` at `ui_gunhud`),
/// a headless tgui interface like [`HealthHudUi`] (`crate::gunhud`, `crate::gunhud_client`).
pub struct GunHudUi;

impl UiInterface for GunHudUi {
    const NAME: &'static str = "GunHud";
    type Data = GunHudData;
    type StaticData = ();
}

/// What the counter shows; all states are in [`GUNHUD_DMI`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(bevy::prelude::Component))]
pub struct GunHudData {
    /// `invisibility = 0` (a gun or welder in hand).
    pub shown: bool,
    /// The screen object's `icon_state` (`backing`, `eammo_counter`, `eammo_counter_empty`).
    pub base: String,
    /// `update_overlays` in order: each overlay's state and `backing_color` (RGB).
    pub layers: Vec<(String, [u8; 3])>,
    /// An energy gun's maptext: the cell percent and its colour, the shot's cost percent
    /// (cyan).
    pub energy: Option<(u32, [u8; 3], u32)>,
}

/// BandaStation `modular_bandastation/gunhud/icons/gun_hud.dmi`.
pub const GUNHUD_DMI: &str = "modular_bandastation/gunhud/icons/gun_hud.dmi";

/// The owner's mood face (tg `/atom/movable/screen/mood`, `ui_mood`), a headless tgui interface
/// like [`HealthHudUi`]. Its one act, [`MOOD_CLICK`], prints the mood to the owner's chat
/// (`/datum/mood/proc/hud_click`).
pub struct MoodHudUi;

impl UiInterface for MoodHudUi {
    const NAME: &'static str = "MoodHud";
    type Data = MoodHudData;
    type StaticData = ();
}

/// [`MoodHudUi`]'s act: a click on the face.
pub const MOOD_CLICK: &str = "click";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(bevy::prelude::Component))]
pub struct MoodHudData {
    /// `mood_level` 1..=9: the `icons/hud/screen_gen.dmi` state `mood[level]`; 0 hides the face.
    pub level: u8,
    /// The face's `color`, by sanity level (`update_mood_icon`).
    pub color: [u8; 3],
}

/// The owner's fullscreen overlays and screen filters (tg `overlay_fullscreen`,
/// `add_client_colour`, the `eye_blur` filter), a headless tgui interface like [`HealthHudUi`]:
/// only the owner learns what its eyes see.
pub struct FullscreenHudUi;

impl UiInterface for FullscreenHudUi {
    const NAME: &'static str = "FullscreenHud";
    type Data = FullscreenHudData;
    type StaticData = ();
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(bevy::prelude::Component))]
pub struct FullscreenHudData {
    /// One per category, in draw order (bottom first).
    pub screens: Vec<FullscreenScreen>,
    /// tg `gauss_blur_filter(size)` on the game plane, in icon pixels; 0: none.
    pub blur: f32,
    /// tg `client_colours` on the game plane, lowest priority first.
    pub colours: Vec<ClientColour>,
    /// tg `lighting_cutoff` + `lighting_color_cutoffs` of the mob, 0..=1 per channel
    /// (`ss13_render::VisionLighting`).
    pub cutoff: [f32; 3],
    /// The mob's `sight` flags show something behind walls (`Sight::SEE_TURFS`, `SEE_MOBS`).
    pub see_behind: bool,
}

/// tg's `screens` categories the port draws, in draw order: by tg layer (`FULLSCREEN_LAYER` 2
/// for flash and the impaired overlays, `UI_DAMAGE_LAYER` 3, `BLIND_LAYER` 4, `CRIT_LAYER` 5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ScreenCategory {
    /// `"tint"`: `/atom/movable/screen/fullscreen/impaired` from worn tint.
    Tint,
    /// `"nearsighted"`: `impaired` by severity.
    Nearsighted,
    /// `"flash"`: `/atom/movable/screen/fullscreen/flash` over the whole view.
    Flash,
    /// `"brute"`: `/atom/movable/screen/fullscreen/brute`.
    Brute,
    /// `"oxy"`: `/atom/movable/screen/fullscreen/oxy`.
    Oxy,
    /// `"blindness"`: `/atom/movable/screen/fullscreen/blind` (flickering) or `blind/noflicker`.
    Blind,
    /// `"critvision"`: `/atom/movable/screen/fullscreen/crit/vision`.
    CritVision,
    /// `"crit"`: `/atom/movable/screen/fullscreen/crit`.
    Crit,
}

/// One shown overlay.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FullscreenScreen {
    pub category: ScreenCategory,
    /// tg `severity`, appended to the icon state; `None` appends nothing.
    pub severity: Option<u8>,
    /// [`ScreenCategory::Blind`]: the flickering overlay (temporary blindness alone).
    pub flicker: bool,
    /// Seconds it fades out over once cleared (tg `clear_fullscreen(category, animated)`).
    pub fade: f32,
}

pub const SCREEN_FULL_DMI: &str = "icons/hud/screen_full.dmi";
pub const SCREEN_GEN_DMI: &str = "icons/hud/screen_gen.dmi";

impl FullscreenScreen {
    /// tg `clear_fullscreen`'s default animation, seconds.
    pub const FADE: f32 = 1.0;

    pub fn new(category: ScreenCategory, severity: Option<u8>) -> Self {
        Self {
            category,
            severity,
            flicker: false,
            fade: Self::FADE,
        }
    }

    /// The `.dmi` and icon state tg's `overlay_fullscreen` gives the screen
    /// (`"[initial(icon_state)][severity]"`).
    pub fn icon(&self) -> (&'static str, String) {
        let base = match self.category {
            ScreenCategory::Flash => return (SCREEN_GEN_DMI, "flash".to_owned()),
            ScreenCategory::Tint | ScreenCategory::Nearsighted => "impairedoverlay",
            ScreenCategory::Brute => "brutedamageoverlay",
            ScreenCategory::Oxy | ScreenCategory::CritVision => "oxydamageoverlay",
            ScreenCategory::Crit => "passage",
            ScreenCategory::Blind if self.flicker => "blackimageoverlay",
            ScreenCategory::Blind => "blackimageoverlaystatic",
        };
        let severity = self.severity.map(|s| s.to_string()).unwrap_or_default();
        (SCREEN_FULL_DMI, format!("{base}{severity}"))
    }
}

/// tg `/datum/client_colour` types the port applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ClientColour {
    /// `/datum/client_colour/blindness` (`CLIENT_COLOR_HELMET_PRIORITY`): dim and less saturated.
    Blindness,
    /// `/datum/client_colour/monochrome` (`CLIENT_COLOR_FILTER_PRIORITY`): grayscale.
    Monochrome,
}

impl ClientColour {
    /// tg `fade_in`/`fade_out`, seconds.
    pub const FADE: f32 = 2.0;

    /// The BYOND colour matrix (rows r, g, b, a, constant).
    pub fn matrix(self) -> [f32; 20] {
        match self {
            Self::Blindness => [
                0.51, 0.3, 0.3, 0.0, 0.29, 0.51, 0.29, 0.0, 0.3, 0.3, 0.61, 0.0, 0.0, 0.0, 0.0,
                1.0, 0.0, 0.0, 0.0, 0.0,
            ],
            // `COLOR_MATRIX_GRAYSCALE`.
            Self::Monochrome => [
                0.33, 0.33, 0.33, 0.0, 0.59, 0.59, 0.59, 0.0, 0.11, 0.11, 0.11, 0.0, 0.0, 0.0, 0.0,
                1.0, 0.0, 0.0, 0.0, 0.0,
            ],
        }
    }
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

    #[test]
    fn fullscreen_icons_follow_overlay_fullscreen() {
        let icon = |c, s| FullscreenScreen::new(c, s).icon();
        assert_eq!(
            icon(ScreenCategory::Brute, Some(3)),
            (SCREEN_FULL_DMI, "brutedamageoverlay3".to_owned())
        );
        assert_eq!(icon(ScreenCategory::Crit, Some(0)).1, "passage0");
        assert_eq!(
            icon(ScreenCategory::CritVision, Some(9)).1,
            "oxydamageoverlay9"
        );
        assert_eq!(
            icon(ScreenCategory::Nearsighted, Some(2)).1,
            "impairedoverlay2"
        );
        assert_eq!(
            icon(ScreenCategory::Blind, None).1,
            "blackimageoverlaystatic"
        );
        let flicker = FullscreenScreen {
            flicker: true,
            ..FullscreenScreen::new(ScreenCategory::Blind, None)
        };
        assert_eq!(flicker.icon().1, "blackimageoverlay");
        assert_eq!(
            icon(ScreenCategory::Flash, None),
            (SCREEN_GEN_DMI, "flash".to_owned())
        );
        // Draw order is tg's layer order.
        assert!(ScreenCategory::Flash < ScreenCategory::Brute);
        assert!(ScreenCategory::Blind < ScreenCategory::Crit);
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
