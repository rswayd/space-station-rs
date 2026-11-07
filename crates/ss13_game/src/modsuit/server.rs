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
use crate::inventory::{ALL_SLOTS, CarriedBy, IdCard, Inventory, NoDrop, check_access, slot_bit};
use crate::mecha::core::ItemCell;
use crate::server::PlayerMob;
use crate::sound::{PlaySound, SHORT_RANGE_SOUND_EXTRARANGE, playsound};
use crate::storage::StoredIn;
use crate::tools::{ToolKind, tell, tool_of};

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

/// `megaphone/handle_speech`'s power: every spoken line of a megaphone module's wearer.
fn megaphone_lines(world: &mut World, mut cursor: Local<MessageCursor<ss13_chat::server::Spoke>>) {
    let speakers: Vec<Entity> = cursor
        .read(world.resource::<Messages<ss13_chat::server::Spoke>>())
        .map(|s| s.speaker)
        .collect();
    for speaker in speakers {
        if world.get_entity(speaker).is_ok() {
            modules::megaphone_spoke(world, speaker);
        }
    }
}

/// Reads `<assets>/modsuits.ron` unless a test inserted [`ModDb`].
fn load_db(world: &mut World) {
    if world.contains_resource::<ModDb>() {
        return;
    }
    let path = world
        .get_resource::<MapLoadPlugin>()
        .map(|m| m.assets.join(ss13_mapload::modsuit::MODSUIT_FILE));
    let data = match path.as_deref().map(ModData::read) {
        Some(Ok(data)) => data,
        Some(Err(e)) => {
            warn!("{e:#}; no MODsuits (rerun `ss13-mapload convert`)");
            ModData::default()
        }
        None => ModData::default(),
    };
    info!(
        "modsuits: {} themes, {} control units, {} modules",
        data.themes.len(),
        data.suits.len(),
        data.modules.len()
    );
    world.insert_resource(ModDb::new(data));
}

/// Control units that appeared (mapped, printed, from crates) are set up.
fn init_new_suits(
    added: Query<(Entity, &DmItem), (Added<DmItem>, Without<ModSuit>)>,
    mut commands: Commands,
) {
    for (e, item) in &added {
        if is_under(&item.path, ss13_mapload::modsuit::CONTROL_ROOT) {
            commands.queue(move |world: &mut World| init_suit(world, e));
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Set-up

fn base_of(part_path: &str) -> &'static str {
    match part_path {
        p if is_under(p, "/obj/item/clothing/head") => "helmet",
        p if is_under(p, "/obj/item/clothing/suit") => "chestplate",
        p if is_under(p, "/obj/item/clothing/gloves") => "gauntlets",
        p if is_under(p, "/obj/item/clothing/shoes") => "boots",
        p if is_under(p, "/obj/item/clothing/glasses") => "glasses",
        _ => "neck",
    }
}

/// The inventory slot of `ITEM_SLOT_*` bits (one slot).
pub fn slot_of_flags(flags: u32) -> Option<InventorySlot> {
    ALL_SLOTS[2..]
        .iter()
        .copied()
        .find(|&s| slot_bit(s) & flags != 0)
}

/// tg `/obj/item/mod/control/Initialize` (with `pre_equipped`'s core, cell and modules).
pub fn init_suit(world: &mut World, control: Entity) {
    if world.get_entity(control).is_err() || world.get::<ModSuit>(control).is_some() {
        return;
    }
    let Some(path) = world.get::<DmItem>(control).map(|i| i.path.clone()) else {
        return;
    };
    let Some(db) = world.get_resource::<ModDb>() else {
        return;
    };
    let Some(suit_i) = db.suit(&path) else {
        return;
    };
    let def: SuitDef = db.data.suits[suit_i].clone();
    let Some(theme_i) = db.theme(&def.theme) else {
        warn!("{path}: no MOD theme {}", def.theme);
        return;
    };
    let theme: ThemeDef = db.data.themes[theme_i].clone();
    let skin_name = if def.skin.is_empty() {
        theme.default_skin.clone()
    } else {
        def.skin.clone()
    };
    let skin = theme
        .skins
        .iter()
        .find(|s| s.name == skin_name)
        .or_else(|| theme.skins.first())
        .cloned()
        .unwrap_or_default();
    let mut parts = Vec::new();
    for part_def in &skin.parts {
        let Some(item) = crate::construction::spawn_detached(world, &part_def.path) else {
            warn!("{path}: no prototype for MOD part {}", part_def.path);
            continue;
        };
        world.entity_mut(item).insert(InSuit(control));
        let Some(slot) = world
            .get::<DmItem>(item)
            .and_then(|i| slot_of_flags(i.slot_flags))
        else {
            continue;
        };
        let clothing = world.get::<DmClothing>(item).cloned().unwrap_or_default();
        parts.push(ModPart {
            item,
            slot,
            sealed: false,
            base: base_of(&part_def.path).to_owned(),
            def: part_def.clone(),
            heat_protection: clothing.heat_protection,
            cold_protection: clothing.cold_protection,
            flash_protect: clothing.flash_protect,
            overslot: None,
        });
    }
    // `set_up_parts`: names, armor, temperature, siemens; `set_skin`: the unsealed flags.
    for part in &parts {
        if let Some(mut c) = world.get_mut::<DmClothing>(part.item) {
            c.clothing_flags = part.def.unsealed_clothing;
            c.flags_inv = part.def.unsealed_inv;
            c.flags_cover = part.def.unsealed_cover;
            c.visor_flags = part.def.sealed_clothing;
            c.visor_flags_inv = part.def.sealed_inv;
            c.visor_flags_cover = part.def.sealed_cover;
            c.heat_protection = 0;
            c.cold_protection = 0;
            c.max_heat_protection_temperature = theme.max_heat_protection_temperature;
            c.min_cold_protection_temperature = theme.min_cold_protection_temperature;
            c.siemens_coefficient = theme.siemens_coefficient;
            if part.slot == InventorySlot::Suit {
                for allowed in &theme.allowed_suit_storage {
                    if !c.allowed.contains(allowed) {
                        c.allowed.push(allowed.clone());
                    }
                }
            }
        }
    }
    for e in parts.iter().map(|p| p.item).chain([control]) {
        world.entity_mut(e).insert(theme.armor);
        if let Some(mut name) = world.get_mut::<DmName>(e) {
            name.name = format!("{} {}", theme.name, name.name);
            name.desc = format!("{} {}", name.desc, theme.desc);
        }
    }
    // `mod.slot_flags = slot_flags`: some themes go on the belt.
    if let Some(mut item) = world.get_mut::<DmItem>(control) {
        item.slot_flags = theme.slot_flags;
    }
    let req_access = def.req_access.clone();
    world.entity_mut(control).insert((
        ModSuit {
            theme: theme_i,
            suit: suit_i,
            skin: skin.name.clone(),
            parts,
            modules: Vec::new(),
            core: None,
            selected: None,
            active: false,
            activating: false,
            open: false,
            locked: !req_access.is_empty(),
            req_access,
            complexity: 0,
            wearer: None,
            holder: None,
            steps: VecDeque::new(),
            step_at: 0.0,
            pending: None,
            pins: Vec::new(),
            pinned_once: Vec::new(),
            ready_until: 0.0,
        },
        ModSuitView::default(),
        AlwaysState,
    ));
    update_icons(world, control);
    if !def.core.is_empty()
        && let Some(core) = crate::construction::spawn_detached(world, &def.core)
    {
        init_core(world, core);
        if !def.cell.is_empty()
            && world
                .get::<ModCore>(core)
                .is_some_and(|c| !c.infinite && c.standard)
            && let Some(cell) = crate::construction::spawn_detached(world, &def.cell)
        {
            crate::storage::put_inside(world, cell, core);
            if let Some(mut c) = world.get_mut::<ModCore>(core) {
                c.cell = Some(cell);
            }
        }
        world.entity_mut(core).insert(InSuit(control));
        if let Some(mut suit) = world.get_mut::<ModSuit>(control) {
            suit.core = Some(core);
        }
    }
    for module_path in theme.inbuilt_modules.iter().chain(&def.modules) {
        let Some(module) = crate::construction::spawn_detached(world, module_path) else {
            continue;
        };
        modules::init_module(world, module);
        if !install(world, control, module, None) {
            world.despawn(module);
        }
    }
}

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

/// Gives a core item its [`ModCore`] (idempotent).
pub fn init_core(world: &mut World, core: Entity) {
    if world.get::<ModCore>(core).is_some() {
        return;
    }
    let Some(path) = world.get::<DmItem>(core).map(|i| i.path.clone()) else {
        return;
    };
    if !is_under(&path, ss13_mapload::modsuit::CORE_ROOT) {
        return;
    }
    // `maxcharge = charge = 10 (lavaland: 15) * STANDARD_CELL_CHARGE`.
    let plasma = is_under(&path, PLASMA_CORE).then(|| {
        let max = if is_under(&path, LAVALAND_CORE) {
            15.0
        } else {
            10.0
        } * STANDARD_CELL_CHARGE;
        (max, max)
    });
    world.entity_mut(core).insert(ModCore {
        infinite: is_under(&path, INFINITE_CORE),
        standard: is_under(&path, STANDARD_CORE),
        cell: None,
        ethereal: is_under(&path, ETHEREAL_CORE),
        plasma,
    });
}

/// `set_skin`/`seal_part`/`update_icon_state`: `[skin]-[base](-sealed)` for the item icon and
/// the worn sprite of the unit and every part.
pub fn update_icons(world: &mut World, control: Entity) {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return;
    };
    let skin = world
        .resource::<ModDb>()
        .data
        .themes
        .get(suit.theme)
        .and_then(|t| t.skins.iter().find(|s| s.name == suit.skin))
        .cloned()
        .unwrap_or_default();
    let icon = if skin.icon.is_empty() {
        MOD_ICON.to_owned()
    } else {
        skin.icon.clone()
    };
    let worn = if skin.worn_icon.is_empty() {
        MOD_WORN_ICON.to_owned()
    } else {
        skin.worn_icon.clone()
    };
    let mut targets: Vec<(Entity, String)> = suit
        .parts
        .iter()
        .map(|p| {
            let sealed = if p.sealed { "-sealed" } else { "" };
            (p.item, format!("{}-{}{sealed}", suit.skin, p.base))
        })
        .collect();
    let sealed = if suit.active { "-sealed" } else { "" };
    targets.push((control, format!("{}-control{sealed}", suit.skin)));
    for (e, state) in targets {
        let id = world
            .get_resource::<IconIndex>()
            .and_then(|i| i.get(&icon, &state));
        if let Some(mut item) = world.get_mut::<DmItem>(e) {
            item.worn_icon = worn.clone();
            item.worn_icon_state = state.clone();
        }
        if let (Some(id), Some(mut look)) = (id, world.get_mut::<Appearance>(e))
            && look.icon_state != id
        {
            look.icon_state = id;
        }
    }
    if let Some(wearer) = suit.wearer.or(suit.holder) {
        crate::inventory::refresh_look(world, wearer);
    }
}

// ---------------------------------------------------------------------------------------------
// Helpers

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

pub(crate) fn theme_of(world: &World, suit: &ModSuit) -> ThemeDef {
    world.resource::<ModDb>().data.themes[suit.theme].clone()
}

pub(crate) fn tell_mob(world: &mut World, mob: Entity, span: Span, text: impl Into<String>) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        tell(world, client, span, text);
    }
}

