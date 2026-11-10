//! Exosuits, after tg `code/modules/vehicles/mecha/`: the mechs themselves
//! (`/obj/vehicle/sealed/mecha`: entering, driving, the pilot's clicks, damage, wreckage) in
//! [`core`], their equipment (`/obj/item/mecha_parts/mecha_equipment`: the hydraulic clamp, the
//! drill, the cargo compartment) in [`equipment`], the weapons and their ammo boxes in
//! [`weapons`], building them from a chassis
//! (`/datum/component/construction/mecha`) in [`construction`] and the exosuit fabricator
//! (`/obj/machinery/mecha_part_fabricator`, tgui `ExosuitFabricator`) in [`fab`]. The pilot's
//! `Mecha` window, action buttons, lights and holodiagnostic are in [`panel`], the medical
//! equipment (sleeper, syringe gun, medical beamgun) in [`medical`], the tracking beacons and
//! the exosuit control console (tgui `ExosuitControlConsole`) in [`control`]. [`client`] draws
//! the windows; this module holds their data, which both halves share.
//!
//! The mech, equipment, construction and fabricator data come from `ss13-mapload convert`
//! (`<assets>/mecha.ron`, [`ss13_mapload::mecha`]); rerun it after updating.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::vending::ProductIcon;

use crate::science::MaterialInfo;

#[cfg(feature = "server")]
pub mod bay;
#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod construction;
#[cfg(feature = "server")]
pub mod control;
#[cfg(feature = "server")]
pub mod core;
#[cfg(feature = "server")]
pub mod equipment;
#[cfg(feature = "server")]
pub mod fab;
#[cfg(feature = "server")]
pub mod medical;
#[cfg(feature = "server")]
pub mod panel;
#[cfg(feature = "server")]
pub mod weapons;

#[cfg(feature = "server")]
pub use core::{Mech, MechaDb, Pilot};

/// Exosuits on the server.
#[cfg(feature = "server")]
pub struct MechaServerPlugin;

#[cfg(feature = "server")]
impl bevy::prelude::Plugin for MechaServerPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // tg `item_interaction`/`attackby` of exosuit things: the fabricator, mechs, chassis,
        // wreckages.
        app.add_click_handler(ClickPhase::HeldOnTarget, 2900, |w, c| {
            c.held.is_some_and(|h| {
                crate::mecha::item_interaction(w, c.client, c.mob, h, c.target, c.modifiers)
            })
        });
        app.add_plugins((
            core::MechCorePlugin,
            fab::FabPlugin,
            bay::MechBayPlugin,
            weapons::MechWeaponsPlugin,
            panel::MechPanelPlugin,
            medical::MechMedicalPlugin,
            control::MechControlPlugin,
        ));
    }
}

/// tgui `ExosuitFabricator`.
pub struct ExosuitFabricatorUi;

impl UiInterface for ExosuitFabricatorUi {
    const NAME: &'static str = "ExosuitFabricator";
    type Data = ExoFabData;
    type StaticData = ExoFabStatic;
}

/// A design in the fabricator's `designs`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ExoDesign {
    /// The design type: the `build` act's `designs` entries.
    pub path: String,
    pub name: String,
    pub desc: String,
    /// Material index -> units, with the fabricator's `component_coeff`.
    pub cost: Vec<(u8, u32)>,
    pub categories: Vec<String>,
    pub icon: Option<ProductIcon>,
    /// `constructionTime`, deciseconds with the fabricator's `time_coeff`.
    pub construction_time: f32,
}

/// `ui_static_data`.
// Not tg: tg resends the static data when the techweb gains designs; here it lists every
// fabricator design once and `ui_data` says which are researched (as the Fabricator window).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ExoFabStatic {
    pub fab_name: String,
    pub materials: Vec<MaterialInfo>,
    pub designs: Vec<ExoDesign>,
    pub sheet_amount: f64,
}

/// A `queue` entry of `ui_data`: the part being built first, then the waiting ones.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ExoQueueEntry {
    pub job_id: u32,
    /// Index into [`ExoFabStatic::designs`].
    pub design: u16,
    pub processing: bool,
    /// `timeLeft`: deciseconds for the part being built, seconds for the waiting ones (as tg).
    pub time_left: f32,
}

