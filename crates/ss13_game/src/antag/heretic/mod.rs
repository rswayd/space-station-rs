//! The heretic (tg `/datum/antagonist/heretic`, `code/modules/antagonists/heretic/`): a crew
//! member touched by the Mansus who drains reality's influences for knowledge, draws a
//! transmutation rune, sacrifices the targets its living heart tracks and ascends.
//! [`server`] is the antagonist, its rituals and the Ash path; [`client`] draws the research
//! window; [`knowledge`] is the knowledge tree and recipe matching, [`data`] the knowledge table
//! generated from the DM dump.
//!
//! - `AntagInfoHeretic` (`AntagInfoHeretic.tsx`): the introduction and objectives, the paths
//!   and the knowledge tree with the shop. Act `research` (`path`, `category`).
//! - The rituals on a rune and the living heart's targets are `ListInputWindow`s (tg radials).

use serde::{Deserialize, Serialize};
use ss13_shared::IconStateId;
use ss13_tgui::UiInterface;

use super::ObjectiveInfo;

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod data;
#[cfg(feature = "server")]
pub mod knowledge;
#[cfg(feature = "server")]
pub mod server;

/// tg `ROLE_HERETIC`: the heretic's `be_special` key.
pub const ROLE_HERETIC: &str = "Heretic";

/// `act('research', { path, category })`.
pub const RESEARCH: &str = "research";

pub struct AntagInfoHereticUi;

impl UiInterface for AntagInfoHereticUi {
    const NAME: &'static str = "AntagInfoHeretic";
    type Data = HereticInfo;
    /// `path_info`: the same for every heretic.
    type StaticData = HereticStatic;
}

/// `path_info` (tg sends it in `ui_data`; it never changes).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HereticStatic {
    pub paths: Vec<HereticPath>,
    /// `knowledge.dmi` `node_finished` and `node_locked`.
    pub node_finished: Option<IconStateId>,
    pub node_locked: Option<IconStateId>,
}

/// `/datum/heretic_knowledge_tree_column/get_ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HereticPath {
    pub route: String,
    pub complexity: String,
    pub complexity_color: String,
    pub description: Vec<String>,
    pub pros: Vec<String>,
    pub cons: Vec<String>,
    pub tips: Vec<String>,
    pub starting_knowledge: KnowledgeNode,
    pub preview_abilities: Vec<KnowledgeNode>,
    pub passive_name: String,
    pub passive_recharge: String,
    pub passive_description: Vec<String>,
}

/// `get_knowledge_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct KnowledgeNode {
    pub path: String,
    pub name: String,
    pub desc: String,
    pub gain_flavor: String,
    pub cost: u32,
    pub depth: u32,
    /// The background state (`node_ash`, `node_side`...) and its sprite.
    pub bgr: String,
    pub bgr_icon: Option<IconStateId>,
    pub icon: Option<IconStateId>,
    /// `start`, `tree`, `shop` or `draft`: the `research` act's `category`.
    pub category: String,
    pub done: bool,
    pub ascension: bool,
    pub disabled: bool,
    pub notice: String,
    pub info: String,
}

/// `/datum/antagonist/heretic/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HereticInfo {
    /// `knowledge_points`.
    pub charges: u32,
    pub total_sacrifices: u32,
    pub ascended: bool,
    pub objectives: Vec<ObjectiveInfo>,
    pub can_change_objective: bool,
    pub passive_level: u32,
    pub points_to_aura: u32,
    /// By depth (1-based in tg: index 0 is depth 1).
    pub knowledge_tiers: Vec<Vec<KnowledgeNode>>,
    pub knowledge_shop: Vec<KnowledgeNode>,
}

impl HereticInfo {
    /// The client's `currentPath`: the path whose starting knowledge is researched.
    pub fn current_path<'a>(&self, paths: &'a [HereticPath]) -> Option<&'a HereticPath> {
        paths.iter().find(|p| {
            self.knowledge_tiers
                .iter()
                .flatten()
                .any(|n| n.done && n.path == p.starting_knowledge.path)
        })
    }
}