fn tell_opt(world: &mut World, mob: Option<Entity>, span: Span, text: impl Into<String>) {
    if let Some(mob) = mob {
        tell_mob(world, mob, span, text);
    }
}

pub(crate) fn sound_at(world: &mut World, e: Entity, sound: &str, vol: f32, extrarange: f32) {
    if let Some(pos) = crate::chem::map_pos(world, e) {
        playsound(
            world,
            PlaySound::new(pos, sound, vol, true).extrarange(extrarange),
        );
    }
}

/// The scanner buzz of every refusal.
pub(crate) fn buzz(world: &mut World, control: Entity) {
    sound_at(world, control, SCANBUZZ, 25.0, SILENCED_SOUND_EXTRARANGE);
}

fn refuse(world: &mut World, control: Entity, user: Option<Entity>, text: &str) {
    tell_opt(world, user, Span::Warning, text);
    buzz(world, control);
}

pub(crate) fn name(world: &World, e: Entity) -> String {
    crate::security::name_of(world, e)
}

/// tg `allowed(user)`: the suit's `req_access` against the access of the ID tg's
/// `get_idcard(hand_first = TRUE)` finds.
pub fn allowed(world: &World, control: Entity, user: Entity) -> bool {
    let Some(suit) = world.get::<ModSuit>(control) else {
        return false;
    };
    let access = world
        .get::<Inventory>(user)
        .and_then(|i| i.id_card(world))
        .and_then(|c| world.get::<IdCard>(c))
        .map(|c| c.access.clone())
        .unwrap_or_default();
    check_access(&suit.req_access, &[], &access)
}

/// The core's cell, if still inside it.
fn cell_of(world: &World, control: Entity) -> Option<Entity> {
    let core = world.get::<ModSuit>(control)?.core?;
    let cell = world.get::<ModCore>(core)?.cell?;
    world
        .get::<StoredIn>(cell)
        .is_some_and(|s| s.0 == core)
        .then_some(cell)
}

fn core_state(world: &World, control: Entity) -> Option<ModCore> {
    let core = world.get::<ModSuit>(control)?.core?;
    world.get::<ModCore>(core).copied()
}

/// tg `get_charge()`.
pub fn charge(world: &World, control: Entity) -> f64 {
    match core_state(world, control) {
        Some(c) if c.infinite => f64::INFINITY,
        Some(ModCore {
            plasma: Some((charge, _)),
            ..
        }) => charge,
        Some(_) => cell_of(world, control)
            .and_then(|c| world.get::<ItemCell>(c))
            .map_or(0.0, |c| c.0.charge),
        None => 0.0,
    }
}

/// tg `get_max_charge()` (1 to avoid dividing by zero).
pub fn max_charge(world: &World, control: Entity) -> f64 {
    match core_state(world, control) {
        Some(c) if c.infinite => f64::INFINITY,
        Some(ModCore {
            plasma: Some((_, max)),
            ..
        }) => max.max(1.0),
        Some(_) => cell_of(world, control)
            .and_then(|c| world.get::<ItemCell>(c))
            .map_or(1.0, |c| c.0.maxcharge),
        None => 1.0,
    }
}

/// tg `check_charge(amount)`.
pub fn check_charge(world: &World, control: Entity, amount: f64) -> bool {
    charge(world, control) >= amount
}

/// tg `subtract_charge(amount)`: the cell's `use(amount, force = TRUE)`.
pub fn subtract_charge(world: &mut World, control: Entity, amount: f64) -> bool {
    match core_state(world, control) {
        Some(c) if c.infinite => true,
        Some(ModCore {
            plasma: Some((charge, _)),
            ..
        }) => {
            // `amount = min(amount, charge); charge -= amount`.
            let taken = amount.max(0.0).min(charge);
            if let Some(core) = world.get::<ModSuit>(control).and_then(|s| s.core)
                && let Some(mut c) = world.get_mut::<ModCore>(core)
                && let Some((now, _)) = c.plasma.as_mut()
            {
                *now -= taken;
            }
            taken > 0.0
        }
        Some(_) => {
            let Some(cell) = cell_of(world, control) else {
                return false;
            };
            world
                .get_mut::<ItemCell>(cell)
                .is_some_and(|mut c| c.0.take(amount, true) > 0.0)
        }
        None => false,
    }
}

fn chargebar(world: &World, control: Entity) -> (String, String) {
    let Some(core) = core_state(world, control) else {
        return ("transparent".into(), "No Core Detected".into());
    };
    if core.infinite {
        return ("teal".into(), "Infinite".into());
    }
    if core.ethereal {
        return ("transparent".into(), "Biological Battery Missing".into());
    }
    if core.plasma.is_none() && (!core.standard || cell_of(world, control).is_none()) {
        return ("transparent".into(), "Power Cell Missing".into());
    }
    let (now, max) = (charge(world, control), max_charge(world, control));
    let ratio = ((now / max) * 100.0).round() / 100.0;
    let color = if core.plasma.is_some() {
        if ratio < 0.33 { "bad" } else { "purple" }
    } else if ratio < 0.33 {
        "bad"
    } else if ratio < 0.66 {
        "average"
    } else {
        "good"
    };
    let text = format!(
        "{} of {} ({}%)",
        display_energy(now),
        display_energy(max),
        (100.0 * now / max).round()
    );
    (color.into(), text)
}

/// Every control unit `mob` carries (hands and slots).
fn carried_suits(world: &World, mob: Entity) -> Vec<Entity> {
    let Some(inventory) = world.get::<Inventory>(mob) else {
        return Vec::new();
    };
    ALL_SLOTS
        .iter()
        .filter_map(|&s| inventory.slot(s))
        .filter(|&e| world.get::<ModSuit>(e).is_some())
        .collect()
}

/// The unit `mob` wears.
pub fn worn_suit(world: &World, mob: Entity) -> Option<Entity> {
    carried_suits(world, mob).into_iter().find(|&e| {
        world
            .get::<ModSuit>(e)
            .is_some_and(|s| s.wearer == Some(mob))
    })
}

// ---------------------------------------------------------------------------------------------
// Modules

/// tg `install(new_module, user)`: incompatibility, complexity, required parts. True when it
/// went in.
pub fn install(world: &mut World, control: Entity, module: Entity, user: Option<Entity>) -> bool {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return false;
    };
    let Some(new) = modules::def_of(world, module) else {
        return false;
    };
    let theme = theme_of(world, &suit);
    for &old in &suit.modules {
        let Some(old_def) = modules::def_of(world, old) else {
            continue;
        };
        let clash = new.incompatible.iter().any(|t| is_under(&old_def.path, t))
            || old_def.incompatible.iter().any(|t| is_under(&new.path, t));
        if clash {
            if user.is_some() {
                let text = format!("несовместим с {}!", name(world, old));
                refuse(world, control, user, &text);
            }
            return false;
        }
    }
    if suit.complexity + new.complexity > theme.complexity_max {
        if user.is_some() {
            refuse(world, control, user, "превышена максимальная сложность!");
        }
        return false;
    }
    if !modules::has_required_parts(&new, suit.part_slots(&theme, false)) {
        if user.is_some() {
            refuse(world, control, user, "нет нужных частей!");
        }
        return false;
    }
    // `finish_install`.
    if let Some(u) = user {
        let inventory = world.get::<Inventory>(u).cloned();
        if let Some(slot) = inventory.and_then(|i| i.slot_of(module)) {
            *world
                .get_mut::<Inventory>(u)
                .expect("checked above")
                .slot_mut(slot) = None;
        }
    }
    world
        .entity_mut(module)
        .remove::<(CarriedBy, TilePos, StoredIn)>()
        .insert(InSuit(control));
    if let Some(mut s) = world.get_mut::<ModSuit>(control) {
        s.modules.push(module);
        s.complexity += new.complexity;
    }
    modules::on_install(world, control, module);
    if suit.active && modules::has_required_parts(&new, suit.part_slots(&theme, true)) {
        modules::on_part_activation(world, control, module);
        if let Some(mut m) = world.get_mut::<ModModule>(module) {
            m.part_activated = true;
        }
    }
    if let Some(u) = user {
        let text = format!("{} добавлен", name(world, module));
        tell_mob(world, u, Span::Notice, text);
        sound_at(world, control, CLICK, 50.0, SILENCED_SOUND_EXTRARANGE);
        refresh_buttons(world, suit.holder);
    }
    true
}