/// `ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ExoFabData {
    /// Units per material index.
    pub materials: Vec<u32>,
    pub queue: Vec<ExoQueueEntry>,
    /// `process_queue`.
    pub processing: bool,
    /// Indices into [`ExoFabStatic::designs`] the techweb has researched (tg `cached_designs`).
    pub researched: Vec<u16>,
}

impl ExoFabData {
    /// The recipe's `canPrint`: every material in stock.
    pub fn can_print(&self, design: &ExoDesign) -> bool {
        design.cost.iter().all(|&(m, n)| {
            self.materials
                .get(usize::from(m))
                .is_some_and(|&have| have > 0 && n <= have)
        })
    }
}

pub const BUILD: &str = "build";
pub const DESIGNS: &str = "designs";
pub const NOW: &str = "now";
pub const DEL_QUEUE_PART: &str = "del_queue_part";
pub const INDEX: &str = "index";
pub const CLEAR_QUEUE: &str = "clear_queue";
pub const BUILD_QUEUE: &str = "build_queue";
pub const STOP_QUEUE: &str = "stop_queue";
pub use crate::science::{AMOUNT, MATERIAL_REF, REMOVE_MAT};

/// tgui `Mecha` (`tgui/packages/tgui/interfaces/Mecha`): the pilot's window.
pub struct MechaUi;

impl UiInterface for MechaUi {
    const NAME: &'static str = "Mecha";
    type Data = MechaData;
    type StaticData = MechaStatic;
}

/// `ui_static_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MechaStatic {
    pub ui_theme: String,
}

/// A reagent line of the equipment data (`get_reagent_data`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GearReagent {
    pub name: String,
    pub volume: f32,
}

/// The sleeper's patient (`get_snowflake_data`, `MECHA_SNOWFLAKE_ID_SLEEPER`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SleeperPatient {
    pub patient_name: String,
    /// `health / maxHealth`.
    pub patient_health: f32,
    pub patient_state: String,
    /// Celsius, humans only.
    pub core_temp: Option<f32>,
    pub brute_loss: f32,
    pub burn_loss: f32,
    pub toxin_loss: f32,
    pub oxygen_loss: f32,
    pub contained_reagents: Vec<GearReagent>,
    pub has_brain_damage: bool,
    pub has_traumas: bool,
}

/// A sleeper's section.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SleeperGear {
    pub patient: Option<SleeperPatient>,
    /// The medical sleeper: the syringe gun's reagents it can inject (`injectible_reagents`).
    pub injectible_reagents: Option<Vec<GearReagent>>,
}

/// A syringe gun's section (`MECHA_SNOWFLAKE_ID_SYRINGE`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SyringeGear {
    /// `"Launch"` or `"Analyze"`.
    pub mode: String,
    pub syringe: u32,
    pub max_syringe: u32,
    pub reagents: f32,
    pub total_reagents: f32,
    /// Known reagents and whether each is being synthesized.
    pub analyzed_reagents: Vec<(String, bool)>,
    pub contained_reagents: Vec<GearReagent>,
}

/// One piece of equipment.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MechaGear {
    pub name: String,
    /// `"Left arm"`, `"Right arm"` or the category.
    pub slot: String,
    /// The equipment entity's bits (tg `REF(module)`).
    pub reference: u64,
    pub sleeper: Option<SleeperGear>,
    pub syringe: Option<SyringeGear>,
}

/// `ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MechaData {
    pub name: String,
    pub integrity: f32,
    pub integrity_max: f32,
    /// Joules; `None` without a cell.
    pub power_level: Option<f64>,
    pub power_max: Option<f64>,
    pub weapons_safety: bool,
    pub id_lock: bool,
    pub has_lights: bool,
    pub lights_on: bool,
    /// The holodiagnostic was made (`TRAIT_MECHA_DIAGNOSTIC_CREATED`).
    pub diagnostic_status: bool,
    pub equipment: Vec<MechaGear>,
}

