//! Exosuits, after tg `code/modules/vehicles/mecha/`: the mechs themselves
//! (`/obj/vehicle/sealed/mecha`: entering, driving, the pilot's clicks, damage, wreckage) in
//! [`core`], their equipment (`/obj/item/mecha_parts/mecha_equipment`: the hydraulic clamp, the
//! drill, the cargo compartment) in [`equipment`], the weapons and their ammo boxes in
//! [`weapons`], building them from a chassis
//! (`/datum/component/construction/mecha`) in [`construction`] and the exosuit fabricator
//! (`/obj/machinery/mecha_part_fabricator`, tgui `ExosuitFabricator`) in [`fab`]. [`client`]
//! draws the fabricator's window; this module holds its data, which both halves share.
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
pub mod core;
#[cfg(feature = "server")]
pub mod equipment;
#[cfg(feature = "server")]
pub mod fab;
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