/// tg `uninstall(old_module)`: the module is out of the suit, nowhere (the caller places it).
pub fn uninstall(world: &mut World, control: Entity, module: Entity) {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return;
    };
    let complexity = modules::def_of(world, module).map_or(0, |d| d.complexity);
    if let Some(mut s) = world.get_mut::<ModSuit>(control) {
        s.modules.retain(|&m| m != module);
        s.complexity -= complexity;
        s.pins.retain(|&(_, m)| m != module);
        if s.selected == Some(module) {
            s.selected = None;
        }
    }
    if suit.active {
        let (part_activated, active) = world
            .get::<ModModule>(module)
            .map_or((false, false), |m| (m.part_activated, m.active));
        if part_activated {
            modules::on_part_deactivation(world, control, module);
        }
        if active {
            modules::deactivate(world, control, module, true);
        }
    }
    if let Some(mut m) = world.get_mut::<ModModule>(module) {
        m.part_activated = false;
        m.active = false;
    }
    modules::on_uninstall(world, control, module);
    world.entity_mut(module).remove::<InSuit>();
    refresh_buttons(world, suit.holder);
}

/// Where things the suit lets go of land (tg `drop_location()`).
fn drop_pos(world: &World, control: Entity) -> Option<TilePos> {
    crate::chem::map_pos(world, control)
}

/// A detached item back into the world: on `pos`.
fn place_on(world: &mut World, item: Entity, pos: TilePos) {
    world.entity_mut(item).remove::<InSuit>();
    crate::storage::take_out(world, item, pos);
}

// ---------------------------------------------------------------------------------------------
// Wearer, deploying, sealing

/// tg `set_wearer`: modules see it; `pre_equipped` pins its default modules once per wearer.
fn set_wearer(world: &mut World, control: Entity, mob: Entity) {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return;
    };
    let defaults = world.resource::<ModDb>().data.suits[suit.suit]
        .default_pins
        .clone();
    let mut pins = Vec::new();
    for &module in &suit.modules {
        let Some(def) = modules::def_of(world, module) else {
            continue;
        };
        if defaults.contains(&def.path) && !suit.pinned_once.contains(&(module, mob)) {
            pins.push(module);
        }
    }
    if let Some(mut s) = world.get_mut::<ModSuit>(control) {
        s.wearer = Some(mob);
        for &m in &pins {
            s.pinned_once.push((m, mob));
            if !s.pins.contains(&(mob, m)) {
                s.pins.push((mob, m));
            }
        }
    }
}

/// tg `clean_up` + `unset_wearer`: active modules off, parts unsealed and retracted at once,
/// the unit off.
fn clean_up(world: &mut World, control: Entity) {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return;
    };
    if suit.active || suit.activating {
        for &m in &suit.modules {
            if world.get::<ModModule>(m).is_some_and(|m| m.active) {
                modules::deactivate(world, control, m, false);
            }
        }
        for i in 0..suit.parts.len() {
            seal_part(world, control, i, false);
        }
    }
    for i in 0..suit.parts.len() {
        do_retract(world, control, i, None);
    }
    if suit.active {
        control_activation(world, control, false);
    }
    if let Some(mut s) = world.get_mut::<ModSuit>(control) {
        s.steps.clear();
        s.activating = false;
        s.wearer = None;
    }
    if let Some(old) = suit.wearer
        && world.get_entity(old).is_ok()
    {
        world
            .entity_mut(old)
            .remove::<(SuitSlowdown, ModuleSelected, modules::NoSlip)>()
            .remove::<(modules::Jetpack, modules::VisorHud, modules::FastCuffing)>();
    }
}

/// tg `deploy(user, part, instant)`. True when the part went on.
pub fn deploy(
    world: &mut World,
    control: Entity,
    index: usize,
    user: Option<Entity>,
    instant: bool,
) -> bool {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return false;
    };
    let Some(part) = suit.parts.get(index).cloned() else {
        return false;
    };
    let Some(wearer) = suit.wearer.filter(|&w| world.get_entity(w).is_ok()) else {
        buzz(world, control);
        return false;
    };
    if world.get::<CarriedBy>(part.item).is_some() {
        if user.is_some() {
            refuse(world, control, user, "уже развёрнуто!");
        }
        return false;
    }
    let Some(inventory) = world.get::<Inventory>(wearer).cloned() else {
        return false;
    };
    // The worn item under it (`CAN_BE_OVERSLOT`) goes inside the part.
    let mut overslot = None;
    if let Some(worn) = inventory.slot(part.slot)
        && world
            .get::<DmItem>(worn)
            .is_some_and(|i| i.item_flags & CAN_BE_OVERSLOT != 0)
        && !crate::inventory::no_drop(world, worn)
    {
        world
            .entity_mut(worn)
            .remove::<CarriedBy>()
            .insert(InSuit(control));
        overslot = Some(worn);
    }
    let inventory = world.get::<Inventory>(wearer).cloned().unwrap_or_default();
    if crate::inventory::can_equip(world, wearer, &inventory, part.item, part.slot) {
        world
            .entity_mut(part.item)
            .insert((CarriedBy(wearer), NoDrop));
        *world
            .get_mut::<Inventory>(wearer)
            .expect("checked above")
            .slot_mut(part.slot) = Some(part.item);
        if let Some(mut s) = world.get_mut::<ModSuit>(control) {
            s.parts[index].overslot = overslot;
        }
        if user.is_some() {
            hiss(world, control, wearer, &part, true);
        }
        if !suit.active || part.sealed {
            return true;
        }
        if instant {
            seal_part(world, control, index, true);
        } else {
            queue_steps(
                world,
                control,
                [Step::Seal {
                    part: index,
                    seal: true,
                    retract: false,
                }],
            );
        }
        return true;
    }
    if let Some(worn) = overslot {
        rewear(world, wearer, worn, part.slot);
    }
    if user.is_some() {
        refuse(world, control, user, "часть тела занята!");
    }
    false
}

/// An overslotted item back into its slot, else at the wearer's feet.
fn rewear(world: &mut World, wearer: Entity, item: Entity, slot: InventorySlot) {
    world.entity_mut(item).remove::<InSuit>();
    let inventory = world.get::<Inventory>(wearer).cloned().unwrap_or_default();
    if world.get_entity(wearer).is_ok()
        && crate::inventory::can_equip(world, wearer, &inventory, item, slot)
    {
        world.entity_mut(item).insert(CarriedBy(wearer));
        if let Some(mut inv) = world.get_mut::<Inventory>(wearer) {
            *inv.slot_mut(slot) = Some(item);
        }
    } else if let Some(pos) = world.get::<TilePos>(wearer).copied() {
        crate::storage::take_out(world, item, pos);
    }
}

/// The "deploys/retracts with a mechanical hiss" message and sound.
fn hiss(world: &mut World, control: Entity, wearer: Entity, part: &ModPart, deployed: bool) {
    let part_name = name(world, part.item);
    let wearer_name = name(world, wearer);
    let (own, others) = if deployed {
        (
            format!("{part_name} разворачивается с механическим шипением."),
            format!("{part_name} {wearer_name} разворачивается с механическим шипением."),
        )
    } else {
        (
            format!(
                "{part_name} убирается в {} с механическим шипением.",
                name(world, control)
            ),
            format!("{part_name} {wearer_name} убирается с механическим шипением."),
        )
    };
    if let Some(pos) = world.get::<TilePos>(wearer).copied() {
        crate::security::show(
            world,
            pos,
            crate::health::DEFAULT_MESSAGE_RANGE,
            ChatChannel::Local,
            (Span::Notice, others),
            &[(wearer, Span::Notice, own)],
        );
    }
    sound_at(world, control, MECHMOVE, 25.0, SHORT_RANGE_SOUND_EXTRARANGE);
}

/// tg `retract(user, part, instant)`. True when it retracts (now or after unsealing).
pub fn retract(
    world: &mut World,
    control: Entity,
    index: usize,
    user: Option<Entity>,
    instant: bool,
) -> bool {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return false;
    };
    let Some(part) = suit.parts.get(index) else {
        return false;
    };
    if world.get::<CarriedBy>(part.item).is_none() {
        if user.is_some() {
            refuse(world, control, user, "уже свёрнуто!");
        }
        return false;
    }
    if suit.active && part.sealed {
        if !instant {
            queue_steps(
                world,
                control,
                [Step::Seal {
                    part: index,
                    seal: false,
                    retract: true,
                }],
            );
            return true;
        }
        seal_part(world, control, index, false);
    }
    do_retract(world, control, index, user);
    true
}

