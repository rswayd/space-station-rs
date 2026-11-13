//! Mining, after tg `code/modules/mining/` and `code/game/turfs/closed/minerals.dm`: mineral
//! walls dug with pickaxes, drills and kinetic accelerators into ore ([`rocks`]), gibtonite,
//! the mining satchel and the ore box, the ore redemption machine that smelts ore into the ore
//! silo and pays mining points ([`orm`]), and the mining equipment order console that spends
//! them ([`console`]), mining vouchers and the point transfer card ([`equipment`]), kinetic
//! accelerator modkits ([`modkits`]), the stacking machine with its console ([`stacking`]) and
//! the fauna's monster cores with the stabilizing serum ([`cores`]), the resonator
//! ([`resonator`]), bluespace shelter capsules ([`capsule`]), marker beacons ([`beacons`]), the explorer gear's
//! armour plates ([`explorer`]), fulton extraction ([`fulton`]) and the necropolis tendrils with
//! their chests ([`necropolis`]), the lazarus injector ([`devices`]), the ore vents with their
//! wave defense ([`vents`]), boulders with the BRM, the refinery and the smelter
//! ([`boulders`]).
//! This module holds the windows' data, which both halves share; the client views are in
//! [`client`].
//!
//! The mining level loads with the station (`ss13_mapload::MiningLevel`) and the mining
//! shuttles fly there (`crate::mining_shuttles`). The processing unit and the unloading machine
//! are not ported.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::vending::ProductIcon;

#[cfg(feature = "server")]
pub mod beacons;
#[cfg(feature = "server")]
pub mod boulders;
#[cfg(feature = "server")]
pub mod capsule;
#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod console;
#[cfg(feature = "server")]
pub mod cores;
#[cfg(feature = "server")]
pub mod crusher;
#[cfg(feature = "server")]
pub mod devices;
#[cfg(feature = "server")]
pub mod equipment;
#[cfg(feature = "server")]
pub mod explorer;
#[cfg(feature = "server")]
pub mod fulton;
#[cfg(feature = "server")]
pub mod modkits;
#[cfg(feature = "server")]
pub mod necropolis;
#[cfg(feature = "server")]
pub mod orm;
#[cfg(feature = "server")]
pub mod resonator;
#[cfg(feature = "server")]
pub mod rocks;
#[cfg(feature = "server")]
pub mod stacking;
#[cfg(feature = "server")]
pub mod vents;

#[cfg(feature = "server")]
pub use rocks::{MiningJob, complete};

/// Mining on the server.
#[cfg(feature = "server")]
pub struct MiningServerPlugin;

#[cfg(feature = "server")]
impl bevy::prelude::Plugin for MiningServerPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // Weak rock by hand; the ore box, ORM and mining console windows.
        // Monster cores used in hand.
        app.add_click_handler(ClickPhase::AttackSelf, 3183, |w, c| {
            crate::mining::cores::attack_self(w, c.client, c.mob, c.target)
        });
        // Resonator modes.
        app.add_click_handler(ClickPhase::AttackSelf, 3184, |w, c| {
            crate::mining::resonator::attack_self(w, c.client, c.target)
        });
        // Shelter capsules arm.
        app.add_click_handler(ClickPhase::AttackSelf, 3185, |w, c| {
            crate::mining::capsule::attack_self(w, c.mob, c.target)
        });
        // Marker beacon stacks anchor a beacon.
        app.add_click_handler(ClickPhase::AttackSelf, 3186, |w, c| {
            crate::mining::beacons::attack_self(w, c.client, c.mob, c.target)
        });
        // Fulton packs link to a beacon, beacon kits unfold.
        app.add_click_handler(ClickPhase::AttackSelf, 3188, |w, c| {
            crate::mining::fulton::attack_self(w, c.client, c.mob, c.target)
        });
        app.add_click_handler(ClickPhase::EmptyHand, 2900, |w, c| {
            crate::mining::interact(w, c.client, c.mob, c.target)
        });
        // Mining tools on rock, ore into the ORM or an ore box, a welder on ore.
        app.add_click_handler(ClickPhase::HeldOnTarget, 3000, |w, c| {
            c.held
                .is_some_and(|h| crate::mining::item_interaction(w, c.client, c.mob, h, c.target))
        });
        // The same with the other hand's item: a modkit and an accelerator, an ID and a point
        // card, a tool and gibtonite ore.
        app.add_click_handler(ClickPhase::OtherHand, 1450, |w, c| {
            c.held
                .is_some_and(|h| crate::mining::hand_interaction(w, c.client, c.mob, h, c.target))
        });
        app.add_plugins((
            rocks::RocksPlugin,
            orm::OrmPlugin,
            console::OrderConsolePlugin,
            equipment::EquipmentPlugin,
            stacking::StackingPlugin,
            cores::CoresPlugin,
            resonator::ResonatorPlugin,
            capsule::CapsulePlugin,
            beacons::BeaconsPlugin,
            explorer::ExplorerPlugin,
            fulton::FultonPlugin,
            crusher::CrusherPlugin,
            necropolis::NecropolisPlugin,
            devices::DevicesPlugin,
        ))
        .add_plugins((vents::VentsPlugin, boulders::BouldersPlugin));
    }
}

