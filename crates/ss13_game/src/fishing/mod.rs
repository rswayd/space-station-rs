//! Fishing (tg `code/modules/fishing`): rods, fishing spots, the catching minigame, fish and
//! aquariums. The data (fish and rod vars, the fish sources' tables, the water turfs that are
//! spots) is [`ss13_mapload::fishing`].
//!
//! - A rod ([`server::FishingRod`]) cast at a spot in reach (`cast_range` plus the line's 2
//!   tiles) starts a challenge ([`server::Fishing`] on the fisher): a wait of 3-25 s, then a bite
//!   with 4 s to strike (any click with the rod); a click while waiting is a miss and adds
//!   3-5 s. The reward is rolled at the bite from the source's table, weighted by the bait
//!   (any food in the rod; worms and doughballs are rated) and the fish's traits. Striking opens
//!   the minigame window ([`FishingMinigameUi`], simulated by [`minigame`]); winning puts the
//!   catch in hand and eats the bait. Dropping the rod, walking out of reach or the spot going
//!   away snaps the line; the rod used in hand stops fishing.
//! - Spots: water turfs (river, ocean, beach, Tiziran sea, hot springs), aquariums (their own
//!   live fish) and an active fish-porter (its dimension).
//! - Fish ([`server::Fish`]) get a gaussian size and weight when caught (the type's average
//!   otherwise) and a weight class from the size; they starve, and out of water or in a wrong
//!   aquarium they lose health until they die. Examining one shows its size and weight.
//! - Aquariums and fish tanks hold fish (a fish or fish feed used on them) with the `Aquarium`
//!   window: fluid, temperature, mode (Assisted picks settings for its fish, Stasis pauses them)
//!   and taking fish out.
//! - The fish-porter opens a portal to the aquarium dimension (and the Syndicate one when
//!   emagged) that is a fishing spot while powered.
// shortcut: no tackle slots but the bait (no line, hook or lure choice, no `FishingRod` window),
// no item hooking or reeling, no fishing skill (every fisher is at `SKILL_LEVEL_NONE`), no
// favourite or disliked baits, no breeding, growth, evolution, petting or aquarium visuals, no
// toilet spots (BandaStation removed them), no portal research unlocks or multitool links.

pub mod minigame;

#[cfg(feature = "server")]
pub mod aquarium;
#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod fish;
#[cfg(feature = "server")]
pub mod server;

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

pub use minigame::MinigameParams;

/// The minigame window (tg draws it as a HUD over the map).
pub struct FishingMinigameUi;

impl UiInterface for FishingMinigameUi {
    const NAME: &'static str = "FishingMinigame";
    type Data = ();
    type StaticData = MinigameParams;
}

/// Acts of [`FishingMinigameUi`]: the client's outcome.
pub const WIN: &str = "win";
pub const LOSE: &str = "lose";

/// tgui `Aquarium` (`/datum/component/aquarium`).
pub struct AquariumUi;

impl UiInterface for AquariumUi {
    const NAME: &'static str = "Aquarium";
    type Data = AquariumData;
    type StaticData = AquariumStatic;
}

/// `ui_static_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct AquariumStatic {
    pub min_temperature: f32,
    pub max_temperature: f32,
    pub fluid_types: Vec<String>,
    pub aquarium_modes: Vec<String>,
}

/// One fish row of `fishData`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct AquariumFish {
    /// The entity bits (tg `REF(fish)`).
    pub fish_ref: String,
    pub fish_name: String,
    /// `get_happiness_value`: -1 (sad) to 4.
    pub fish_happiness: i32,
    pub fish_alive: bool,
}

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct AquariumData {
    pub fluid_type: String,
    pub temperature: f32,
    pub locked_fluid_temp: bool,
    pub current_mode: String,
    pub fish_data: Vec<AquariumFish>,
    /// Minutes.
    pub feeding_interval: f32,
}

/// Acts of [`AquariumUi`] and their params.
pub const TEMPERATURE: &str = "temperature";
pub const FLUID: &str = "fluid";
pub const CHANGE_MODE: &str = "change_mode";
pub const NEW_MODE: &str = "new_mode";
pub const FEEDING_INTERVAL: &str = "feeding_interval";
pub const REMOVE_ITEM: &str = "remove_item";
pub const ITEM_REFERENCE: &str = "item_reference";