/// The retraction itself: the part back into the unit, the overslotted item back on.
fn do_retract(world: &mut World, control: Entity, index: usize, user: Option<Entity>) {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return;
    };
    let Some(part) = suit.parts.get(index).cloned() else {
        return;
    };
    let Some(wearer) = world.get::<CarriedBy>(part.item).map(|c| c.0) else {
        return;
    };
    world.entity_mut(part.item).remove::<(CarriedBy, NoDrop)>();
    if let Some(worn) = part.overslot {
        if let Some(mut s) = world.get_mut::<ModSuit>(control) {
            s.parts[index].overslot = None;
        }
        if world.get_entity(worn).is_ok() {
            rewear(world, wearer, worn, part.slot);
        }
    }
    crate::inventory::refresh_look(world, wearer);
    if user.is_some() {
        hiss(world, control, wearer, &part, false);
    }
}

/// tg `quick_deploy`: every part out, or every part in when all are out.
pub fn quick_deploy(world: &mut World, control: Entity, user: Entity) -> bool {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return false;
    };
    if suit.activating || !suit.steps.is_empty() {
        refuse(world, control, Some(user), "идёт герметизация!");
        return false;
    }
    let Some(wearer) = suit.wearer else {
        refuse(world, control, Some(user), "не надет!");
        return false;
    };
    let out = suit
        .parts
        .iter()
        .filter(|p| world.get::<CarriedBy>(p.item).is_some())
        .count();
    let deploying = out == 0;
    let text = if deploying {
        "разворачивает"
    } else {
        "сворачивает"
    };
    let (own, others) = (
        format!(
            "{} {text} свои части с механическим шипением.",
            name(world, control)
        ),
        format!(
            "{} {} {text} свои части с механическим шипением.",
            name(world, control),
            name(world, wearer)
        ),
    );
    if let Some(pos) = world.get::<TilePos>(wearer).copied() {
        crate::security::show(
            world,
            pos,
            crate::health::DEFAULT_MESSAGE_RANGE,
            ChatChannel::Local,
            (Span::Notice, others),
            &[(wearer, Span::Notice, own)],
        );
    }
    sound_at(world, control, MECHMOVE, 25.0, SHORT_RANGE_SOUND_EXTRARANGE);
    for i in 0..suit.parts.len() {
        let is_out = world.get::<CarriedBy>(suit.parts[i].item).is_some();
        if deploying && !is_out {
            deploy(world, control, i, None, false);
        } else if !deploying && is_out {
            retract(world, control, i, None, false);
        }
    }
    true
}

/// tg `seal_part(part, is_sealed)`.
pub fn seal_part(world: &mut World, control: Entity, index: usize, sealed: bool) {
    let Some(part) = world
        .get::<ModSuit>(control)
        .and_then(|s| s.parts.get(index).cloned())
    else {
        return;
    };
    if let Some(mut s) = world.get_mut::<ModSuit>(control) {
        s.parts[index].sealed = sealed;
    }
    if let Some(mut c) = world.get_mut::<DmClothing>(part.item) {
        if sealed {
            c.clothing_flags |= part.def.sealed_clothing;
            c.flags_inv |= part.def.sealed_inv;
            c.flags_cover |= part.def.sealed_cover;
            c.heat_protection = part.heat_protection;
            c.cold_protection = part.cold_protection;
        } else {
            c.clothing_flags &= !part.def.sealed_clothing;
            c.flags_inv &= !part.def.sealed_inv;
            c.flags_cover &= !part.def.sealed_cover;
            c.heat_protection = 0;
            c.cold_protection = 0;
        }
    }
    update_icons(world, control);
    module_part_activation(world, control, sealed);
}

/// The end of `seal_part` and `control_activation`: modules whose required parts became all
/// sealed activate their parts, those that lost one deactivate.
fn module_part_activation(world: &mut World, control: Entity, sealing: bool) {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return;
    };
    let theme = theme_of(world, &suit);
    let sealed = suit.part_slots(&theme, true);
    for &module in &suit.modules {
        let (Some(def), Some(state)) = (
            modules::def_of(world, module),
            world.get::<ModModule>(module).cloned(),
        ) else {
            continue;
        };
        let has = suit.active && modules::has_required_parts(&def, sealed);
        if sealing {
            if !state.part_activated && has {
                modules::on_part_activation(world, control, module);
                if let Some(mut m) = world.get_mut::<ModModule>(module) {
                    m.part_activated = true;
                }
            }
        } else if state.part_activated && !has {
            modules::on_part_deactivation(world, control, module);
            if let Some(mut m) = world.get_mut::<ModModule>(module) {
                m.part_activated = false;
            }
            if state.active && def.allow_flags & modules::MODULE_ALLOW_INACTIVE == 0 {
                modules::deactivate(world, control, module, false);
            }
        }
    }
}

/// tg `control_activation(is_on)`.
pub fn control_activation(world: &mut World, control: Entity, on: bool) {
    if let Some(mut s) = world.get_mut::<ModSuit>(control) {
        s.active = on;
    }
    if on {
        module_part_activation(world, control, true);
    } else {
        let modules_list = world
            .get::<ModSuit>(control)
            .map(|s| s.modules.clone())
            .unwrap_or_default();
        for module in modules_list {
            if world
                .get::<ModModule>(module)
                .is_some_and(|m| m.part_activated)
            {
                modules::on_part_deactivation(world, control, module);
                if let Some(mut m) = world.get_mut::<ModModule>(module) {
                    m.part_activated = false;
                }
            }
        }
    }
    update_icons(world, control);
}

fn queue_steps(world: &mut World, control: Entity, steps: impl IntoIterator<Item = Step>) {
    let now = now(world);
    let step_secs = world
        .get::<ModSuit>(control)
        .map(|s| theme_of(world, s).activation_step_time / 10.0)
        .unwrap_or(1.0);
    if let Some(mut s) = world.get_mut::<ModSuit>(control) {
        if s.steps.is_empty() {
            s.step_at = now + f64::from(step_secs);
        }
        s.steps.extend(steps);
    }
}

/// tg `toggle_activate(user, force_deactivate)`: starts the sealing sequence. True when it
/// started.
pub fn toggle_activate(
    world: &mut World,
    control: Entity,
    user: Option<Entity>,
    force: bool,
) -> bool {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return false;
    };
    let Some(wearer) = suit.wearer else {
        if !force {
            refuse(world, control, user, "не надет!");
        }
        return false;
    };
    if !force {
        if suit.locked && !suit.active && !user.is_some_and(|u| allowed(world, control, u)) {
            refuse(world, control, user, "доступ запрещён!");
            return false;
        }
        if charge(world, control) <= 0.0 {
            refuse(world, control, user, "нет источника питания!");
            return false;
        }
        if suit.open {
            refuse(world, control, user, "панель открыта!");
            return false;
        }
    }
    if suit.activating || !suit.steps.is_empty() {
        if !force {
            let text = if suit.active {
                "уже отключается!"
            } else {
                "уже запускается!"
            };
            refuse(world, control, user, text);
        }
        return false;
    }
    for &module in &suit.modules {
        let keeps = modules::def_of(world, module)
            .is_some_and(|d| d.allow_flags & modules::MODULE_ALLOW_INACTIVE != 0);
        if world.get::<ModModule>(module).is_some_and(|m| m.active) && !keeps {
            modules::deactivate(world, control, module, false);
        }
    }
    if let Some(mut s) = world.get_mut::<ModSuit>(control) {
        s.activating = true;
    }
    let text = if suit.active {
        "MODsuit отключается."
    } else {
        "MODsuit запускается."
    };
    tell_mob(world, wearer, Span::Notice, text);
    let deployed: Vec<usize> = (0..suit.parts.len())
        .filter(|&i| world.get::<CarriedBy>(suit.parts[i].item).is_some())
        .collect();
    let seal = !suit.active;
    let seals = deployed.into_iter().map(move |part| Step::Seal {
        part,
        seal,
        retract: false,
    });
    let steps: Vec<Step> = if suit.active {
        std::iter::once(Step::Control { on: false })
            .chain(seals)
            .collect()
    } else {
        seals.chain([Step::Control { on: true }]).collect()
    };
    queue_steps(world, control, steps);
    true
}

