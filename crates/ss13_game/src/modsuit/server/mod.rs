//! The MOD control unit (server): tg `mod_control.dm`, `mod_activation.dm`, `mod_part.dm`,
//! `mod_core.dm` (standard and infinite cores), `mod_ui.dm`, `mod_actions.dm`, `mod_types.dm`.
//!
//! - A control unit ([`ModSuit`]) is set up from its type when it appears (tg `Initialize` ->
//!   `theme.set_up_parts`, `pre_equipped` core, cell and modules): its parts are items of their
//!   own, owned through [`InSuit`] and kept nowhere while retracted; each gets the theme's armor,
//!   temperature limits, siemens coefficient, skin icon states and unsealed clothing flags.
//! - Worn in its theme's slot it has a wearer: parts deploy into their slots (over gloves and
//!   shoes that `CAN_BE_OVERSLOT`, which come back on retraction) and stick there; the unit
//!   cannot be taken off while active or with parts out. Losing the wearer retracts and
//!   unseals everything at once (`clean_up`).
//! - Activation seals each deployed part, then the unit, one `activation_step_time` apart
//!   (tg's do_afters with `MOD_ACTIVATION_STEP_FLAGS`: only losing the wearer stops them);
//!   deactivation runs the other way. A sealed part gets its `visor_flags` (pressure sealing,
//!   internals) and its heat and cold protection back; modules whose required parts are all
//!   sealed get `on_part_activation`.
//! - While active the core pays `charge_drain` watts and each module its idle or active cost;
//!   an empty core shuts the suit down. The parts slow the wearer by the theme's
//!   `slowdown_deployed` plus active module slowdowns, split over all parts ([`SuitSlowdown`]).
//! - Maintenance: a screwdriver (1 s) opens the cover, a crowbar removes a module (list
//!   prompt), a wrench (3 s) the core, an empty hand (1.5 s) the core's cell; modules, cores,
//!   cells and IDs (the access lock) go in with the cover open; an emag flips the lock.
//! - The action buttons (deploy, activate with confirmation, panel, module, pinned modules),
//!   tgui `MODsuit` and `emp_act` (burns an active wearer, drops the selected module).
//! - Cores: standard (a cell inside), infinite, plasma (its own charge, refuelled by plasma ore
//!   and sheets used on it or on the unit) and ethereal (no charge: ethereals have no charge
//!   here).
//! - The parts hide what their skin's `UNSEALED_INVISIBILITY`/`SEALED_INVISIBILITY` and cover
//!   what `UNSEALED_COVER`/`SEALED_COVER` say (`flags_inv`, `flags_cover`).
// shortcut: no wires, malfunctions, electrification, MODlink calls, AI/pAI assistants, paint
// kits, ethereal charge (the ethereal core reads "Biological Battery Missing"), the lavaland
// core's light and pollen, speed potions, strip actions, the spacesuit HUD charge icon or
// hearing protection; a radial menu is a list prompt. Add them with their mechanics.

use std::collections::VecDeque;

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::modsuit::{ModData, PartDef, SuitDef, ThemeDef};
use ss13_mapload::{DmClothing, DmItem, DmName, MapLoadPlugin};
use ss13_net::server::Networked;
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::protocol::InventorySlot;
use ss13_shared::{Appearance, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::UiStatus;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems, ask_world};
use ss13_tgui::server::{
    AlwaysState, AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};

use super::modules::{self, ModModule};
use super::*;
use crate::actions::{ActionButton, ActionButtons, ActionButtonsUi, set_buttons};
use crate::emp::{EmpAct, EmpProtection, EmpSystems, PROTECT_ALL, emp_acts};
use crate::health::{DamageType, Health, Stat};
use crate::inventory::{ALL_SLOTS, CarriedBy, IdCard, Inventory, NoDrop, slot_bit};
use crate::mecha::core::ItemCell;
use crate::server::PlayerMob;
use crate::sound::{PlaySound, SHORT_RANGE_SOUND_EXTRARANGE, playsound};
use crate::storage::StoredIn;
use crate::tools::now;
pub(crate) use crate::tools::tell_mob;

mod deploy;
mod init;
mod interact;
mod parts;
#[cfg(test)]
mod tests;
mod ui;

