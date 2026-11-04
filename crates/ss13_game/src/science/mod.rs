//! Research and development, after tg `code/modules/research/`: the station techweb
//! (`/datum/techweb/science`, `/datum/techweb_node`, `/datum/design`) in [`techweb`], `SSresearch`
//! point generation from `/obj/machinery/rnd/server` and the research queue in [`points`], the
//! R&D console (`/obj/machinery/computer/rdconsole`, tgui `Techweb`) in [`console`], the
//! protolathe, circuit imprinter and departmental techfabs (`/obj/machinery/rnd/production`,
//! tgui `Fabricator`) with their material storage (`/datum/remote_materials` and the ore silo) in
//! [`lathe`], and the destructive analyzer (tgui `DestructiveAnalyzer`) in [`analyzer`].
//! [`client`] draws the three windows. This module holds the windows' data, which both halves
//! share.
//!
//! The node, design, material and machine data come from `ss13-mapload convert`
//! (`<assets>/research.ron`, [`ss13_mapload::research`]); rerun it after updating.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::vending::ProductIcon;

#[cfg(feature = "server")]
pub mod analyzer;
#[cfg(feature = "server")]
pub mod autolathe;
#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod console;
#[cfg(feature = "server")]
pub mod lathe;
#[cfg(feature = "server")]
pub mod points;
#[cfg(feature = "server")]
pub mod techweb;

/// Research on the server: the techweb, its servers, consoles, lathes and the analyzer.
#[cfg(feature = "server")]
pub struct ScienceServerPlugin;

#[cfg(feature = "server")]
impl bevy::prelude::Plugin for ScienceServerPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_plugins((
            points::PointsPlugin,
            console::ConsolePlugin,
            lathe::LathePlugin,
            analyzer::AnalyzerPlugin,
            autolathe::AutolathePlugin,
        ));
    }
}

/// tg `item_interaction` of the R&D machines with `held`. True when the click was theirs.
#[cfg(feature = "server")]
pub fn item_interaction(
    world: &mut bevy::prelude::World,
    client: bevy::prelude::Entity,
    mob: bevy::prelude::Entity,
    held: bevy::prelude::Entity,
    target: bevy::prelude::Entity,
) -> bool {
    lathe::item_interaction(world, client, mob, held, target)
        || analyzer::item_interaction(world, client, mob, held, target)
        || autolathe::item_interaction(world, client, mob, held, target)
}

/// tgui `Techweb`.
pub struct TechwebUi;

impl UiInterface for TechwebUi {
    const NAME: &'static str = "Techweb";
    type Data = TechwebData;
    type StaticData = TechwebStatic;
}

/// A node in `node_cache`. Node and design references are indices into
/// [`TechwebStatic::nodes`] and [`TechwebStatic::designs`] (tg's `compress_id`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NodeInfo {
    /// The type path: the acts' `node_path`.
    pub path: String,
    pub name: String,
    pub description: String,
    /// Point type -> amount, before discounts.
    pub costs: Vec<(String, f64)>,
    pub prerequisites: Vec<u16>,
    pub designs: Vec<u16>,
    pub unlocked_nodes: Vec<u16>,
    pub required_experiments: Vec<String>,
    pub discount_experiments: Vec<(String, f64)>,
}

/// A design in `design_cache`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DesignInfo {
    pub name: String,
    /// Material name -> units (`OPTIMAL_COST`).
    pub cost: Vec<(String, u32)>,
    pub build_types: u32,
    pub department_flags: u32,
    pub icon: Option<ProductIcon>,
}

/// `ui_static_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TechwebStatic {
    /// `point_types_abbreviations`.
    pub point_types: Vec<(String, String)>,
    pub nodes: Vec<NodeInfo>,
    pub designs: Vec<DesignInfo>,
    /// `build_types`: bit -> machine name.
    pub build_types: Vec<(u32, String)>,
    /// `department_flags`: bit -> department name.
    pub departments: Vec<(u32, String)>,
    pub sheet_amount: f64,
}

/// A visible node's state (`nodes` entry).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NodeState {
    pub id: u16,
    pub is_free: bool,
    pub can_unlock: bool,
    pub have_experiments_done: bool,
    /// 0 researched, 1 available, 2+ steps further.
    pub tier: u32,
    pub enqueued_by_user: bool,
    pub discount_boosted: bool,
}

/// `ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TechwebData {
    pub stored_research: bool,
    pub locked: bool,
    pub nodes: Vec<NodeState>,
    /// `queue_nodes`: node -> who queued it.
    pub queue: Vec<(u16, String)>,
    pub points: Vec<(String, f64)>,
    pub points_last_tick: Vec<(String, f64)>,
    pub web_org: String,
    pub sec_protocols: bool,
    /// Experiments counted as completed.
    pub completed_experiments: Vec<String>,
}

impl TechwebData {
    pub fn node(&self, id: u16) -> Option<&NodeState> {
        self.nodes.iter().find(|n| n.id == id)
    }
}

pub const TOGGLE_LOCK: &str = "toggleLock";
pub const RESEARCH_NODE: &str = "researchNode";
pub const ENQUEUE_NODE: &str = "enqueueNode";
pub const DEQUEUE_NODE: &str = "dequeueNode";
pub const NODE_PATH: &str = "node_path";

/// tgui `Fabricator`.
pub struct FabricatorUi;

impl UiInterface for FabricatorUi {
    const NAME: &'static str = "Fabricator";
    type Data = FabricatorData;
    type StaticData = FabricatorStatic;
}