/// The next due step of the sequence (tg's do_after finishing): stops when the wearer is gone
/// (`extra_checks = get_wearer`).
fn run_steps(world: &mut World, control: Entity) {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return;
    };
    if suit.steps.is_empty() || now(world) < suit.step_at {
        return;
    }
    let Some(wearer) = suit.wearer else {
        if let Some(mut s) = world.get_mut::<ModSuit>(control) {
            s.steps.clear();
            s.activating = false;
        }
        return;
    };
    let step = world
        .get_mut::<ModSuit>(control)
        .and_then(|mut s| s.steps.pop_front())
        .expect("checked above");
    match step {
        Step::Seal {
            part,
            seal,
            retract,
        } => {
            if let Some(p) = suit.parts.get(part).cloned() {
                let message = if seal {
                    &p.def.sealed_message
                } else {
                    &p.def.unsealed_message
                };
                let text = format!("{} {message}.", name(world, p.item));
                tell_mob(world, wearer, Span::Notice, text);
                sound_at(world, control, MECHMOVE, 25.0, SHORT_RANGE_SOUND_EXTRARANGE);
                seal_part(world, control, part, seal);
                if retract {
                    do_retract(world, control, part, Some(wearer));
                }
            }
        }
        Step::Control { on } => {
            control_activation(world, control, on);
            if on {
                sound_at(
                    world,
                    control,
                    SYNTH_YES,
                    50.0,
                    SHORT_RANGE_SOUND_EXTRARANGE,
                );
                // BandaStation: the theme's `activation_sound` to the wearer only.
                let sound = theme_of(world, &suit).activation_sound;
                if !sound.is_empty() {
                    let pos = crate::chem::map_pos(world, control);
                    crate::hallucination::play_local(world, wearer, pos, &sound, 50.0, false);
                }
            } else {
                sound_at(world, control, SYNTH_NO, 50.0, SHORT_RANGE_SOUND_EXTRARANGE);
                tell_mob(
                    world,
                    wearer,
                    Span::Notice,
                    "Блок управления отключён. Модули недоступны.",
                );
            }
        }
    }
    let Some(s) = world.get::<ModSuit>(control).cloned() else {
        return;
    };
    if !s.steps.is_empty() {
        let step_secs = f64::from(theme_of(world, &s).activation_step_time / 10.0);
        let now = now(world);
        if let Some(mut s) = world.get_mut::<ModSuit>(control) {
            s.step_at = now + step_secs;
        }
    } else if s.activating {
        if let Some(mut s) = world.get_mut::<ModSuit>(control) {
            s.activating = false;
        }
        let who = name(world, wearer);
        let text = if s.active {
            format!("Системы запущены. Части загерметизированы. Добро пожаловать, {who}.")
        } else {
            format!("Системы отключены. Части разгерметизированы. До свидания, {who}.")
        };
        tell_mob(world, wearer, Span::Notice, text);
    }
}

// ---------------------------------------------------------------------------------------------
// The tick

/// Every unit, every tick: its wearer and holder, its sequence and tool delays, power
/// (`process`), the wearer's slowdown, whether it can be taken off and its window data.
// Cost: one pass over the control units (a few dozen per round at most) per tick.
pub fn tick(world: &mut World) {
    let mut q = world.query_filtered::<Entity, With<ModSuit>>();
    let suits: Vec<Entity> = q.iter(world).collect();
    let dt = world.resource::<Time>().delta_secs_f64();
    for control in suits {
        track_wearer(world, control);
        run_steps(world, control);
        run_pending(world, control);
        process(world, control, dt);
        update_wearer(world, control);
        if modules::refresh_overlays(world, control)
            && let Some(mob) = world.get::<ModSuit>(control).and_then(|s| s.wearer)
        {
            crate::inventory::refresh_look(world, mob);
        }
        refresh_view(world, control);
    }
}

/// tg `visual_equipped`/`dropped`/`Moved`: the wearer is who wears it in its theme's slot.
fn track_wearer(world: &mut World, control: Entity) {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return;
    };
    let holder = world.get::<CarriedBy>(control).map(|c| c.0);
    let slot_flags = theme_of(world, &suit).slot_flags;
    let wearer = holder.filter(|&mob| {
        world
            .get::<Inventory>(mob)
            .and_then(|i| i.slot_of(control))
            .is_some_and(|slot| slot_bit(slot) & slot_flags != 0)
    });
    if wearer != suit.wearer {
        if suit.wearer.is_some() {
            clean_up(world, control);
        }
        if let Some(mob) = wearer {
            set_wearer(world, control, mob);
        }
    }
    if holder != suit.holder {
        if let Some(mut s) = world.get_mut::<ModSuit>(control) {
            s.holder = holder;
        }
        refresh_buttons(world, suit.holder);
        refresh_buttons(world, holder);
    } else if wearer != suit.wearer {
        refresh_buttons(world, holder);
    }
}

/// A tool delay that ran out: done if the mob stayed, still holds what it held and still
/// reaches the unit (tg `use_tool`/`do_after` checks).
fn run_pending(world: &mut World, control: Entity) {
    let Some(pending) = world.get::<ModSuit>(control).and_then(|s| s.pending) else {
        return;
    };
    if now(world) < pending.until {
        return;
    }
    if let Some(mut s) = world.get_mut::<ModSuit>(control) {
        s.pending = None;
    }
    let mob = pending.mob;
    let stayed = world.get::<TilePos>(mob) == Some(&pending.mob_pos)
        && world.get::<Inventory>(mob).map(Inventory::active_item) == Some(pending.held)
        && world.get::<Stat>(mob) == Some(&Stat::Conscious);
    let reach = world.get::<CarriedBy>(control).is_some_and(|c| c.0 == mob)
        || crate::storage::in_reach(world, mob, control);
    if !stayed || !reach {
        tell(world, pending.client, Span::Warning, "прервано!");
        return;
    }
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return;
    };
    match pending.kind {
        PendingKind::Cover => {
            if suit.active || suit.activating {
                tell(world, pending.client, Span::Warning, "блок активен!");
                return;
            }
            if let Some(mut s) = world.get_mut::<ModSuit>(control) {
                s.open = !s.open;
            }
            let text = if suit.open {
                "крышка закрыта"
            } else {
                "крышка открыта"
            };
            tell(world, pending.client, Span::Notice, text);
        }
        PendingKind::RemoveCore => {
            let Some(core) = suit.core.filter(|_| suit.open) else {
                tell(world, pending.client, Span::Warning, "прервано!");
                return;
            };
            if let Some(mut s) = world.get_mut::<ModSuit>(control) {
                s.core = None;
            }
            if let Some(pos) = drop_pos(world, control) {
                place_on(world, core, pos);
            }
            tell(world, pending.client, Span::Notice, "ядро извлечено");
        }
        PendingKind::RemoveCell => {
            let Some(cell) = cell_of(world, control).filter(|_| suit.open) else {
                tell(
                    world,
                    pending.client,
                    Span::Warning,
                    "нет элемента питания!",
                );
                return;
            };
            if let Some(core) = suit.core
                && let Some(mut c) = world.get_mut::<ModCore>(core)
            {
                c.cell = None;
            }
            world
                .entity_mut(cell)
                .remove::<StoredIn>()
                .insert(Networked);
            if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
                crate::storage::put_in_hands(world, mob, &inventory, cell);
            }
            sound_at(world, control, CLICK, 50.0, SILENCED_SOUND_EXTRARANGE);
            tell(
                world,
                pending.client,
                Span::Notice,
                "элемент питания извлечён",
            );
        }
    }
}

/// tg `process(seconds_per_tick)`: an active unit without charge shuts down; otherwise it pays
/// `charge_drain` and each module runs.
fn process(world: &mut World, control: Entity, dt: f64) {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return;
    };
    if !suit.active {
        return;
    }
    if charge(world, control) <= 0.0 && !suit.activating {
        tell_opt(world, suit.wearer, Span::Warning, "нет питания!");
        toggle_activate(world, control, suit.wearer, true);
        return;
    }
    let drain = theme_of(world, &suit).charge_drain;
    subtract_charge(world, control, drain * dt);
    for &module in &suit.modules {
        modules::on_process(world, control, module, dt);
    }
}

/// The wearer's slowdown (tg `update_speed`: the total split over every part, unsealed parts
/// never faster), and whether the unit comes off (`can_mob_unequip`).
fn update_wearer(world: &mut World, control: Entity) {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return;
    };
    let locked_on = suit.wearer.is_some()
        && (suit.active
            || suit.activating
            || suit
                .parts
                .iter()
                .any(|p| world.get::<CarriedBy>(p.item).is_some()));
    let has = world.get::<NoDrop>(control).is_some();
    if locked_on && !has {
        world.entity_mut(control).insert(NoDrop);
    } else if !locked_on && has {
        world.entity_mut(control).remove::<NoDrop>();
    }
    let Some(wearer) = suit.wearer.filter(|&w| world.get_entity(w).is_ok()) else {
        return;
    };
    let theme = theme_of(world, &suit);
    let mut total = theme.slowdown_deployed;
    for &m in &suit.modules {
        total += modules::slowdown(world, m);
    }
    let share = total / (suit.parts.len() + 1) as f32;
    let mut slowdown = share;
    // `/datum/movespeed_modifier/jetpack/full_speed` (`movetypes = FLOATING`).
    if world
        .get::<modules::Jetpack>(wearer)
        .is_some_and(|j| suit.modules.contains(&j.module))
        && world.get::<crate::spacemove::Weightless>(wearer).is_some()
    {
        slowdown += modules::JETPACK_FULL_SPEED;
    }
    for p in &suit.parts {
        if world
            .get::<CarriedBy>(p.item)
            .is_some_and(|c| c.0 == wearer)
        {
            slowdown += if p.sealed { share } else { share.max(0.0) };
        }
    }
    let new = SuitSlowdown(slowdown);
    match world.get_mut::<SuitSlowdown>(wearer) {
        Some(mut old) => {
            old.set_if_neq(new);
        }
        None => {
            world.entity_mut(wearer).insert(new);
        }
    }
}