use crate::tools::{ToolKind, tell, tool_of};
use deploy::*;
pub use deploy::{
    control_activation, deploy, quick_deploy, retract, seal_part, tick, toggle_activate,
};
pub(crate) use init::theme_of;
use init::*;
pub use init::{init_core, init_suit, slot_of_flags, update_icons};
use interact::*;
pub(crate) use interact::{SHIELD_PROTECTION, set_emp_protection};
pub use interact::{charge_plasma, hand_interaction, item_interaction, plasma_uses};
use parts::*;
pub use parts::{
    allowed, charge, check_charge, install, max_charge, subtract_charge, uninstall, worn_suit,
};
pub(crate) use parts::{buzz, name, sound_at};
use ui::*;
pub use ui::{open_panel, pin, refresh_buttons};

/// `SILENCED_SOUND_EXTRARANGE`.
const SILENCED_SOUND_EXTRARANGE: f32 = -11.0;
const SCANBUZZ: &str = "sound/machines/scanner/scanbuzz.ogg";
const MECHMOVE: &str = "sound/vehicles/mecha/mechmove03.ogg";
const SYNTH_YES: &str = "sound/machines/synth/synth_yes.ogg";
const SYNTH_NO: &str = "sound/machines/synth/synth_no.ogg";
const CLICK: &str = "sound/machines/click.ogg";
/// The default MOD icon files (`set_skin` without `MOD_ICON_OVERRIDE`).
pub const MOD_ICON: &str = "icons/obj/clothing/modsuit/mod_clothing.dmi";
pub const MOD_WORN_ICON: &str = "icons/mob/clothing/modsuit/mod_clothing.dmi";
const ACTIONS_DMI: &str = "icons/mob/actions/actions_mod.dmi";
const BACKGROUNDS_DMI: &str = "icons/mob/actions/backgrounds.dmi";
/// `CAN_BE_OVERSLOT` (`item_flags`).
pub const CAN_BE_OVERSLOT: u32 = 1 << 19;
/// `screwdriver_act`, `wrench_act`, `mod_uninstall_cell` delays, seconds.
const COVER_SECS: f64 = 1.0;
const CORE_SECS: f64 = 3.0;
const CELL_SECS: f64 = 1.5;
/// `/datum/action/item_action/mod/activate` `reset_ready` timer, seconds.
const READY_SECS: f64 = 3.0;
/// Action button ids: `mod:<control bits>:<what>`.
const BUTTON: &str = "mod:";
const INFINITE_CORE: &str = "/obj/item/mod/core/infinite";
const STANDARD_CORE: &str = "/obj/item/mod/core/standard";
const ETHEREAL_CORE: &str = "/obj/item/mod/core/ethereal";
const PLASMA_CORE: &str = "/obj/item/mod/core/plasma";
const LAVALAND_CORE: &str = "/obj/item/mod/core/plasma/lavaland";
/// `STANDARD_CELL_CHARGE`, joules.
const STANDARD_CELL_CHARGE: f64 = 10_000.0;
/// `charger_list`: `PLASMA_CORE_ORE_CHARGE`, `PLASMA_CORE_SHEET_CHARGE` (subtypes count).
const PLASMA_CHARGERS: [(&str, f64); 2] = [
    ("/obj/item/stack/ore/plasma", 1.5 * STANDARD_CELL_CHARGE),
    (
        "/obj/item/stack/sheet/mineral/plasma",
        2.0 * STANDARD_CELL_CHARGE,
    ),
];

/// The MODsuit data from `<assets>/modsuits.ron`, with lookups by type.
#[derive(Resource, Default)]
pub struct ModDb {
    pub data: ModData,
}

impl ModDb {
    pub fn new(data: ModData) -> Self {
        Self { data }
    }

    pub fn theme(&self, path: &str) -> Option<usize> {
        self.data.themes.iter().position(|t| t.path == path)
    }

    pub fn suit(&self, path: &str) -> Option<usize> {
        self.data.suits.iter().position(|s| s.path == path)
    }

    pub fn module(&self, path: &str) -> Option<usize> {
        self.data.modules.iter().position(|m| m.path == path)
    }
}

/// On a part, module, core or overslotted item: the control unit that owns it. Despawning the
/// unit despawns them (tg deletes its parts, modules and core with it).
#[derive(Component, Debug)]
#[relationship(relationship_target = SuitContents)]
pub struct InSuit(pub Entity);

#[derive(Component, Debug, Default)]
#[relationship_target(relationship = InSuit, linked_spawn)]
pub struct SuitContents(Vec<Entity>);

/// tg `/datum/mod_part` with the part's theme flags.
#[derive(Clone, Debug, PartialEq)]
pub struct ModPart {
    pub item: Entity,
    pub slot: InventorySlot,
    pub sealed: bool,
    /// `base_icon_state`: `helmet`, `chestplate`, `gauntlets`, `boots`.
    pub base: String,
    pub def: PartDef,
    /// `initial(heat_protection)`, `initial(cold_protection)`, `initial(flash_protect)`.
    pub heat_protection: u32,
    pub cold_protection: u32,
    pub flash_protect: i32,
    /// `overslotting`: the worn item this part went over.
    pub overslot: Option<Entity>,
}