/// A storable material (`mat_container.ui_static_data` / `ui_data` entry).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MaterialInfo {
    pub name: String,
    pub color: Option<[u8; 4]>,
    /// The sheet's icon.
    pub icon: Option<ProductIcon>,
}

/// A design this machine can print once researched (`designs` entry).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FabDesign {
    /// The type path: the `build` act's `design_path`.
    pub path: String,
    pub name: String,
    pub desc: String,
    /// Material index -> units for one, with this machine's efficiency.
    pub cost: Vec<(u8, u32)>,
    pub categories: Vec<String>,
    pub icon: Option<ProductIcon>,
}

/// `ui_static_data`.
// Not tg: tg resends the static data when the techweb gains designs
// (`update_static_data_for_all_viewers`); static data is sent once here, so it lists every design
// the machine could print and `ui_data` says which of them are researched.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FabricatorStatic {
    pub fab_name: String,
    pub materials: Vec<MaterialInfo>,
    pub designs: Vec<FabDesign>,
    pub sheet_amount: f64,
}

/// `ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FabricatorData {
    /// Units per material index.
    pub materials: Vec<u32>,
    pub on_hold: bool,
    pub busy: bool,
    /// `materialMaximum`: the storage's size in units.
    pub material_maximum: u32,
    /// Indices into [`FabricatorStatic::designs`] the techweb has researched.
    pub researched: Vec<u16>,
}

impl FabricatorData {
    pub fn can_print(&self, design: &FabDesign, amount: u32) -> bool {
        design.cost.iter().all(|&(m, n)| {
            self.materials
                .get(usize::from(m))
                .is_some_and(|&have| have > 0 && n.saturating_mul(amount) <= have)
        })
    }

    /// `CustomPrint`'s `maxMult`.
    pub fn max_prints(&self, design: &FabDesign) -> u32 {
        design
            .cost
            .iter()
            .map(|&(m, n)| self.materials.get(usize::from(m)).copied().unwrap_or(0) / n.max(1))
            .min()
            .unwrap_or(0)
            .min(MAX_PRINT)
    }
}

/// tg `print_quantity = clamp(print_quantity, 1, 50)`.
pub const MAX_PRINT: u32 = 50;
pub const BUILD: &str = "build";
pub const REMOVE_MAT: &str = "remove_mat";
pub const DESIGN_PATH: &str = "design_path";
pub const AMOUNT: &str = "amount";
pub const MATERIAL_REF: &str = "ref";

/// tgui `Autolathe`.
pub struct AutolatheUi;

impl UiInterface for AutolatheUi {
    const NAME: &'static str = "Autolathe";
    type Data = AutolatheData;
    type StaticData = AutolatheStatic;
}

/// `ui_static_data`: every design the autolathe prints, researched (`initial`) or hacked.
// Not tg: tg resends static data on hacking and on `RefreshParts`; here the costs are at
// efficiency 1 and `ui_data` carries the efficiency and the hack state.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AutolatheStatic {
    pub fab: FabricatorStatic,
    /// Designs printed at their exact cost (stacks, `fixed_cost_efficiency`).
    pub exact: Vec<u16>,
    /// Designs shown only while hacked.
    pub hacked_only: Vec<u16>,
}

impl AutolatheStatic {
    /// The catalogue with each cost at `efficiency` (`OPTIMAL_COST(cost * coeff)`).
    pub fn priced(&self, efficiency: f64) -> FabricatorStatic {
        let mut fab = self.fab.clone();
        for (i, d) in fab.designs.iter_mut().enumerate() {
            if self.exact.contains(&(i as u16)) {
                continue;
            }
            for (_, n) in &mut d.cost {
                *n = (f64::from(*n) * efficiency).floor().max(1.0) as u32;
            }
        }
        fab
    }

    /// The designs shown: all but the hacked ones unless hacked.
    pub fn shown(&self, hacked: bool) -> Vec<u16> {
        (0..self.fab.designs.len() as u16)
            .filter(|i| hacked || !self.hacked_only.contains(i))
            .collect()
    }
}

/// `ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AutolatheData {
    /// Units per material index.
    pub materials: Vec<u32>,
    pub material_total: u32,
    pub materials_max: u32,
    /// `busy`.
    pub active: bool,
    /// `creation_efficiency`.
    pub efficiency: f64,
    pub hacked: bool,
}

pub const MAKE: &str = "make";
pub const EJECT: &str = "eject";
pub const MULTIPLIER: &str = "multiplier";

/// tgui `DestructiveAnalyzer`.
pub struct DestructiveAnalyzerUi;

impl UiInterface for DestructiveAnalyzerUi {
    const NAME: &'static str = "DestructiveAnalyzer";
    type Data = AnalyzerData;
    type StaticData = ();
}

/// `node_data` entry.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AnalyzerNode {
    pub node_name: String,
    pub node_path: String,
    pub node_hidden: bool,
}

/// `ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AnalyzerData {
    pub server_connected: bool,
    pub loaded_item: Option<String>,
    pub item_icon: Option<ProductIcon>,
    /// tg's name for the opposite: true when the item can be destroyed.
    pub indestructible: bool,
    pub already_deconstructed: bool,
    pub recoverable_points: String,
    pub node_data: Vec<AnalyzerNode>,
}

pub const EJECT_ITEM: &str = "eject_item";
pub const DECONSTRUCT: &str = "deconstruct";
pub const DECONSTRUCT_PATH: &str = "deconstruct_path";