/// tg `attack_hand` of mining things with an empty hand: weak walls, the ore box, the ORM and
/// the order console. True when handled.
#[cfg(feature = "server")]
pub fn interact(
    world: &mut bevy::prelude::World,
    client: bevy::prelude::Entity,
    mob: bevy::prelude::Entity,
    target: bevy::prelude::Entity,
) -> bool {
    rocks::interact(world, client, mob, target)
        || orm::interact(world, client, mob, target)
        || console::interact(world, client, mob, target)
        || stacking::interact(world, client, mob, target)
        || beacons::interact(world, client, mob, target)
        || fulton::interact(world, client, mob, target)
}

/// The mining `item_interaction`s of a held item: digging, ore into the ORM or an ore box,
/// smelting ore with a welder. True when handled.
#[cfg(feature = "server")]
pub fn item_interaction(
    world: &mut bevy::prelude::World,
    client: bevy::prelude::Entity,
    mob: bevy::prelude::Entity,
    held: bevy::prelude::Entity,
    target: bevy::prelude::Entity,
) -> bool {
    orm::item_interaction(world, held, target)
        || vents::item_interaction(world, client, mob, held, target)
        || boulders::item_interaction(world, client, mob, held, target)
        || devices::item_interaction(world, mob, held, target)
        || cores::item_interaction(world, client, mob, held, target)
        || resonator::item_interaction(world, mob, held, target)
        || beacons::item_interaction(world, client, mob, held, target)
        || explorer::item_interaction(world, client, held, target)
        || fulton::item_interaction(world, client, mob, held, target)
        || modkits::item_interaction(world, client, mob, held, target)
        || equipment::item_interaction(world, client, mob, held, target)
        || stacking::item_interaction(world, client, mob, held, target)
        || rocks::item_interaction(world, client, mob, held, target)
}

/// [`item_interaction`] of the active hand's `held` on the item in the other hand: the ones
/// that work between two items. True when handled.
#[cfg(feature = "server")]
pub fn hand_interaction(
    world: &mut bevy::prelude::World,
    client: bevy::prelude::Entity,
    mob: bevy::prelude::Entity,
    held: bevy::prelude::Entity,
    target: bevy::prelude::Entity,
) -> bool {
    modkits::item_interaction(world, client, mob, held, target)
        || equipment::item_interaction(world, client, mob, held, target)
        || explorer::item_interaction(world, client, held, target)
        || rocks::hand_interaction(world, client, mob, held, target)
}

/// The mining `examine` lines: accelerators and modkits, the point card, gibtonite ore.
#[cfg(feature = "server")]
pub fn examine_lines(
    world: &bevy::prelude::World,
    target: bevy::prelude::Entity,
    lines: &mut Vec<crate::examine::Line>,
) {
    modkits::examine_lines(world, target, lines);
    cores::examine_lines(world, target, lines);
    capsule::examine_lines(world, target, lines);
    beacons::examine_lines(world, target, lines);
    explorer::examine_lines(world, target, lines);
    fulton::examine_lines(world, target, lines);
    equipment::examine_lines(world, target, lines);
    rocks::examine_lines(world, target, lines);
    devices::examine_lines(world, target, lines);
    vents::examine_lines(world, target, lines);
    boulders::examine_lines(world, target, lines);
}

/// The mining `click_alt`s: rotating an open ORM's input and output. True when handled.
#[cfg(feature = "server")]
pub fn click_alt(
    world: &mut bevy::prelude::World,
    client: bevy::prelude::Entity,
    mob: bevy::prelude::Entity,
    target: bevy::prelude::Entity,
) -> bool {
    orm::click_alt(world, client, target)
        || beacons::click_alt(world, client, mob, target)
        || explorer::click_alt(world, client, mob, target)
}

/// tgui `OreBox`.
pub struct OreBoxUi;

impl UiInterface for OreBoxUi {
    const NAME: &'static str = "OreBox";
    type Data = OreBoxData;
    type StaticData = ();
}