/// `ui_data`/`ui_static_data` into [`ModSuitView`].
fn refresh_view(world: &mut World, control: Entity) {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return;
    };
    let theme = theme_of(world, &suit);
    let now = now(world);
    let (color, text) = chargebar(world, control);
    let (charge_now, charge_max) = if core_state(world, control).is_some_and(|c| c.infinite) {
        (1.0, 1.0)
    } else {
        (charge(world, control), max_charge(world, control))
    };
    let module_info = suit
        .modules
        .iter()
        .filter_map(|&m| {
            let def = modules::def_of(world, m)?;
            let state = world.get::<ModModule>(m)?;
            Some(ModuleInfo {
                module_name: name(world, m),
                description: def.desc.clone(),
                module_type: def.module_type,
                module_active: state.active,
                pinned: false,
                idle_power: def.idle_power_cost,
                active_power: modules::active_power(world, m),
                use_energy: def.use_energy_cost,
                module_complexity: def.complexity,
                cooldown_time: def.cooldown_time,
                cooldown: (((state.cooldown_until - now) * 10.0).max(0.0) as f32 / 10.0).round()
                    * 10.0,
                id: def.tgui_id.clone(),
                reference: m.to_bits(),
                configuration_data: modules::configuration(world, m),
            })
        })
        .collect();
    let parts = suit
        .parts
        .iter()
        .map(|p| PartInfo {
            slot: slot_name(p.slot).to_owned(),
            name: name(world, p.item),
            deployed: world.get::<CarriedBy>(p.item).is_some(),
            reference: p.item.to_bits(),
        })
        .collect();
    let core_name = suit.core.map(|c| name(world, c));
    let view = ModSuitView {
        statics: ModSuitStatic {
            ui_theme: theme.ui_theme.clone(),
            complexity_max: theme.complexity_max,
        },
        data: ModSuitData {
            suit_status: SuitStatus {
                core_name,
                charge_current: charge_now,
                charge_max,
                chargebar_color: color,
                chargebar_string: text,
                active: suit.active,
                activating: suit.activating,
                open: suit.open,
                malfunctioning: world
                    .get::<crate::device_wires::WireState>(control)
                    .is_some_and(|w| w.malfunctioning),
                locked: suit.locked,
                complexity: suit.complexity,
            },
            user_status: UserStatus {
                user_name: suit.wearer.map(|w| name(world, w)).unwrap_or_default(),
                user_assignment: String::new(),
            },
            control: name(world, control),
            module_info,
            parts,
        },
        pins: suit
            .pins
            .iter()
            .map(|&(user, m)| (user, m.to_bits()))
            .collect(),
    };
    if let Some(mut old) = world.get_mut::<ModSuitView>(control) {
        old.set_if_neq(view);
    }
}

/// `parse_slot_flags`.
fn slot_name(slot: InventorySlot) -> &'static str {
    match slot {
        InventorySlot::Head => "head",
        InventorySlot::Suit => "suit",
        InventorySlot::Gloves => "gloves",
        InventorySlot::Shoes => "feet",
        InventorySlot::Eyes => "eyes",
        InventorySlot::Neck => "neck",
        InventorySlot::Mask => "mask",
        _ => "back",
    }
}

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

/// tg `default_state` of a carried item: the window stays while its user carries the unit.
fn window_status(mut windows: Query<&mut UiWindow>, carried: Query<&CarriedBy>) {
    for mut window in &mut windows {
        if window.interface != ModSuitUi::NAME {
            continue;
        }
        let held = carried
            .get(window.source)
            .is_ok_and(|c| c.0 == window.actor);
        if !held {
            window.cap_status(UiStatus::Close);
        }
    }
}

/// tg `/obj/item/mod/control/proc/ui_interact` (the panel action).
pub fn open_panel(world: &mut World, client: Entity, mob: Entity, control: Entity) {
    let title = name(world, control);
    world.write_message(OpenUi::new::<ModSuitUi>(client, mob, control, title));
}

/// `ui_act`, checked against the unit now (`locate(params["ref"]) in modules`/`get_parts()`).
fn suit_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<ModSuitUi>>>) {
    let pending: Vec<(Entity, Entity, ModAct)> = acts
        .read(world.resource::<Messages<UiActed<ModSuitUi>>>())
        .map(|a| (a.actor, a.source, a.act.clone()))
        .collect();
    for (actor, control, act) in pending {
        let Some(suit) = world.get::<ModSuit>(control).cloned() else {
            continue;
        };
        let module = |bits: u64| Entity::try_from_bits(bits).filter(|e| suit.modules.contains(e));
        match act {
            ModAct::Lock => {
                if !suit.locked || allowed(world, control, actor) {
                    if let Some(mut s) = world.get_mut::<ModSuit>(control) {
                        s.locked = !s.locked;
                    }
                    let text = if suit.locked {
                        "разблокировано"
                    } else {
                        "заблокировано"
                    };
                    tell_mob(world, actor, Span::Notice, text);
                } else {
                    refuse(world, control, Some(actor), "недостаточно доступа!");
                }
            }
            ModAct::Activate => {
                toggle_activate(world, control, Some(actor), false);
            }
            ModAct::Select(bits) => {
                if let Some(m) = module(bits) {
                    modules::on_select(world, control, m, actor);
                }
            }
            ModAct::Configure(bits, key, value) => {
                if let Some(m) = module(bits) {
                    modules::configure_edit(world, control, m, &key, &value);
                }
            }
            ModAct::Pin(bits) => {
                if let Some(m) = module(bits) {
                    pin(world, control, m, actor);
                }
            }
            ModAct::Deploy(bits) => {
                let Some(index) = Entity::try_from_bits(bits).and_then(|e| suit.part_index(e))
                else {
                    continue;
                };
                if world.get::<CarriedBy>(suit.parts[index].item).is_some() {
                    retract(world, control, index, Some(actor), false);
                } else {
                    deploy(world, control, index, Some(actor), false);
                }
            }
        }
    }
}

/// tg `module.pin(user)`: a button of its own for `user`, or the button gone again.
pub fn pin(world: &mut World, control: Entity, module: Entity, user: Entity) {
    let passive = modules::def_of(world, module).is_none_or(|d| d.module_type == MODULE_PASSIVE);
    if passive {
        return;
    }
    let holder = world.get::<ModSuit>(control).and_then(|s| s.holder);
    if let Some(mut s) = world.get_mut::<ModSuit>(control) {
        if let Some(i) = s.pins.iter().position(|&p| p == (user, module)) {
            s.pins.remove(i);
        } else {
            s.pins.push((user, module));
        }
    }
    refresh_buttons(world, holder);
}

// ---------------------------------------------------------------------------------------------
// Action buttons and prompts

fn button(world: &World, id: String, name: &str, dmi: &str, state: &str) -> (ActionButton, f64) {
    let icons = world.get_resource::<IconIndex>();
    (
        ActionButton {
            id,
            name: name.to_owned(),
            icon: icons.and_then(|i| i.get(dmi, state)),
            background: icons.and_then(|i| i.get(BACKGROUNDS_DMI, "bg_mod")),
            border: icons.and_then(|i| i.get(BACKGROUNDS_DMI, "bg_mod_border")),
            available: true,
            cooldown: 0.0,
            active: false,
            maptext: None,
        },
        0.0,
    )
}

/// The mob's MOD buttons after the others it has (tg `actions_types` and pinned modules,
/// granted while it carries the unit).
pub fn refresh_buttons(world: &mut World, mob: Option<Entity>) {
    let Some(mob) = mob.filter(|&m| world.get_entity(m).is_ok()) else {
        return;
    };
    if world.get::<PlayerMob>(mob).is_none() {
        return;
    }
    let mut buttons: Vec<(ActionButton, f64)> = world
        .get::<ActionButtons>(mob)
        .map(|b| {
            b.0.iter()
                .filter(|(b, _)| !b.id.starts_with(BUTTON))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let now = now(world);
    for control in carried_suits(world, mob) {
        let Some(suit) = world.get::<ModSuit>(control).cloned() else {
            continue;
        };
        let bits = control.to_bits();
        let ready = if now < suit.ready_until {
            "activate-ready"
        } else {
            "activate"
        };
        buttons.push(button(
            world,
            format!("{BUTTON}{bits}:deploy"),
            "Развернуть MODsuit",
            ACTIONS_DMI,
            "deploy",
        ));
        buttons.push(button(
            world,
            format!("{BUTTON}{bits}:activate"),
            "Активировать MODsuit",
            ACTIONS_DMI,
            ready,
        ));
        buttons.push(button(
            world,
            format!("{BUTTON}{bits}:panel"),
            "Панель MODsuit",
            ACTIONS_DMI,
            "panel",
        ));
        buttons.push(button(
            world,
            format!("{BUTTON}{bits}:module"),
            "Переключить модуль",
            ACTIONS_DMI,
            "module",
        ));
        for &(user, module) in &suit.pins {
            if user != mob || !suit.modules.contains(&module) {
                continue;
            }
            let (dmi, state) = world
                .get::<ss13_mapload::DmAtom>(module)
                .map(|a| (a.icon.clone(), a.icon_state.clone()))
                .unwrap_or_default();
            let module_name = name(world, module);
            let mut b = button(
                world,
                format!("{BUTTON}{bits}:pin:{}", module.to_bits()),
                &module_name,
                &dmi,
                &state,
            );
            b.0.active = world.get::<ModModule>(module).is_some_and(|m| m.active);
            b.1 = world
                .get::<ModModule>(module)
                .map_or(0.0, |m| m.cooldown_until);
            buttons.push(b);
        }
    }
    if buttons.is_empty() && world.get::<ActionButtons>(mob).is_none() {
        return;
    }
    set_buttons(world, mob, buttons);
}

/// A click on a MOD button, checked now: the client controls the conscious mob that carries
/// the unit (`AB_CHECK_CONSCIOUS`).
fn trigger_buttons(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .filter(|a| a.act.starts_with(BUTTON))
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        if actor != source || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client) {
            continue;
        }
        if world.get::<Stat>(actor) != Some(&Stat::Conscious) {
            continue;
        }
        let mut fields = id.trim_start_matches(BUTTON).split(':');
        let Some(control) = fields
            .next()
            .and_then(|b| b.parse::<u64>().ok())
            .and_then(Entity::try_from_bits)
            .filter(|&c| world.get::<CarriedBy>(c).is_some_and(|h| h.0 == actor))
            .filter(|&c| world.get::<ModSuit>(c).is_some())
        else {
            continue;
        };
        match fields.next() {
            Some("deploy") => choose_deploy(world, client, actor, control),
            Some("activate") => {
                let now = now(world);
                let ready = world
                    .get::<ModSuit>(control)
                    .is_some_and(|s| now < s.ready_until);
                if let Some(mut s) = world.get_mut::<ModSuit>(control) {
                    s.ready_until = if ready { 0.0 } else { now + READY_SECS };
                }
                if ready {
                    toggle_activate(world, control, Some(actor), false);
                } else {
                    tell(
                        world,
                        client,
                        Span::Notice,
                        "Нажмите ещё раз, чтобы подтвердить.",
                    );
                }
                refresh_buttons(world, Some(actor));
            }
            Some("panel") => open_panel(world, client, actor, control),
            Some("module") => quick_module(world, client, actor, control),
            Some("pin") => {
                let module = fields
                    .next()
                    .and_then(|b| b.parse::<u64>().ok())
                    .and_then(Entity::try_from_bits)
                    .filter(|m| {
                        world
                            .get::<ModSuit>(control)
                            .is_some_and(|s| s.modules.contains(m))
                    });
                if let Some(module) = module {
                    modules::on_select(world, control, module, actor);
                }
            }
            _ => {}
        }
    }
}

