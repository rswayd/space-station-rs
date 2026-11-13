//! Station bots, after tg `/mob/living/basic/bot` (`code/modules/mob/living/basic/
//! bots/_bots.dm`, `bot_ai.dm`, `code/datums/ai/bots/`) and its subtypes; navigation beacons
//! (`code/game/machinery/navbeacon.dm`).
//!
//! A basic mob whose `mobs.ron` definition has bot vars ([`ss13_mapload::mobs::BotDef`]) gets a
//! [`Bot`]: its mode flags (on, autopatrol, remote control), cover flags (maintenance panel,
//! control lock, emagged), its access card (`ACCESS_ROBOTICS` plus the `additional_access` trim;
//! doors open to it, `crate::doors`, and its AI paths through them,
//! `crate::mobs::ai::movement`), its radio channel and its kind's settings. Its AI tree is
//! [`ai`]'s (the secbot family's and the smaller kinds' parts in [`sec`]).
//!
//! - A human's empty hand out of combat mode opens the `SimpleBot` window ([`ui`], the MULEbot's
//!   `Mule`, [`mule`]); an ID card or an alt-click with one locks or unlocks the controls (tg
//!   `unlock_with_id`, with the bot's `req_one_access`); a screwdriver opens the maintenance
//!   panel while unlocked; a welder out of combat mode repairs 10 with the panel open; an emag
//!   unlocks the controls, then (panel open) emags the bot. Melee and other items fall through
//!   to `crate::mobs::defense`.
//! - [`speak`]: the bot says a line with its say verbs, plays the line's recording ([`voice`])
//!   and says it on its radio channel when asked (tg `bot_announcement/announce`), only while
//!   on.
//! - Death (tg `explode`): "blows apart!", its kind's parts drop, the robot arm in half the
//!   cases; bots are `DEL_ON_DEATH`.
//! - [`BotTask`]: the bot's own `do_after` (cleaning, tending, cuffing); the AI pauses while it
//!   runs (tg pauses controllers during `do_after`), and it ends if either side moves.
//! - [`construction`]: bot assemblies built from items; [`cleanbot`]: its knife, titles and acid;
//!   [`repair`]: the repairbot's stock, repairs and girders.
//!
//! The `SimpleBot` and `Mule` windows' data is here (both halves); [`client`] draws them.
// shortcut: no pAI cards, ghost possession (so no `BotAnnouncement` action), path HUD,
// diagnostic HUD or PDA/AI remote control and summoning; add with pAIs, ghost roles and the
// bot control program. EMPs: `crate::emp::mobs`.

#[cfg(feature = "server")]
pub mod ai;
#[cfg(feature = "server")]
pub mod cleanbot;
#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod construction;
#[cfg(feature = "server")]
mod core;
#[cfg(feature = "server")]
pub mod mule;
#[cfg(feature = "server")]
pub mod repair;
#[cfg(feature = "server")]
pub mod sec;
#[cfg(feature = "server")]
pub mod ui;
#[cfg(feature = "server")]
pub mod voice;

#[cfg(feature = "server")]
pub use core::*;

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

/// tgui `SimpleBot` (`tgui/packages/tgui/interfaces/SimpleBot.tsx`).
pub struct SimpleBotUi;

impl UiInterface for SimpleBotUi {
    const NAME: &'static str = "SimpleBot";
    type Data = SimpleBotData;
    type StaticData = ();
}

/// `/mob/living/basic/bot/ui_data` and the kinds' `custom_controls`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SimpleBotData {
    pub can_hack: bool,
    pub emagged: bool,
    pub has_access: bool,
    pub locked: bool,
    /// `settings`, empty (`None`) while locked to this user.
    pub settings: Option<BotSettings>,
    /// `custom_controls` in tg's order: a toggle's 0/1, or the medbot's `heal_threshold`.
    pub custom_controls: Vec<(String, i32)>,
    /// The repairbot's `repairbot_materials`: what `remove_item` hands back.
    pub materials: Vec<BotMaterial>,
}

/// A stack the repairbot holds: `remove_item`'s `item_reference`, its name and amount.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct BotMaterial {
    pub key: String,
    pub name: String,
    pub amount: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct BotSettings {
    pub airplane_mode: bool,
    pub maintenance_lock: bool,
    pub power: bool,
    pub patrol_station: bool,
}

/// tgui `Mule` (`tgui/packages/tgui/interfaces/Mule.tsx`).
pub struct MuleUi;

impl UiInterface for MuleUi {
    const NAME: &'static str = "Mule";
    type Data = MuleData;
    type StaticData = ();
}

/// `/mob/living/basic/bot/mulebot/ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct MuleData {
    /// `powerStatus`.
    pub power: bool,
    /// `cellPercent` (`None`: no cell).
    pub cell_percent: Option<f32>,
    /// `loadName` while loaded (`load`).
    pub load_name: Option<String>,
    pub locked: bool,
    /// `siliconUser`.
    pub silicon: bool,
    pub mode: String,
    /// `modeStatus`: `good`, `average`, `bad` or empty.
    pub mode_status: String,
    pub auto_return: bool,
    pub auto_pickup: bool,
    pub report_delivery: bool,
    pub destination: Option<String>,
    /// `destinationsList` (`GLOB.deliverybeacontags`).
    pub destinations: Vec<String>,
    /// `homeDestination`.
    pub home: Option<String>,
    pub bot_id: String,
}

/// The medbot's slider control (`MedbotThreshold`).
pub const HEAL_THRESHOLD: &str = "heal_threshold";