/// One step of the sealing sequence (tg `delayed_seal_part`, `delayed_activation`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Seals or unseals `parts[part]`, then retracts it when `retract`.
    Seal {
        part: usize,
        seal: bool,
        retract: bool,
    },
    /// `control_activation(is_on)`.
    Control { on: bool },
}

/// A maintenance action waiting for its tool delay (tg `use_tool`/`do_after`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PendingKind {
    Cover,
    RemoveCore,
    RemoveCell,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pending {
    pub kind: PendingKind,
    pub client: Entity,
    pub mob: Entity,
    pub mob_pos: TilePos,
    /// What the mob's active hand held when it started.
    pub held: Option<Entity>,
    pub until: f64,
}

/// The live `/obj/item/mod/control`.
#[derive(Component, Clone, Debug)]
pub struct ModSuit {
    /// Index into [`ModData::themes`].
    pub theme: usize,
    /// Index into [`ModData::suits`].
    pub suit: usize,
    pub skin: String,
    pub parts: Vec<ModPart>,
    pub modules: Vec<Entity>,
    pub core: Option<Entity>,
    pub selected: Option<Entity>,
    pub active: bool,
    pub activating: bool,
    pub open: bool,
    pub locked: bool,
    pub req_access: Vec<String>,
    pub complexity: i32,
    pub wearer: Option<Entity>,
    /// Who carries it (hands or a slot): who has its action buttons.
    pub holder: Option<Entity>,
    pub steps: VecDeque<Step>,
    pub step_at: f64,
    pub pending: Option<Pending>,
    /// `pinned_to`: (user, module).
    pub pins: Vec<(Entity, Entity)>,
    /// `default_pins` already applied: (module, wearer).
    pub pinned_once: Vec<(Entity, Entity)>,
    /// The activate button's first click (`ready`), until this time.
    pub ready_until: f64,
}

impl ModSuit {
    pub fn part_index(&self, item: Entity) -> Option<usize> {
        self.parts.iter().position(|p| p.item == item)
    }

    /// The slot bits of every part (the control unit's own included), sealed ones only with
    /// `need_active` (tg `mod_parts` keys with `part_datum.sealed`).
    pub fn part_slots(&self, theme: &ThemeDef, need_active: bool) -> u32 {
        let mut bits = 0;
        if !need_active || self.active {
            bits |= theme.slot_flags;
        }
        for p in &self.parts {
            if !need_active || p.sealed {
                bits |= slot_bit(p.slot);
            }
        }
        bits
    }
}

/// The equipment slowdown of a wearer's MOD parts, deciseconds per tile (tg
/// `/datum/movespeed_modifier/equipment_speedmod` from the parts' `slowdown`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct SuitSlowdown(pub f32);

/// The MODsuit window's data, rebuilt every tick ([`refresh_views`]).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ModSuitView {
    pub statics: ModSuitStatic,
    pub data: ModSuitData,
    /// (user, module bits) pins.
    pub pins: Vec<(Entity, u64)>,
}

/// A middle click of a wearer whose selected module is used by clicking (tg
/// `on_special_click` on `COMSIG_MOB_MIDDLECLICKON`): `crate::click` sends it instead of
/// cycling the hands.
#[derive(Message, Clone, Copy, Debug)]
pub struct ModuleClick {
    pub client: Entity,
    pub mob: Entity,
    pub target: Entity,
    pub target_pos: TilePos,
}

/// On a wearer with a selected module that clicks (tg `used_signal`).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct ModuleSelected;

/// On a prompt: what the MODsuit asked.
#[derive(Component, Clone, Debug)]
struct ModChoice {
    control: Entity,
    kind: ChoiceKind,
}