pub const DIAGNOSTIC: &str = "diagnostic";
pub const TOGGLE_SAFETY: &str = "toggle_safety";
pub const TOGGLE_ID_LOCK: &str = "toggle_id_lock";
pub const TOGGLE_LIGHTS: &str = "toggle_lights";
pub const EQUIP_ACT: &str = "equip_act";
pub const REF: &str = "ref";
pub const GEAR_ACTION: &str = "gear_action";

/// tgui `ExosuitControlConsole` (`ExosuitControlConsole.tsx`).
pub struct ExosuitControlConsoleUi;

impl UiInterface for ExosuitControlConsoleUi {
    const NAME: &'static str = "ExosuitControlConsole";
    type Data = ExoControlData;
    type StaticData = ();
}

/// A `mechs` entry: a mech with a tracking beacon.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TrackedMech {
    pub name: String,
    /// Percent.
    pub integrity: i32,
    /// Percent; `None` without a cell.
    pub charge: Option<i32>,
    /// kPa; `None` for an open cockpit.
    pub airtank: Option<f32>,
    pub pilot: Vec<String>,
    pub location: Option<String>,
    pub emp_recharging: bool,
    /// The beacon entity's bits.
    pub tracker_ref: u64,
    /// Percent, Ripleys only.
    pub cargo_space: Option<i32>,
}

/// `ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ExoControlData {
    pub mechs: Vec<TrackedMech>,
}

pub const SEND_MESSAGE: &str = "send_message";
pub const SHOCK: &str = "shock";
pub const TRACKER_REF: &str = "tracker_ref";

/// tg `item_interaction`/`attackby` of exosuit things with `held`: sheets into a fabricator,
/// equipment onto a mech (a right click: the right arm), an ID toggling its ID lock, other items
/// hitting it; parts, boards and tools on a chassis; a welder or crowbar on a wreckage. True
/// when the click was theirs.
#[cfg(feature = "server")]
pub fn item_interaction(
    world: &mut bevy::prelude::World,
    client: bevy::prelude::Entity,
    mob: bevy::prelude::Entity,
    held: bevy::prelude::Entity,
    target: bevy::prelude::Entity,
    modifiers: ss13_shared::protocol::ClickModifiers,
) -> bool {
    use ss13_shared::chat::Span;

    if fab::item_interaction(world, client, mob, held, target) {
        return true;
    }
    let actor = crate::tools::Actor {
        client,
        mob,
        item: Some(held),
    };
    if world.get::<Mech>(target).is_some() {
        if control::is_tracker(world, held) {
            control::try_attach(world, actor, held, target);
            return true;
        }
        // `ammo_resupply`; an unusable box falls through to hitting the mech.
        if weapons::is_ammo(world, held) && weapons::ammo_resupply(world, actor, held, target) {
            return true;
        }
        if equipment::is_equipment(world, held) {
            return equipment::try_attach_part(world, actor, held, target, modifiers.right);
        }
        if crate::inventory::get_id(world, held).is_some() {
            let allowed = {
                let m = world.get::<Mech>(target).expect("checked above");
                let def = &world.resource::<MechaDb>().data.mechs[m.def];
                let access = crate::inventory::get_id(world, held)
                    .and_then(|c| world.get::<crate::inventory::IdCard>(c))
                    .map(|c| c.access.clone())
                    .unwrap_or_default();
                crate::inventory::check_access(&[], &def.accesses, &access)
            };
            let mut m = world.get_mut::<Mech>(target).expect("checked above");
            let text = if !allowed {
                if m.flags & core::ID_LOCK_ON != 0 {
                    "в доступе отказано!"
                } else {
                    "unable to set id lock!"
                }
            } else {
                m.flags ^= core::ID_LOCK_ON;
                if m.flags & core::ID_LOCK_ON != 0 {
                    "enabled id lock!"
                } else {
                    "disabled id lock!"
                }
            };
            crate::tools::tell(world, client, Span::Notice, text);
            return true;
        }
        return core::attacked_by(world, actor, target);
    }
    if construction::attackby(world, actor, target) {
        return true;
    }
    if let Some((kind, _)) = crate::tools::tool_of(world, held)
        && core::wreckage_tool(world, actor, kind, target)
    {
        return true;
    }
    false
}