/// tg `choose_deploy`'s radial menu, as a list: every part (with its state) and "all".
fn choose_deploy(world: &mut World, client: Entity, mob: Entity, control: Entity) {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return;
    };
    if suit.parts.is_empty() {
        return;
    }
    let mut items = vec!["Все части".to_owned()];
    let mut options = vec![None];
    for p in &suit.parts {
        let state = if world.get::<CarriedBy>(p.item).is_some() {
            "развёрнуто"
        } else {
            "свёрнуто"
        };
        items.push(format!("{} ({state})", name(world, p.item)));
        options.push(Some(p.item));
    }
    ask(
        world,
        client,
        mob,
        control,
        items,
        ChoiceKind::Deploy(options),
    );
}

/// tg `quick_module`'s radial menu, as a list of the non-passive modules.
fn quick_module(world: &mut World, client: Entity, mob: Entity, control: Entity) {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return;
    };
    let options: Vec<Entity> = suit
        .modules
        .iter()
        .copied()
        .filter(|&m| modules::def_of(world, m).is_some_and(|d| d.module_type != MODULE_PASSIVE))
        .collect();
    if options.is_empty() {
        return;
    }
    let items = options
        .iter()
        .map(|&m| {
            let state = if suit.selected == Some(m) {
                " (выбран)"
            } else if world.get::<ModModule>(m).is_some_and(|s| s.active) {
                " (активен)"
            } else {
                ""
            };
            format!("{}{state}", name(world, m))
        })
        .collect();
    ask(
        world,
        client,
        mob,
        control,
        items,
        ChoiceKind::Module(options),
    );
}

fn ask(
    world: &mut World,
    client: Entity,
    mob: Entity,
    control: Entity,
    items: Vec<String>,
    kind: ChoiceKind,
) {
    let title = name(world, control);
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            title,
            "",
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(ModChoice { control, kind });
}

/// The answers, revalidated: a conscious actor still carrying (deploy, modules) or reaching
/// (removal) the unit, the choice still part of it.
fn prompt_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in answers {
        let Some(choice) = world.get::<ModChoice>(prompt).cloned() else {
            continue;
        };
        let Some(PromptAnswer::Item(i)) = answer else {
            continue;
        };
        let control = choice.control;
        let Some(suit) = world.get::<ModSuit>(control).cloned() else {
            continue;
        };
        let able = world.get::<Stat>(actor) == Some(&Stat::Conscious)
            && !crate::status_effects::incapacitation(world, actor).incapacitated;
        let holds = world
            .get::<CarriedBy>(control)
            .is_some_and(|c| c.0 == actor);
        if !able {
            continue;
        }
        match choice.kind {
            ChoiceKind::Deploy(options) if holds => {
                if suit.activating || !suit.steps.is_empty() {
                    let text = if suit.active {
                        "идёт разгерметизация!"
                    } else {
                        "идёт герметизация!"
                    };
                    refuse(world, control, Some(actor), text);
                    continue;
                }
                match options.get(i) {
                    Some(None) => {
                        quick_deploy(world, control, actor);
                    }
                    Some(Some(part)) => {
                        let Some(index) = suit.part_index(*part) else {
                            continue;
                        };
                        let done = if world.get::<CarriedBy>(*part).is_some() {
                            retract(world, control, index, Some(actor), false)
                        } else {
                            deploy(world, control, index, Some(actor), false)
                        };
                        // tg asks again while some part is still on the other side.
                        if done {
                            let all_same = {
                                let s = world.get::<ModSuit>(control).cloned();
                                s.is_some_and(|s| {
                                    let out =
                                        |p: &ModPart| world.get::<CarriedBy>(p.item).is_some();
                                    s.parts.iter().all(out) || !s.parts.iter().any(out)
                                })
                            };
                            if !all_same {
                                choose_deploy(world, client, actor, control);
                            }
                        }
                    }
                    None => {}
                }
            }
            ChoiceKind::Module(options) if holds => {
                if let Some(&m) = options.get(i).filter(|m| suit.modules.contains(m)) {
                    modules::on_select(world, control, m, actor);
                }
            }
            ChoiceKind::Remove(options) => {
                let reach = holds || crate::storage::in_reach(world, actor, control);
                let Some(&m) = options.get(i).filter(|m| suit.modules.contains(m)) else {
                    continue;
                };
                let removable = modules::def_of(world, m).is_some_and(|d| d.removable);
                if !reach || !suit.open || !removable || !allowed(world, control, actor) {
                    continue;
                }
                uninstall(world, control, m);
                if let Some(pos) = drop_pos(world, control) {
                    place_on(world, m, pos);
                }
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Item interactions

/// `held` used on a control unit (on the floor or in the other hand): emags, modules, cores,
/// cells, tools and IDs (tg `emag_act`, `tool_act`, `screwdriver_act`, `crowbar_act`,
/// `wrench_act`, the core's `replace_cell`). True when the click was theirs.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if charge_plasma(world, client, held, target) {
        return true;
    }
    let Some(suit) = world.get::<ModSuit>(target).cloned() else {
        return false;
    };
    let control = target;
    let user = Some(mob);
    if crate::emag::is_emag(world, held) {
        if let Some(mut s) = world.get_mut::<ModSuit>(control) {
            s.locked = !s.locked;
        }
        let text = if suit.locked {
            "доступ разблокирован"
        } else {
            "доступ заблокирован"
        };
        tell(world, client, Span::Notice, text);
        return true;
    }
    modules::init_module(world, held);
    if world.get::<ModModule>(held).is_some() {
        if !suit.open {
            refuse(world, control, user, "крышка закрыта!");
        } else {
            install(world, control, held, user);
        }
        return true;
    }
    init_core(world, held);
    if world.get::<ModCore>(held).is_some() {
        if !suit.open {
            refuse(world, control, user, "крышка закрыта!");
        } else if suit.core.is_some() {
            refuse(world, control, user, "ядро уже установлено!");
        } else {
            if let Some(slot) = world.get::<Inventory>(mob).and_then(|i| i.slot_of(held))
                && let Some(mut inv) = world.get_mut::<Inventory>(mob)
            {
                *inv.slot_mut(slot) = None;
            }
            world
                .entity_mut(held)
                .remove::<(CarriedBy, TilePos, StoredIn)>()
                .insert(InSuit(control));
            if let Some(mut s) = world.get_mut::<ModSuit>(control) {
                s.core = Some(held);
            }
            tell(world, client, Span::Notice, "ядро установлено");
            sound_at(world, control, CLICK, 50.0, SILENCED_SOUND_EXTRARANGE);
        }
        return true;
    }
    if world.get::<ItemCell>(held).is_some()
        && let Some(core) = suit.core
        && world.get::<ModCore>(core).is_some_and(|c| c.standard)
    {
        if !suit.open {
            refuse(world, control, user, "крышка закрыта!");
        } else if cell_of(world, control).is_some() {
            refuse(world, control, user, "элемент питания уже установлен!");
        } else {
            if let Some(slot) = world.get::<Inventory>(mob).and_then(|i| i.slot_of(held))
                && let Some(mut inv) = world.get_mut::<Inventory>(mob)
            {
                *inv.slot_mut(slot) = None;
            }
            crate::storage::put_inside(world, held, core);
            if let Some(mut c) = world.get_mut::<ModCore>(core) {
                c.cell = Some(held);
            }
            tell(world, client, Span::Notice, "элемент питания установлен");
            sound_at(world, control, CLICK, 50.0, SILENCED_SOUND_EXTRARANGE);
        }
        return true;
    }
    match tool_of(world, held).map(|(kind, _)| kind) {
        Some(ToolKind::Screwdriver) => {
            if suit.active || suit.activating {
                refuse(world, control, user, "блок активен!");
                return true;
            }
            let text = if suit.open {
                "закрываем крышку..."
            } else {
                "открываем крышку..."
            };
            tell(world, client, Span::Notice, text);
            start_pending(
                world,
                control,
                client,
                mob,
                Some(held),
                PendingKind::Cover,
                COVER_SECS,
            );
            return true;
        }
        Some(ToolKind::Crowbar) => {
            if !suit.open {
                refuse(world, control, user, "крышка закрыта!");
                return true;
            }
            if !allowed(world, control, mob) {
                refuse(world, control, user, "недостаточно доступа!");
                return true;
            }
            let removable: Vec<Entity> = suit
                .modules
                .iter()
                .copied()
                .filter(|&m| modules::def_of(world, m).is_some_and(|d| d.removable))
                .collect();
            if removable.is_empty() {
                refuse(world, control, user, "нет модулей!");
                return true;
            }
            let items = removable.iter().map(|&m| name(world, m)).collect();
            let title = "Извлечение модуля".to_owned();
            let prompt = ask_world(
                world,
                Ask::new(
                    client,
                    mob,
                    title,
                    "Какой модуль извлечь?",
                    PromptKind::List {
                        items,
                        default: None,
                    },
                ),
            );
            world.entity_mut(prompt).insert(ModChoice {
                control,
                kind: ChoiceKind::Remove(removable),
            });
            return true;
        }
        Some(ToolKind::Wrench) if suit.open => {
            if suit.core.is_none() {
                refuse(world, control, user, "нет ядра!");
                return true;
            }
            tell(world, client, Span::Notice, "извлекаем ядро...");
            start_pending(
                world,
                control,
                client,
                mob,
                Some(held),
                PendingKind::RemoveCore,
                CORE_SECS,
            );
            return true;
        }
        _ => {}
    }
    if suit.open && crate::inventory::get_id(world, held).is_some() {
        // `update_access`.
        if !allowed(world, control, mob) {
            refuse(world, control, user, "недостаточно доступа!");
        } else {
            let access = crate::inventory::get_id(world, held)
                .and_then(|c| world.get::<IdCard>(c))
                .map(|c| c.access.clone())
                .unwrap_or_default();
            if let Some(mut s) = world.get_mut::<ModSuit>(control) {
                s.req_access = access;
            }
            tell(world, client, Span::Notice, "доступ обновлён");
        }
        return true;
    }
    false
}

/// A click on the control unit in the other hand: with an item, [`item_interaction`]; with an
/// empty hand and the cover open, the standard core's cell comes out (tg `on_attack_hand`).
/// tg `core/plasma/charge_plasma` (also through the unit's `COMSIG_ATOM_ITEM_INTERACTION`):
/// plasma ore or sheets used on a plasma core, or a unit with one, refuel it with as few as fill
/// it up. True when it took some.
pub fn charge_plasma(world: &mut World, client: Entity, held: Entity, target: Entity) -> bool {
    let Some(path) = world.get::<DmItem>(held).map(|i| i.path.clone()) else {
        return false;
    };
    let Some(&(_, given)) = PLASMA_CHARGERS.iter().find(|(p, _)| is_under(&path, p)) else {
        return false;
    };
    let core = match world.get::<ModSuit>(target) {
        Some(suit) => suit.core,
        None => Some(target),
    };
    let Some(core) = core else {
        return false;
    };
    init_core(world, core);
    let Some((charge, max)) = world.get::<ModCore>(core).and_then(|c| c.plasma) else {
        return false;
    };
    let uses = plasma_uses(charge, max, given, crate::tools::stack_amount(world, held));
    if uses == 0 || !crate::tools::use_stack(world, held, uses) {
        return false;
    }
    if let Some(mut c) = world.get_mut::<ModCore>(core)
        && let Some((now, max)) = c.plasma.as_mut()
    {
        *now = (*now + f64::from(uses) * given).min(*max);
    }
    tell(world, client, Span::Notice, "ядро заправлено");
    true
}

/// `min(plasma.amount, ROUND_UP((max_charge_amount() - charge_amount()) / charge_given))`.
pub fn plasma_uses(charge: f64, max: f64, given: f64, amount: u32) -> u32 {
    if given <= 0.0 {
        return 0;
    }
    let needed = ((max - charge) / given).ceil().max(0.0) as u32;
    amount.min(needed)
}

pub fn hand_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    active: Option<Entity>,
    item: Entity,
) -> bool {
    let Some(suit) = world.get::<ModSuit>(item).cloned() else {
        return false;
    };
    if let Some(held) = active {
        return item_interaction(world, client, mob, held, item);
    }
    if !suit.open || !core_state(world, item).is_some_and(|c| c.standard) {
        return false;
    }
    if cell_of(world, item).is_none() {
        tell(world, client, Span::Warning, "нет элемента питания!");
        return true;
    }
    tell(world, client, Span::Notice, "извлекаем элемент питания...");
    start_pending(
        world,
        item,
        client,
        mob,
        None,
        PendingKind::RemoveCell,
        CELL_SECS,
    );
    true
}