#[derive(Clone, Debug)]
enum ChoiceKind {
    /// `choose_deploy`: `None` is every part (the button's right click in tg).
    Deploy(Vec<Option<Entity>>),
    /// `quick_module`.
    Module(Vec<Entity>),
    /// `crowbar_act`'s "Which module to remove?".
    Remove(Vec<Entity>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModAct {
    Lock,
    Activate,
    Select(u64),
    Configure(u64, String, String),
    Pin(u64),
    Deploy(u64),
}

pub struct ModSuitPlugin;

impl Plugin for ModSuitPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // A MODsuit control unit in the other hand: modules, cores, cells, tools and IDs with the
        // active hand, its cell with an empty one (before its storage).
        app.add_click_handler(ClickPhase::OtherHand, 100, |w, c| {
            crate::modsuit::server::hand_interaction(w, c.client, c.mob, c.held, c.target)
        });
        // A MODsuit control unit: emags, modules, cores, cells, tools, IDs.
        app.add_click_handler(ClickPhase::HeldOnTarget, 400, |w, c| {
            c.held.is_some_and(|h| {
                crate::modsuit::server::item_interaction(w, c.client, c.mob, h, c.target)
            })
        });
        // An anomaly core into an anomaly-locked module, a screwdriver taking it out
        // (`crate::modsuit::anomalock`): the module in the other hand or on the floor.
        app.add_click_handler(ClickPhase::OtherHand, 91, |w, c| {
            let actor = crate::tools::Actor {
                client: c.client,
                mob: c.mob,
                item: c.held,
            };
            crate::modsuit::anomalock::item_interaction(w, actor, c.target)
        });
        app.add_click_handler(ClickPhase::HeldOnTarget, 1571, |w, c| {
            let actor = crate::tools::Actor {
                client: c.client,
                mob: c.mob,
                item: c.held,
            };
            crate::modsuit::anomalock::item_interaction(w, actor, c.target)
        });
        app.register_ui::<ModSuitUi>()
            .add_systems(Startup, load_db)
            .add_systems(
                Update,
                (
                    init_new_suits,
                    tick.after(crate::inventory::inventory_intents)
                        .before(UiSystems::Open),
                    window_status.in_set(UiSystems::StatusHook),
                    (
                        suit_acts,
                        prompt_answers.after(PromptSystems::Answer),
                        trigger_buttons
                            .run_if(resource_exists::<Messages<UiActed<ActionButtonsUi>>>),
                    )
                        .in_set(UiSystems::Handle),
                    module_clicks
                        .after(crate::click::ResolveClicks)
                        .run_if(resource_exists::<Messages<ModuleClick>>),
                    suit_emp
                        .in_set(EmpSystems::Effects)
                        .run_if(resource_exists::<Messages<EmpAct>>),
                    megaphone_lines
                        .after(ss13_chat::server::SaySystems)
                        .run_if(resource_exists::<Messages<ss13_chat::server::Spoke>>),
                ),
            );
    }
}

// ---------------------------------------------------------------------------------------------
// Set-up

/// A MOD core's state (tg `/obj/item/mod/core`): the standard core holds a cell, the infinite
/// one never runs out, the plasma one keeps its own charge, the ethereal one has none here.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct ModCore {
    pub infinite: bool,
    pub standard: bool,
    /// `cell`, [`StoredIn`] the core.
    pub cell: Option<Entity>,
    pub ethereal: bool,
    /// The plasma core's `charge` and `maxcharge`, joules.
    pub plasma: Option<(f64, f64)>,
}

// ---------------------------------------------------------------------------------------------
// Helpers

// ---------------------------------------------------------------------------------------------
// Modules

// ---------------------------------------------------------------------------------------------
// Wearer, deploying, sealing

// ---------------------------------------------------------------------------------------------
// The tick

// ---------------------------------------------------------------------------------------------
// The window

impl ServerUi for ModSuitUi {
    type Source = &'static ModSuitView;
    type UserParam = ();
    type Act = ModAct;

    fn static_data(view: &ModSuitView) -> ModSuitStatic {
        view.statics.clone()
    }

    fn data(view: &ModSuitView, user: Entity, _: &()) -> ModSuitData {
        let mut data = view.data.clone();
        for m in &mut data.module_info {
            m.pinned = view.pins.contains(&(user, m.reference));
        }
        data
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<ModAct, UiReject> {
        let reference = || {
            params
                .get(REF)
                .and_then(|r| r.trim().parse::<u64>().ok())
                .ok_or_else(|| UiReject::BadParams("ref: expected an entity".into()))
        };
        match action {
            LOCK => Ok(ModAct::Lock),
            ACTIVATE => Ok(ModAct::Activate),
            SELECT => Ok(ModAct::Select(reference()?)),
            PIN => Ok(ModAct::Pin(reference()?)),
            DEPLOY => Ok(ModAct::Deploy(reference()?)),
            CONFIGURE => Ok(ModAct::Configure(
                reference()?,
                params.text(KEY)?,
                params.text(VALUE)?,
            )),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Action buttons and prompts

// ---------------------------------------------------------------------------------------------
// Item interactions

// ---------------------------------------------------------------------------------------------
// Clicks with a selected module, EMP