/// One `materials` entry of the ore box.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OreBoxEntry {
    pub name: String,
    pub amount: u32,
}

/// `/obj/structure/ore_box/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OreBoxData {
    pub materials: Vec<OreBoxEntry>,
}

pub const REMOVE_ALL: &str = "removeall";

/// tgui `OreRedemptionMachine`.
pub struct OrmUi;

impl UiInterface for OrmUi {
    const NAME: &'static str = "OreRedemptionMachine";
    type Data = OrmData;
    type StaticData = ();
}

/// A `materials` entry: a stored material (`category` "material") or an alloy design
/// ("alloy").
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OrmMaterial {
    pub name: String,
    /// The material's or the alloy design's index (the `material_ref` / `path` act param).
    pub id: u32,
    /// Sheets (materials, fractional) or how many can be smelted (alloys).
    pub amount: f64,
    pub alloy: bool,
    /// `ore_values`: credits per sheet (0: none shown).
    pub value: f32,
    pub icon: Option<ProductIcon>,
}

/// `ui_data` `user`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrmUser {
    pub name: String,
    pub cash: i64,
}

/// `/obj/machinery/mineral/ore_redemption/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OrmData {
    pub unclaimed_points: i64,
    pub materials: Vec<OrmMaterial>,
    /// Why the silo cannot be used; `None`: connected.
    pub disconnected: Option<String>,
    pub user: Option<OrmUser>,
}

pub const CLAIM: &str = "Claim";
pub const RELEASE: &str = "Release";
pub const SMELT: &str = "Smelt";
pub const MATERIAL_REF: &str = "material_ref";
pub const PATH: &str = "path";
pub const SHEETS: &str = "sheets";

/// tgui `ProduceConsole`.
pub struct ProduceConsoleUi;

impl UiInterface for ProduceConsoleUi {
    const NAME: &'static str = "ProduceConsole";
    type Data = ProduceData;
    type StaticData = ProduceStatic;
}

/// An `order_datums` entry.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OrderDatum {
    pub name: String,
    pub desc: String,
    pub cat: String,
    /// The `target` act param: the index into the console's catalogue.
    pub r#ref: u32,
    /// `round(cost_per_order * cargo_cost_multiplier)`.
    pub cost: u32,
    pub max: u32,
    pub icon: Option<ProductIcon>,
}

/// `/obj/machinery/computer/order_console/ui_static_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ProduceStatic {
    pub credit_type: String,
    pub express_tooltip: String,
    pub purchase_tooltip: String,
    pub forced_express: bool,
    pub cargo_value: u32,
    pub cargo_cost_multiplier: f32,
    pub express_cost_multiplier: f32,
    pub order_categories: Vec<String>,
    pub order_datums: Vec<OrderDatum>,
}

/// An `item_amts` entry.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemAmount {
    pub name: String,
    pub amt: u32,
}

/// `ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ProduceData {
    /// `get_total_cost()`, before the delivery multiplier.
    pub total_cost: u32,
    pub off_cooldown: bool,
    pub item_amts: Vec<ItemAmount>,
    /// `retrieve_points(id)`; `None` without an ID.
    pub points: Option<i64>,
}

impl ProduceData {
    /// `findAmount`.
    pub fn amount(&self, name: &str) -> u32 {
        self.item_amts
            .iter()
            .find(|i| i.name == name)
            .map_or(0, |i| i.amt)
    }
}

pub const ADD_ONE: &str = "add_one";
pub const REMOVE_ONE: &str = "remove_one";
pub const CART_SET: &str = "cart_set";
pub const PURCHASE: &str = "purchase";
pub const EXPRESS: &str = "express";
pub const TARGET: &str = "target";
pub const AMT: &str = "amt";

/// tgui `StackingConsole`.
pub struct StackingConsoleUi;

impl UiInterface for StackingConsoleUi {
    const NAME: &'static str = "StackingConsole";
    type Data = StackingData;
    type StaticData = ();
}

/// A `contents` entry: a kind of stored sheet.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StackingSheet {
    /// The `type` act param.
    pub r#type: String,
    pub name: String,
    pub amount: u32,
}

/// `/obj/machinery/mineral/stacking_unit_console/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StackingData {
    pub machine: bool,
    pub stacking_amount: Option<u32>,
    pub input_direction: String,
    pub output_direction: String,
    pub contents: Vec<StackingSheet>,
}

pub const STACK_RELEASE: &str = "release";
pub const STACK_ROTATE: &str = "rotate";
pub const STACK_TYPE: &str = "type";
pub const STACK_INPUT: &str = "input";
