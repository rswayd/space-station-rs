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
//! - The fishing skill (`crate::skills`): the minigame's experience, its difficulty and
//!   completion rates, a longer cast, and what an angler sees: fish size and weight (Novice),
//!   their health (Apprentice), a spot's fish (Journeyman), what bites (Master).
//! - The fish analyzer and catalog (`catalog`), trophy mounts (`mount`) and the admin
//!   fishing calculator (`admin`).
// shortcut: no tackle slots but the bait (no line, hook or lure choice, no `FishingRod` window),
// no item hooking or reeling, no favourite or disliked baits, no breeding, growth, evolution,
// petting or aquarium visuals, no toilet spots (BandaStation removed them), no portal research
// unlocks or multitool links.

pub mod minigame;

#[cfg(feature = "server")]
pub mod admin;
#[cfg(feature = "server")]
pub mod aquarium;
#[cfg(feature = "server")]
pub mod catalog;
#[cfg(feature = "client")]
pub mod catalog_client;
#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod fish;
#[cfg(feature = "server")]
pub mod mount;
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

/// tgui `FishAnalyzer` (`/obj/item/fish_analyzer`).
pub struct FishAnalyzerUi;

impl UiInterface for FishAnalyzerUi {
    const NAME: &'static str = "FishAnalyzer";
    type Data = FishAnalyzerData;
    type StaticData = ();
}

/// One `fish_traits` row of the analyzer.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct FishTraitInfo {
    pub trait_name: String,
    pub trait_desc: String,
    pub trait_inherit: u32,
}

/// One fish of the analyzer's `fish_list` (`extract_fish_info`, without evolutions and breeding).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct AnalyzedFish {
    pub fish_name: String,
    pub fish_food: String,
    pub fish_min_temp: f32,
    pub fish_max_temp: f32,
    pub fish_fluid_compatible: bool,
    pub fish_fluid_type: String,
    pub fish_traits: Vec<FishTraitInfo>,
    pub fish_suitable_temp: bool,
    /// Percent; 0 when dead.
    pub fish_health: f32,
    pub fish_size: f32,
    pub fish_weight: f32,
    /// `1 - get_hunger()`: 1 fed, 0 starving.
    pub fish_hunger: f32,
}

/// The analyzer's `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct FishAnalyzerData {
    /// A single fish was scanned (else an aquarium's fish).
    pub fish_scanned: bool,
    pub fish_list: Vec<AnalyzedFish>,
}

/// tgui `FishCatalog` (`/obj/item/book/manual/fish_catalog`).
pub struct FishCatalogUi;

impl UiInterface for FishCatalogUi {
    const NAME: &'static str = "FishCatalog";
    type Data = ();
    type StaticData = FishCatalogStatic;
}

/// One fish of the catalog (`fish_data` with its `fishing_tips`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct CatalogFish {
    pub name: String,
    pub desc: String,
    pub fluid: String,
    pub temp_min: f32,
    pub temp_max: f32,
    /// "Aquatech Ltd. Fish Packs" or "Unknown".
    pub source: String,
    pub size: f32,
    pub weight: f32,
    pub feed: String,
    pub spots: String,
    pub favorite_bait: String,
    pub disliked_bait: String,
    pub traits: Vec<String>,
    /// "Easy", "Medium", "Hard".
    pub difficulty: String,
}

/// The catalog's `ui_static_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct FishCatalogStatic {
    pub fish_info: Vec<CatalogFish>,
    pub sponsored_by: String,
}

/// tgui `FishingCalculator` (the `R_DEBUG` admin verb).
pub struct FishingCalculatorUi;

impl UiInterface for FishingCalculatorUi {
    const NAME: &'static str = "FishingCalculator";
    type Data = CalculatorData;
    type StaticData = CalculatorStatic;
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct CalculatorStatic {
    pub rod_types: Vec<String>,
    pub spot_types: Vec<String>,
}

/// One outcome of the calculated table.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct CalculatorRow {
    pub result: String,
    pub weight: f64,
    pub difficulty: f32,
    /// The limited count left, or "Infinite".
    pub count: String,
}

/// The calculator's `ui_data`: the chosen setup (kept on the server) and the last table.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct CalculatorData {
    pub spot: String,
    pub rod: String,
    pub bait: String,
    pub info: Vec<CalculatorRow>,
}

/// Acts of [`FishingCalculatorUi`] and their params.
pub const CALC_SET_SPOT: &str = "set_spot";
pub const CALC_SET_ROD: &str = "set_rod";
pub const CALC_SET_BAIT: &str = "set_bait";
pub const CALC_RECALC: &str = "recalc";
pub const CALC_VALUE: &str = "value";