fn start_pending(
    world: &mut World,
    control: Entity,
    client: Entity,
    mob: Entity,
    held: Option<Entity>,
    kind: PendingKind,
    secs: f64,
) {
    let Some(&mob_pos) = world.get::<TilePos>(mob) else {
        return;
    };
    // `toolspeed` multiplies `use_tool` delays.
    let speed = held
        .and_then(|h| world.get::<ss13_mapload::DmTool>(h))
        .map_or(1.0, |t| f64::from(t.speed));
    let until = now(world) + secs * speed;
    if let Some(mut s) = world.get_mut::<ModSuit>(control) {
        s.pending = Some(Pending {
            kind,
            client,
            mob,
            mob_pos,
            held,
            until,
        });
    }
}

// ---------------------------------------------------------------------------------------------
// Clicks with a selected module, EMP

/// A wearer's middle click with a selected module (tg `on_special_click` -> `on_select_use`).
fn module_clicks(world: &mut World, mut cursor: Local<MessageCursor<ModuleClick>>) {
    let clicks: Vec<ModuleClick> = cursor
        .read(world.resource::<Messages<ModuleClick>>())
        .copied()
        .collect();
    for click in clicks {
        let Some(control) = worn_suit(world, click.mob) else {
            continue;
        };
        let Some(module) = world.get::<ModSuit>(control).and_then(|s| s.selected) else {
            continue;
        };
        modules::on_select_use(world, control, module, click.client, click.target);
    }
}

/// `/obj/item/mod/control/emp_act`: an active worn unit warns its wearer and, unless its
/// contents are protected, drops the selected module and burns the wearer `5 / severity`.
fn suit_emp(world: &mut World, mut cursor: Local<MessageCursor<EmpAct>>) {
    for ev in emp_acts(world, &mut cursor) {
        let Some(suit) = world.get::<ModSuit>(ev.target).cloned() else {
            continue;
        };
        let Some(wearer) = suit.wearer.filter(|_| suit.active) else {
            continue;
        };
        let text = if ev.severity > 1 {
            "Обнаружен слабый электромагнитный импульс!"
        } else {
            "Обнаружен сильный электромагнитный импульс!"
        };
        tell_mob(world, wearer, Span::Notice, text);
        if !ev.hits_contents() {
            continue;
        }
        if let Some(selected) = suit.selected {
            modules::deactivate(world, ev.target, selected, true);
        }
        if let Ok(mut entity) = world.get_entity_mut(wearer)
            && let Ok((mut health, mut stat)) =
                entity.get_components_mut::<(&mut Health, &mut Stat)>()
        {
            let mut new = *stat;
            crate::health::apply_damage(
                &mut health,
                &mut new,
                DamageType::Burn,
                ev.by_severity(5.0) as f32,
            );
            stat.set_if_neq(new);
        }
        let text = format!(
            "Вы чувствуете, как {} нагревается от ЭМИ, слегка обжигая вас.",
            name(world, ev.target)
        );
        tell_mob(world, wearer, Span::Danger, text);
    }
}

/// `EmpProtection` bits on/off without touching the other sources' bits.
pub(crate) fn set_emp_protection(world: &mut World, e: Entity, bits: u8, on: bool) {
    let old = world.get::<EmpProtection>(e).map_or(0, |p| p.0);
    let new = if on { old | bits } else { old & !bits };
    if new == 0 {
        world.entity_mut(e).remove::<EmpProtection>();
    } else {
        world.entity_mut(e).insert(EmpProtection(new));
    }
}

/// The EMP shield's `empprotection` on the unit.
pub(crate) const SHIELD_PROTECTION: u8 = PROTECT_ALL;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_follow_item_slot_bits() {
        assert_eq!(slot_of_flags(1 << 6), Some(InventorySlot::Head));
        assert_eq!(slot_of_flags(1 << 0), Some(InventorySlot::Suit));
        assert_eq!(slot_of_flags(1 << 2), Some(InventorySlot::Gloves));
        assert_eq!(slot_of_flags(1 << 7), Some(InventorySlot::Shoes));
        assert_eq!(slot_of_flags(1 << 10), Some(InventorySlot::Back));
        assert_eq!(base_of("/obj/item/clothing/shoes/mod"), "boots");
    }

    #[test]
    fn plasma_refuels_only_what_fits() {
        // 100 kJ core at 75 kJ: two sheets (20 kJ each) fill it, the rest stays.
        assert_eq!(plasma_uses(75_000.0, 100_000.0, 20_000.0, 5), 2);
        assert_eq!(plasma_uses(75_000.0, 100_000.0, 20_000.0, 1), 1);
        assert_eq!(plasma_uses(100_000.0, 100_000.0, 20_000.0, 5), 0);
    }
}
