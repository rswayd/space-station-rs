//! The changeling (tg `/datum/antagonist/changeling`): a shapeshifting parasite that absorbs the
//! crew's DNA, takes their shape and spends chemicals on powers bought with genetic points.
//! [`server`] is the antagonist, [`client`] draws its windows; this module holds what both share.
//!
//! - `AntagInfoChangeling`: the info panel (`AntagInfoChangeling.tsx`): the hivemind name, the
//!   objectives, the absorbed DNA count, the abilities' introduction and the stolen memories.
//! - `CellularEmporium`: the power shop (`CellularEmporium.tsx`); acts `evolve` (`path`) and
//!   `readapt`.
//! - `ChangelingHud` (headless): the chemical counter on the HUD (`/atom/movable/screen/ling/
//!   chems` at `ui_lingchemdisplay`).

use serde::{Deserialize, Serialize};
use ss13_shared::IconStateId;
use ss13_tgui::UiInterface;

use super::ObjectiveInfo;

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod server;

/// tg `ROLE_CHANGELING`: the changeling's `be_special` key.
pub const ROLE_CHANGELING: &str = "Changeling";

/// `act('evolve', { path })`.
pub const EVOLVE: &str = "evolve";
/// `act('readapt')`.
pub const READAPT: &str = "readapt";

pub struct AntagInfoChangelingUi;

impl UiInterface for AntagInfoChangelingUi {
    const NAME: &'static str = "AntagInfoChangeling";
    type Data = ChangelingInfo;
    type StaticData = ();
}

/// `/datum/antagonist/changeling/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ChangelingInfo {
    pub true_name: String,
    pub hive_name: String,
    /// `antag_memory`: what absorbed antagonists knew.
    pub stolen_antag_info: String,
    pub memories: Vec<Memory>,
    pub objectives: Vec<ObjectiveInfo>,
    pub can_change_objective: bool,
    pub absorbed_dna: u32,
}

/// `stolen_memories` entry.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Memory {
    pub name: String,
    pub story: String,
}

pub struct CellularEmporiumUi;

impl UiInterface for CellularEmporiumUi {
    const NAME: &'static str = "CellularEmporium";
    type Data = EmporiumData;
    type StaticData = EmporiumStatic;
}

/// `/datum/cellular_emporium/ui_static_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EmporiumStatic {
    pub abilities: Vec<Ability>,
}

/// One purchasable power (`dna_cost >= 0`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Ability {
    pub name: String,
    pub desc: String,
    /// `button_icon_state` in `actions_changeling.dmi`, and its resolved sprite with the
    /// `bg_changeling` background (tgui `DmIcon`).
    pub icon: String,
    pub icon_state: Option<IconStateId>,
    pub background: Option<IconStateId>,
    pub helptext: String,
    /// The `/datum/action/changeling` type: the `evolve` act's `path`.
    pub path: String,
    pub genetic_point_required: u32,
    pub absorbs_required: u32,
    pub dna_required: u32,
    pub category: String,
}

/// `/datum/cellular_emporium/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmporiumData {
    pub can_readapt: u32,
    pub genetic_points_count: u32,
    pub owned_abilities: Vec<String>,
    pub absorb_count: u32,
    pub dna_count: u32,
}

impl EmporiumData {
    /// `ItemList`'s `canAfford`.
    pub fn can_afford(&self, ability: &Ability) -> bool {
        !self.owned_abilities.contains(&ability.path)
            && ability.genetic_point_required <= self.genetic_points_count
            && ability.absorbs_required <= self.absorb_count
            && ability.dna_required <= self.dna_count
    }
}

/// The chemical counter.
pub struct ChangelingHudUi;

impl UiInterface for ChangelingHudUi {
    const NAME: &'static str = "ChangelingHud";
    type Data = ChangelingHudData;
    type StaticData = ();
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(bevy::prelude::Component))]
pub struct ChangelingHudData {
    /// `round(chem_charges)`.
    pub chem_charges: u32,
    /// `total_chem_storage` (shown while hovered).
    pub chem_max: u32,
}
