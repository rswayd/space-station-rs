//! Law racks and their modules (server), after tg `/obj/machinery/ai_law_rack`
//! (`code/game/machinery/ai_core/law.dm`) and `/obj/item/ai_module`
//! (`code/game/objects/items/AI_modules/`):
//!
//! - A rack holds modules in slots; a base rack (`/base`: standard, core, portable) combines
//!   the law modules of its slots and of its linked broadcast racks (`update_lawset`) and gives
//!   that lawset to every silicon linked to it (`apply_lawset`: "Ваши законы были изменены." and
//!   the new laws, only when they changed). A plain rack only holds modules; a broadcaster
//!   (`/broadcaster`) feeds a base rack on its z-level. Depowered racks neither update nor
//!   count (`is_operational`).
//! - Modules: core ones (the first slot of a rack with a core slot, nowhere else) add inherent
//!   laws, supplied ones supplied laws, zeroth ones law 0, the hacked module a hacked law, the
//!   toy AI an ioned first law. A full core module (`law_id`) carries its lawset's laws.
//!   Freeform, safeguard, OneHuman and Asimov modules are configured in hand (`attack_self`:
//!   `tgui_input_text`); unconfigured ones refuse installation.
//! - The window (`LawRack`): with a module in hand an empty slot takes it, an unsecured module
//!   comes out into the hand, a screwdriver screws or unscrews a module (3 s), a welder welds a
//!   screwed one or unwelds it (3 s, 1 fuel), a multitool repairs an ioned one (4 s, its laws
//!   before the damage). Base racks link and unlink silicons (same z-level, not already
//!   linked, AI control on, cyborgs neither emagged nor scrambled), broadcasters link to a base
//!   rack; `refresh` rebuilds the linkable lists (30 s cooldown). Silicons cannot use racks.
//! - The mapped core rack is named after a greek letter and starts with the round's default
//!   lawset module welded in its core slot. A joining AI links to the first core rack that
//!   takes it (`link_to_first_rack`), a cyborg too when no AI takes it.
// shortcut: no rack stun or firewall on core removal and rack loss (only the message), no ion
// storm damage or ion law generation (the damaged module and the toy AI keep their laws), no
// wrench or cable anchoring, no examine of slots, no GPS on linked racks, no law change log,
// no IC filter on typed laws, deleted silicons are not kept as names in the linked list.

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::silicon::{AI_MODULE_ROOT, SiliconData};
use ss13_mapload::{DmItem, DmObj, DmTool};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use super::Silicons;
use super::ai::AiCore;
use super::chassis::{Chassis, name_of, tell_ai, tell_mob};
use super::cyborg::Cyborg;
use super::laws::{FALLBACK_LAWS, Laws, SPECIFIED_LAWS};
use super::ui::*;
use crate::construction::Job;
use crate::inventory::Inventory;
use crate::power::Unpowered;
use crate::sound::{PlaySound, playsound};
use crate::tools::{Actor, DoAfter, ToolKind, tell, tool_of};

pub const RACK_ROOT: &str = "/obj/machinery/ai_law_rack";
const BROADCASTER: &str = "/obj/machinery/ai_law_rack/broadcaster";
const BASE: &str = "/obj/machinery/ai_law_rack/base";
const CORE_RACK: &str = "/obj/machinery/ai_law_rack/base/core";
const SMALL_RACK: &str = "/obj/machinery/ai_law_rack/base/small";

const CORE_MODULE: &str = "/obj/item/ai_module/law/core";
const FULL_MODULE: &str = "/obj/item/ai_module/law/core/full";
const DAMAGED_MODULE: &str = "/obj/item/ai_module/law/core/full/damaged";
const ASIMOV_MODULE: &str = "/obj/item/ai_module/law/core/full/asimov";
const FREEFORM_CORE: &str = "/obj/item/ai_module/law/core/freeformcore";
const SUPPLIED_MODULE: &str = "/obj/item/ai_module/law/supplied";
const FREEFORM: &str = "/obj/item/ai_module/law/supplied/freeform";
const SAFEGUARD: &str = "/obj/item/ai_module/law/supplied/safeguard";
const ZEROTH_MODULE: &str = "/obj/item/ai_module/law/zeroth";
const ONEHUMAN: &str = "/obj/item/ai_module/law/zeroth/onehuman";
const HACKED_MODULE: &str = "/obj/item/ai_module/law/syndicate";
const TOY_AI: &str = "/obj/item/ai_module/law/toy_ai";

/// tg `GLOB.greek_letters`: the core racks' designations, in order.
const DESIGNATIONS: [&str; 24] = [
    "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta", "iota", "kappa",
    "lambda", "mu", "nu", "xi", "omicron", "pi", "rho", "sigma", "tau", "upsilon", "phi", "chi",
    "psi", "omega",
];
/// tg `refresh_cooldown`.
const REFRESH_COOLDOWN_SECS: f64 = 30.0;
/// Deciseconds of the timed steps.
const SCREW_DS: f32 = 30.0;
const WELD_DS: f32 = 30.0;
const REPAIR_DS: f32 = 40.0;
/// tg `HIGH_TEMPERATURE_REQUIRED`.
const HIGH_TEMPERATURE_REQUIRED: f32 = 1500.0;
/// tg `MAX_NAME_LEN`, `CONFIG_GET(number/max_law_len)`.
const MAX_NAME_LEN: u32 = 42;
const MAX_LAW_LEN: u32 = 1024;
const DECONSTRUCT_SOUND: &str = "sound/items/deconstruct.ogg";
const TERMINAL_ON: &str = "sound/machines/terminal/terminal_on.ogg";
const TERMINAL_OFF: &str = "sound/machines/terminal/terminal_off.ogg";

/// What a module's `apply_to_combined_lawset` does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ModuleKind {
    Core,
    Supplied,
    Zeroth,
    Hacked,
    Toy,
    /// No laws (AI upgrades, the base `/obj/item/ai_module/law`).
    #[default]
    Other,
}

/// A module's state (tg `/obj/item/ai_module/law` vars), made when first used.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct LawModule {
    pub kind: ModuleKind,
    /// tg `laws`: (law, ioned).
    pub laws: Vec<(String, bool)>,
    pub ioned: bool,
    /// tg `saved_laws`: the laws before the ion damage.
    pub saved: Option<Vec<(String, bool)>>,
    /// tg `targetName` of the safeguard and OneHuman modules.
    pub target_name: String,
}

impl LawModule {
    /// tg `set_ioned`.
    pub fn set_ioned(&mut self, ioned: bool) {
        if !self.ioned {
            self.saved = Some(self.laws.clone());
        }
        self.ioned = ioned;
    }

    /// tg `apply_to_combined_lawset`.
    pub fn apply(&self, laws: &mut Laws) {
        let first = self.laws.first().map(|(l, _)| l.as_str());
        match self.kind {
            ModuleKind::Core => {
                for (law, ioned) in &self.laws {
                    laws.add_inherent(law, None, *ioned);
                }
            }
            ModuleKind::Supplied => {
                laws.supplied
                    .extend(self.laws.iter().map(|(l, _)| (l.clone(), false)));
            }
            ModuleKind::Zeroth => {
                if let Some(law) = first {
                    laws.zeroth = Some(law.to_owned());
                }
            }
            ModuleKind::Hacked => {
                if let Some(law) = first {
                    laws.hacked.push(law.to_owned());
                }
            }
            ModuleKind::Toy => {
                if let Some(law) = first {
                    laws.add_inherent(law, Some(1), true);
                }
            }
            ModuleKind::Other => {}
        }
    }
}

pub fn module_kind(path: &str) -> ModuleKind {
    if is_under(path, TOY_AI) {
        ModuleKind::Toy
    } else if is_under(path, HACKED_MODULE) {
        ModuleKind::Hacked
    } else if is_under(path, CORE_MODULE) {
        ModuleKind::Core
    } else if is_under(path, SUPPLIED_MODULE) {
        ModuleKind::Supplied
    } else if is_under(path, ZEROTH_MODULE) {
        ModuleKind::Zeroth
    } else {
        ModuleKind::Other
    }
}

/// A new module of `path` (tg `Initialize`): its `laws`, a full core module's lawset; the
/// damaged one starts ioned.
pub fn new_module(data: &SiliconData, path: &str) -> LawModule {
    let def = data.module(path);
    let laws: Vec<String> = match def.and_then(|d| d.law_id.as_deref()) {
        Some(id) if is_under(path, FULL_MODULE) => data
            .lawset(id)
            .map(|l| l.inherent.clone())
            .unwrap_or_default(),
        _ => def.map(|d| d.laws.clone()).unwrap_or_default(),
    };
    let mut module = LawModule {
        kind: module_kind(path),
        laws: laws.into_iter().map(|l| (l, false)).collect(),
        ..default()
    };
    if is_under(path, DAMAGED_MODULE) {
        module.set_ioned(true);
    }
    module
}

fn item_path(world: &World, item: Entity) -> Option<String> {
    world.get::<DmItem>(item).map(|i| i.path.clone())
}

/// `istype(item, /obj/item/ai_module)`; gives it its [`LawModule`] when it has none yet.
pub fn ensure_module(world: &mut World, item: Entity) -> bool {
    if world.get::<LawModule>(item).is_some() {
        return true;
    }
    let Some(path) = item_path(world, item).filter(|p| is_under(p, AI_MODULE_ROOT)) else {
        return false;
    };
    let module = new_module(&world.resource::<Silicons>().0, &path);
    world.entity_mut(item).insert(module);
    true
}

/// What `configure` asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Configure {
    FreeformCore,
    Freeform,
    Hacked,
    Safeguard,
    OneHuman,
    Asimov,
}

pub fn configure_of(path: &str) -> Option<Configure> {
    Some(if is_under(path, FREEFORM_CORE) {
        Configure::FreeformCore
    } else if is_under(path, FREEFORM) {
        Configure::Freeform
    } else if is_under(path, HACKED_MODULE) {
        Configure::Hacked
    } else if is_under(path, SAFEGUARD) {
        Configure::Safeguard
    } else if is_under(path, ONEHUMAN) {
        Configure::OneHuman
    } else if is_under(path, ASIMOV_MODULE) {
        Configure::Asimov
    } else {
        return None;
    })
}

/// The answer of `configure` applied to the module (tg after `tgui_input_text`).
pub fn configured(module: &mut LawModule, kind: Configure, text: &str) {
    let set_first = |module: &mut LawModule, law: String| match module.laws.first_mut() {
        Some(first) => first.0 = law,
        None => module.laws.push((law, false)),
    };
    match kind {
        Configure::FreeformCore | Configure::Freeform | Configure::Hacked => {
            set_first(module, text.to_owned());
        }
        Configure::Safeguard => {
            module.target_name = text.to_owned();
            set_first(
                module,
                format!(
                    "Охраняйте {text}. Личности угрожающие {text} не считаются членами экипажа и \
                     должны быть уничтожены."
                ),
            );
        }
        Configure::OneHuman => {
            module.target_name = text.to_owned();
            set_first(module, format!("Только {text} - человек"));
        }
        Configure::Asimov => {
            module.target_name = text.to_owned();
            module.laws = [
                format!(
                    "Вы не можете причинить вред {text} или своим бездействием допустить, чтобы \
                     {text} был причинён вред."
                ),
                format!(
                    "Вы должны повиноваться всем приказам, которые даёт {text}, кроме тех \
                     случаев, когда эти приказы противоречат Первому Закону."
                ),
                "Вы должны заботиться о своей безопасности в той мере, в которой это не \
                 противоречит Первому или Второму Законам."
                    .to_owned(),
            ]
            .map(|l| (l, false))
            .to_vec();
        }
    }
}

/// tg `can_install_to_rack`: why the module refuses, if it does.
pub fn install_refusal(path: &str, module: &LawModule) -> Option<&'static str> {
    let empty_law = module.laws.first().is_none_or(|(l, _)| l.is_empty());
    match configure_of(path)? {
        Configure::FreeformCore | Configure::Freeform if empty_law => {
            Some("No law entered on module, please enter one.")
        }
        Configure::Safeguard if module.target_name.is_empty() => {
            Some("No name detected on module, please enter one.")
        }
        Configure::OneHuman if module.target_name.is_empty() => {
            Some("Имя не задано в модуле, пожалуйста, введите его.")
        }
        _ => None,
    }
}

/// tg `MODULE_UNSECURED`, `MODULE_SCREWED`, `MODULE_WELDED`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Security {
    #[default]
    Unsecured,
    Screwed,
    Welded,
}

impl Security {
    fn code(self) -> u8 {
        match self {
            Self::Unsecured => SLOT_UNSECURED,
            Self::Screwed => SLOT_SCREWED,
            Self::Welded => SLOT_WELDED,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slot {
    pub module: Entity,
    /// The module's name when it went in.
    pub name: String,
    pub security: Security,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RackKind {
    /// `/obj/machinery/ai_law_rack`: holds modules, does nothing with them.
    Plain,
    Broadcaster,
    /// `/obj/machinery/ai_law_rack/base`: links silicons.
    Base,
}

/// A rack's state (tg `ai_modules`, `linked_racks`, `linked_mobs`, the linkable lists).
#[derive(Component, Clone, Debug)]
pub struct LawRack {
    pub kind: RackKind,
    pub has_core_slot: bool,
    pub slots: Vec<Option<Slot>>,
    pub refresh_ready_at: f64,
    /// A broadcaster's base rack and its name.
    pub parent: Option<(Entity, String)>,
    /// A base rack's broadcasters.
    pub linked_racks: Vec<Entity>,
    /// A base rack's silicons, by the name they had when linked.
    pub linked_mobs: Vec<(String, Option<Entity>)>,
    /// `last_linkable_silicon_list` (base) or `last_linkable_rack_list` (broadcaster).
    pub linkable: Vec<(String, Entity)>,
    /// `last_linked_rack_module_count`.
    pub linked_module_count: u32,
    /// tg `combined_lawset`.
    pub combined: Laws,
    /// Whether it was operational when last looked at (`on_set_is_operational`).
    pub powered: bool,
}

/// `(kind, law_slots, has_core_slot)` of a rack type.
pub fn rack_spec(path: &str) -> Option<(RackKind, usize, bool)> {
    Some(if is_under(path, BROADCASTER) {
        (RackKind::Broadcaster, 3, false)
    } else if is_under(path, SMALL_RACK) {
        (RackKind::Base, 3, false)
    } else if is_under(path, CORE_RACK) {
        (RackKind::Base, 10, true)
    } else if is_under(path, BASE) {
        (RackKind::Base, 5, true)
    } else if is_under(path, RACK_ROOT) {
        (RackKind::Plain, 5, false)
    } else {
        return None;
    })
}

impl LawRack {
    pub fn new(kind: RackKind, slots: usize, has_core_slot: bool) -> Self {
        Self {
            kind,
            has_core_slot,
            slots: vec![None; slots],
            refresh_ready_at: 0.0,
            parent: None,
            linked_racks: Vec::new(),
            linked_mobs: Vec::new(),
            linkable: Vec::new(),
            linked_module_count: 0,
            combined: Laws::default(),
            powered: true,
        }
    }

    /// The base rack whose lawset this rack's modules feed (tg `get_parent_rack`).
    pub fn parent_rack(&self, me: Entity) -> Option<Entity> {
        match self.kind {
            RackKind::Base => Some(me),
            RackKind::Broadcaster => self.parent.map(|(p, _)| p),
            RackKind::Plain => None,
        }
    }

    fn modules(&self) -> impl Iterator<Item = Entity> + '_ {
        self.slots.iter().flatten().map(|s| s.module)
    }
}

/// On a silicon linked to a rack (tg `no_law_rack_link` and the rack's `linked_mobs`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkedRack(pub Entity);

/// A timed rack step ([`Job::LawRack`]); `slot` is 0-based.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LawRackJob {
    Screw { slot: usize, module: Entity },
    Weld { slot: usize, module: Entity },
    Repair { module: Entity },
}

/// A module's `configure` prompt.
#[derive(Component, Clone, Copy, Debug)]
struct ModulePrompt {
    item: Entity,
    mob: Entity,
    kind: Configure,
}

pub struct LawRackPlugin;

impl Plugin for LawRackPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<LawRackUi>()
            .add_systems(PostStartup, init_racks)
            .add_systems(
                Update,
                (
                    rack_acts.in_set(UiSystems::Handle),
                    module_configured
                        .in_set(UiSystems::Handle)
                        .after(PromptSystems::Answer),
                    rack_power,
                ),
            )
            .add_observer(rack_removed)
            .add_observer(silicon_removed);
    }
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn operational(world: &World, rack: Entity) -> bool {
    world.get::<Unpowered>(rack).is_none()
}

fn rack_sound(world: &mut World, rack: Entity, sound: &str) {
    if let Some(&pos) = world.get::<TilePos>(rack) {
        playsound(world, PlaySound::new(pos, sound, 50.0, true));
    }
}

/// The round's default lawset id (tg `get_round_default_lawset()::id`).
fn round_default_id(data: &SiliconData) -> &str {
    SPECIFIED_LAWS
        .iter()
        .copied()
        .find(|id| data.lawset(id).is_some())
        .unwrap_or(FALLBACK_LAWS)
}

/// Mapped racks get their state; core racks their designation and the round's default module,
/// welded in (tg `/obj/machinery/ai_law_rack/base/core/Initialize(mapload)`).
fn init_racks(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &DmObj), Without<LawRack>>();
    let racks: Vec<(Entity, String)> = query
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, RACK_ROOT))
        .map(|(e, o)| (e, o.path.clone()))
        .collect();
    let mut designations = DESIGNATIONS.iter();
    for (rack, path) in racks {
        let Some((kind, slots, core_slot)) = rack_spec(&path) else {
            continue;
        };
        world
            .entity_mut(rack)
            .insert(LawRack::new(kind, slots, core_slot));
        if !is_under(&path, CORE_RACK) {
            continue;
        }
        if let Some(letter) = designations.next() {
            let name = format!("{} '{letter}'", name_of(world, rack));
            world.entity_mut(rack).insert(Name::new(name));
        }
        load_config_law(world, rack);
    }
}

/// tg `load_config_law`: the first full core module of the default lawset, welded in.
fn load_config_law(world: &mut World, rack: Entity) {
    let path = {
        let data = &world.resource::<Silicons>().0;
        let id = round_default_id(data);
        data.modules
            .iter()
            .find(|m| is_under(&m.path, FULL_MODULE) && m.law_id.as_deref() == Some(id))
            .map(|m| m.path.clone())
    };
    let Some(path) = path else {
        return;
    };
    let Some(module) = crate::mecha::core::spawn_inside(world, &path, rack) else {
        return;
    };
    ensure_module(world, module);
    let name = name_of(world, module);
    if let Some(mut r) = world.get_mut::<LawRack>(rack) {
        r.slots[0] = Some(Slot {
            module,
            name,
            security: Security::Welded,
        });
    }
    recompute(world, rack);
}

/// tg `get_law_affecting_modules`: the rack's law modules, then those of its operational
/// broadcasters, each once.
fn affecting_modules(world: &World, rack: Entity) -> Vec<Entity> {
    let Some(r) = world.get::<LawRack>(rack) else {
        return Vec::new();
    };
    let mut out: Vec<Entity> = r.modules().collect();
    for &child in &r.linked_racks {
        if !operational(world, child) {
            continue;
        }
        if let Some(c) = world.get::<LawRack>(child) {
            for m in c.modules() {
                if !out.contains(&m) {
                    out.push(m);
                }
            }
        }
    }
    out.retain(|&m| {
        world
            .get::<LawModule>(m)
            .is_some_and(|l| l.kind != ModuleKind::Other)
    });
    out
}

/// The combined lawset from the modules, without pushing it to anyone.
fn recompute(world: &mut World, rack: Entity) {
    let mut laws = Laws {
        name: "Inherent Laws".to_owned(),
        ..default()
    };
    for module in affecting_modules(world, rack) {
        if let Some(m) = world.get::<LawModule>(module) {
            m.apply(&mut laws);
        }
    }
    if let Some(mut r) = world.get_mut::<LawRack>(rack) {
        r.combined = laws;
    }
}

/// tg `update_lawset` of `rack`'s parent: refreshes the combined lawset and syncs it to every
/// linked silicon, unless the base rack is depowered.
pub fn update_parent_lawset(world: &mut World, rack: Entity) {
    let Some(base) = world.get::<LawRack>(rack).and_then(|r| r.parent_rack(rack)) else {
        return;
    };
    if !operational(world, base) {
        return;
    }
    recompute(world, base);
    let mobs: Vec<Entity> = world
        .get::<LawRack>(base)
        .map(|r| r.linked_mobs.iter().filter_map(|(_, m)| *m).collect())
        .unwrap_or_default();
    for mob in mobs {
        apply_lawset(world, base, mob, true);
    }
}

/// tg `apply_lawset`: the rack's lawset replaces the silicon's (a protected law 0 stays).
fn apply_lawset(world: &mut World, rack: Entity, mob: Entity, announce: bool) {
    let Some(combined) = world.get::<LawRack>(rack).map(|r| r.combined.clone()) else {
        return;
    };
    let Some(old) = world.get::<Laws>(mob).cloned() else {
        return;
    };
    let mut new = old.clone();
    if !(old.protected_zeroth && old.zeroth.is_some()) {
        new.zeroth = combined.zeroth.clone();
        new.zeroth_borg = combined.zeroth_borg.clone();
    }
    new.hacked = combined.hacked;
    new.inherent = combined.inherent;
    new.supplied = combined.supplied;
    if new == old {
        return;
    }
    world.entity_mut(mob).insert(new);
    if announce {
        announce_law_change(world, mob);
    }
}

/// tg `announce_law_change`: the warning, then the laws (a cyborg's with its master line).
pub fn announce_law_change(world: &mut World, mob: Entity) {
    const TEXT: &str = "Ваши законы были изменены.";
    if let Some(eye) = world.get::<AiCore>(mob).map(|c| c.eye) {
        tell_ai(world, mob, Span::Danger, TEXT);
        let client = world.get::<crate::server::PlayerMob>(eye).map(|p| p.client);
        if let (Some(client), Some(laws)) = (client, world.get::<Laws>(mob).cloned()) {
            super::show_laws(world, client, &laws);
        }
    } else if world.get::<Cyborg>(mob).is_some() {
        tell_mob(world, mob, Span::Danger, TEXT);
        super::chassis::show_laws(world, mob);
    }
}

/// tg `can_link_to`.
pub fn can_link_to(world: &World, rack: Entity, mob: Entity) -> bool {
    let same_z = world
        .get::<TilePos>(rack)
        .zip(world.get::<TilePos>(mob))
        .is_some_and(|(a, b)| a.z == b.z);
    if !same_z || world.get::<LinkedRack>(mob).is_some() {
        return false;
    }
    if let Some(core) = world.get::<AiCore>(mob) {
        return !core.control_disabled;
    }
    if world.get::<Cyborg>(mob).is_some() {
        return world
            .get::<Chassis>(mob)
            .is_none_or(|c| !c.emagged && !c.scrambledcodes);
    }
    false
}

/// tg `link_silicon`.
pub fn link_silicon(world: &mut World, rack: Entity, mob: Entity, announce: bool) -> bool {
    if world
        .get::<LawRack>(rack)
        .is_none_or(|r| r.kind != RackKind::Base)
        || !can_link_to(world, rack, mob)
    {
        return false;
    }
    let name = name_of(world, mob);
    if let Some(mut r) = world.get_mut::<LawRack>(rack) {
        r.linked_mobs.push((name, Some(mob)));
    }
    world.entity_mut(mob).insert(LinkedRack(rack));
    apply_lawset(world, rack, mob, announce);
    refresh_linkable(world, rack);
    true
}

/// tg `link_to_first_rack`: the first core rack that takes `mob`.
pub fn link_to_first_rack(world: &mut World, mob: Entity) -> Option<Entity> {
    let mut query = world.query::<(Entity, &LawRack, &DmObj)>();
    let mut racks: Vec<Entity> = query
        .iter(world)
        .filter(|(_, r, o)| r.kind == RackKind::Base && is_under(&o.path, CORE_RACK))
        .map(|(e, ..)| e)
        .collect();
    racks.sort();
    let rack = racks.into_iter().find(|&r| can_link_to(world, r, mob))?;
    link_silicon(world, rack, mob, false).then_some(rack)
}

/// tg `unlink_silicon`: the silicon's laws are wiped (a protected law 0 stays) and announced.
pub fn unlink_silicon(world: &mut World, rack: Entity, mob: Entity) {
    forget_link(world, mob);
    if let Some(mut laws) = world.get::<Laws>(mob).cloned() {
        laws.clear();
        world.entity_mut(mob).insert(laws);
        announce_law_change(world, mob);
    }
    refresh_linkable(world, rack);
}

/// The silicon leaves its rack's list, its laws untouched (an emag takes the cyborg over).
pub fn forget_link(world: &mut World, mob: Entity) {
    let Some(LinkedRack(rack)) = world.get::<LinkedRack>(mob).copied() else {
        return;
    };
    if let Some(mut r) = world.get_mut::<LawRack>(rack) {
        r.linked_mobs.retain(|(_, m)| *m != Some(mob));
    }
    world.entity_mut(mob).remove::<LinkedRack>();
}

/// tg `link_child_law_rack`.
fn link_child(world: &mut World, base: Entity, child: Entity) {
    if base == child
        || world
            .get::<LawRack>(base)
            .is_none_or(|r| r.linked_racks.contains(&child))
    {
        return;
    }
    let name = name_of(world, base);
    if let Some(mut r) = world.get_mut::<LawRack>(base) {
        r.linked_racks.push(child);
    }
    if let Some(mut c) = world.get_mut::<LawRack>(child) {
        c.parent = Some((base, name));
    }
    update_parent_lawset(world, base);
}

/// tg `unlink_child_law_rack`.
fn unlink_child(world: &mut World, base: Entity, child: Entity) {
    if let Some(mut r) = world.get_mut::<LawRack>(base) {
        r.linked_racks.retain(|&c| c != child);
    }
    if let Some(mut c) = world.get_mut::<LawRack>(child) {
        c.parent = None;
    }
    update_parent_lawset(world, base);
}

/// tg `refresh_linkable_lists`.
fn refresh_linkable(world: &mut World, rack: Entity) {
    let Some((kind, children)) = world
        .get::<LawRack>(rack)
        .map(|r| (r.kind, r.linked_racks.clone()))
    else {
        return;
    };
    match kind {
        RackKind::Plain => {}
        RackKind::Base => {
            let mut silicons = world
                .query_filtered::<Entity, (With<super::Silicon>, Or<(With<AiCore>, With<Cyborg>)>)>(
                );
            let mut candidates: Vec<Entity> = silicons.iter(world).collect();
            candidates.sort();
            let linkable: Vec<(String, Entity)> = candidates
                .into_iter()
                .filter(|&m| can_link_to(world, rack, m))
                .map(|m| (name_of(world, m), m))
                .collect();
            let count = children
                .iter()
                .filter(|&&c| operational(world, c))
                .map(|&c| {
                    world.get::<LawRack>(c).map_or(0, |c| {
                        c.modules()
                            .filter(|&m| {
                                world
                                    .get::<LawModule>(m)
                                    .is_some_and(|l| l.kind != ModuleKind::Other)
                            })
                            .count() as u32
                    })
                })
                .sum::<u32>();
            if let Some(mut r) = world.get_mut::<LawRack>(rack) {
                r.linkable = linkable;
                r.linked_module_count = count;
            }
        }
        RackKind::Broadcaster => {
            let z = world.get::<TilePos>(rack).map(|p| p.z);
            let mut racks = world.query::<(Entity, &LawRack, &TilePos)>();
            let mut bases: Vec<Entity> = racks
                .iter(world)
                .filter(|(_, r, p)| r.kind == RackKind::Base && Some(p.z) == z)
                .map(|(e, ..)| e)
                .collect();
            bases.sort();
            let linkable: Vec<(String, Entity)> = bases
                .into_iter()
                .filter(|&b| operational(world, b))
                .map(|b| (name_of(world, b), b))
                .collect();
            if let Some(mut r) = world.get_mut::<LawRack>(rack) {
                r.linkable = linkable;
            }
        }
    }
}

/// tg `ui_interact`: silicons are refused; the linkable lists refresh when off cooldown.
pub fn open(world: &mut World, client: Entity, user: Entity, rack: Entity) {
    if super::interact::has_silicon_access(world, user) {
        tell(
            world,
            client,
            Span::Warning,
            "Your programming forbids you from using this.",
        );
        return;
    }
    if world
        .get::<LawRack>(rack)
        .is_some_and(|r| now(world) >= r.refresh_ready_at)
    {
        refresh_linkable(world, rack);
    }
    world.write_message(OpenUi::new::<LawRackUi>(client, user, rack, "Module Rack"));
}

/// An empty hand on a rack (`attack_hand` -> `ui_interact`, `INTERACT_MACHINE_OFFLINE`).
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<LawRack>(target).is_none() {
        return false;
    }
    open(world, client, mob, target);
    true
}

/// `attack_ai` / `attack_robot` of a rack: the refusal.
pub fn attack_ai(world: &mut World, client: Entity, user: Entity, target: Entity) -> bool {
    if world.get::<LawRack>(target).is_none() {
        return false;
    }
    open(world, client, user, target);
    true
}

/// tg `item_interaction` with a module and the screwdriver, welder and multitool acts: the
/// window.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<LawRack>(target).is_none() {
        return false;
    }
    let tool = tool_of(world, held).map(|(k, _)| k);
    let opens = item_path(world, held).is_some_and(|p| is_under(&p, AI_MODULE_ROOT))
        || matches!(
            tool,
            Some(ToolKind::Screwdriver | ToolKind::Welder | ToolKind::Multitool)
        );
    if !opens {
        return false;
    }
    open(world, client, mob, target);
    true
}

/// tg `/obj/item/ai_module/law/attack_self`: the programmed laws, then `configure`.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if !ensure_module(world, item) {
        return false;
    }
    let Some(module) = world.get::<LawModule>(item).cloned() else {
        return false;
    };
    let laws: Vec<&str> = module.laws.iter().map(|(l, _)| l.as_str()).collect();
    if !laws.is_empty() {
        let mut text = format!("Programmed Law{}:", if laws.len() > 1 { "s" } else { "" });
        for law in &laws {
            text.push_str(&format!("\n\"{law}\""));
        }
        tell(world, client, Span::Bold, text);
    }
    let Some(kind) = item_path(world, item).as_deref().and_then(configure_of) else {
        return true;
    };
    if module.ioned {
        return true;
    }
    let (title, message, default, max_length, multiline) = match kind {
        Configure::FreeformCore => (
            "Freeform Law Entry",
            "Enter a new core law for the AI.",
            module.laws.first().map(|l| l.0.clone()).unwrap_or_default(),
            MAX_LAW_LEN,
            true,
        ),
        Configure::Freeform => (
            "Freeform Law Entry",
            "Enter a new law for the AI.",
            module.laws.first().map(|l| l.0.clone()).unwrap_or_default(),
            MAX_LAW_LEN,
            true,
        ),
        Configure::Hacked => (
            "Freeform Law Entry",
            "Enter a new law for the AI",
            module.laws.first().map(|l| l.0.clone()).unwrap_or_default(),
            MAX_LAW_LEN,
            true,
        ),
        Configure::Safeguard => (
            "Safeguard",
            "Subject to safeguard.",
            name_of(world, mob),
            MAX_NAME_LEN,
            false,
        ),
        Configure::OneHuman => (
            "Один человек",
            "Введите имя субъекта, который является единственным человеком.",
            name_of(world, mob),
            MAX_NAME_LEN,
            false,
        ),
        Configure::Asimov => (
            "Asimov",
            "Введите имя нового ОБЪЕКТА, волнующую Азимова",
            if module.target_name.is_empty() {
                "human being".to_owned()
            } else {
                module.target_name.clone()
            },
            MAX_NAME_LEN,
            false,
        ),
    };
    let prompt = ss13_tgui::prompt::ask_world(
        world,
        Ask::new(
            client,
            mob,
            title,
            message,
            PromptKind::Text {
                default,
                max_length,
                multiline,
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(ModulePrompt { item, mob, kind });
    true
}

fn holds(world: &World, mob: Entity, item: Entity) -> bool {
    world
        .get::<Inventory>(mob)
        .is_some_and(|i| i.hands.contains(&Some(item)))
}

/// The typed text, at the moment of effect: the module is still in the user's hands
/// (`user.is_holding(src)`).
fn module_configured(world: &mut World, mut answers: Local<MessageCursor<PromptAnswered>>) {
    let pending: Vec<(Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.answer.clone()))
        .collect();
    for (prompt, answer) in pending {
        let Some(ask) = world.get::<ModulePrompt>(prompt).copied() else {
            continue;
        };
        let Some(PromptAnswer::Text(text)) = answer else {
            continue;
        };
        if text.trim().is_empty() || !holds(world, ask.mob, ask.item) {
            continue;
        }
        if let Some(mut module) = world.get_mut::<LawModule>(ask.item) {
            configured(&mut module, ask.kind, &text);
        }
    }
}

/// A parsed `LawRack` act; slots are 1-based as sent, clamped by the handler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RackAct {
    Insert(usize),
    Remove(usize),
    Screw(usize),
    Weld(usize),
    Multitool(usize),
    LinkSilicon(Entity),
    UnlinkSilicon(usize),
    LinkRack(Entity),
    UnlinkRack,
    Refresh,
}

fn parse_ref(params: &UiParams, key: &str) -> Result<Entity, UiReject> {
    params
        .get(key)
        .and_then(|r| r.parse::<u64>().ok())
        .and_then(Entity::try_from_bits)
        .ok_or_else(|| UiReject::BadParams(format!("{key}: not a ref")))
}

fn parse_index(params: &UiParams, key: &str) -> Result<usize, UiReject> {
    let n = params.number(key)?;
    if n < 0.0 || n > 1000.0 {
        return Err(UiReject::BadParams(format!("{key}: out of range")));
    }
    Ok(n as usize)
}

impl ServerUi for LawRackUi {
    type Source = (&'static LawRack, Has<Unpowered>);
    type UserParam = (
        Query<'static, 'static, &'static Inventory>,
        Query<'static, 'static, &'static DmItem>,
        Query<'static, 'static, &'static DmTool>,
        Query<'static, 'static, &'static LawModule>,
        Res<'static, Time>,
    );
    type Act = RackAct;

    fn static_data(_: (&LawRack, bool)) {}

    fn data(
        (rack, unpowered): (&LawRack, bool),
        user: Entity,
        (inventories, items, tools, modules, time): &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> LawRackData {
        let held = inventories.get(user).ok().and_then(Inventory::active_item);
        let tool = held
            .and_then(|h| tools.get(h).ok())
            .and_then(|t| ToolKind::from_dm(&t.behaviour));
        let slots = rack
            .slots
            .iter()
            .map(|slot| match slot {
                None => LawSlotData {
                    name: "Empty".to_owned(),
                    security: SLOT_UNSECURED,
                    ioned: false,
                    empty: true,
                },
                Some(s) => LawSlotData {
                    name: if s.security == Security::Welded {
                        "unidentified welded module".to_owned()
                    } else {
                        s.name.clone()
                    },
                    security: s.security.code(),
                    ioned: modules.get(s.module).is_ok_and(|m| m.ioned),
                    empty: false,
                },
            })
            .collect();
        let links = |list: &[(String, Entity)]| -> Vec<Linkable> {
            list.iter()
                .map(|(name, e)| Linkable {
                    name: name.clone(),
                    link_ref: e.to_bits().to_string(),
                })
                .collect()
        };
        let base = rack.kind == RackKind::Base;
        let broadcaster = rack.kind == RackKind::Broadcaster;
        LawRackData {
            slots,
            holding_module: held
                .and_then(|h| items.get(h).ok())
                .is_some_and(|i| is_under(&i.path, AI_MODULE_ROOT)),
            holding_screwdriver: tool == Some(ToolKind::Screwdriver),
            holding_welder: tool == Some(ToolKind::Welder),
            holding_multitool: tool == Some(ToolKind::Multitool),
            has_core_slot: rack.has_core_slot,
            depowered: unpowered,
            refresh_cooldown: (rack.refresh_ready_at - time.elapsed_secs_f64())
                .max(0.0)
                .ceil() as u32,
            parent_rack: rack.parent.as_ref().map(|(e, name)| Linkable {
                name: name.clone(),
                link_ref: e.to_bits().to_string(),
            }),
            linkable_racks: broadcaster.then(|| links(&rack.linkable)),
            linked_mobs: base.then(|| rack.linked_mobs.iter().map(|(n, _)| n.clone()).collect()),
            linkable_silicons: base.then(|| links(&rack.linkable)),
            linked_rack_module_count: base.then_some(rack.linked_module_count),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<RackAct, UiReject> {
        Ok(match action {
            INSERT_MODULE => RackAct::Insert(parse_index(params, "slot")?),
            REMOVE_MODULE => RackAct::Remove(parse_index(params, "slot")?),
            SCREW_MODULE => RackAct::Screw(parse_index(params, "slot")?),
            WELD_MODULE => RackAct::Weld(parse_index(params, "slot")?),
            MULTITOOL_MODULE => RackAct::Multitool(parse_index(params, "slot")?),
            LINK_SILICON => RackAct::LinkSilicon(parse_ref(params, "silicon_ref")?),
            UNLINK_SILICON => RackAct::UnlinkSilicon(parse_index(params, "silicon_index")?),
            LINK_RACK => RackAct::LinkRack(parse_ref(params, "rack_ref")?),
            UNLINK_RACK => RackAct::UnlinkRack,
            REFRESH => RackAct::Refresh,
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

fn rack_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<LawRackUi>>>) {
    let acts: Vec<(Entity, Entity, Entity, RackAct)> = acts
        .read(world.resource::<Messages<UiActed<LawRackUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act))
        .collect();
    for (client, actor, rack, act) in acts {
        // `if(!istype(user) || issilicon(user)) return`.
        if super::interact::has_silicon_access(world, actor) {
            continue;
        }
        let Some(len) = world.get::<LawRack>(rack).map(|r| r.slots.len()) else {
            continue;
        };
        // `clamp(text2num(params["slot"]), 1, length(ai_modules))`, 0-based.
        let slot = |n: usize| n.clamp(1, len.max(1)) - 1;
        match act {
            RackAct::Insert(n) => insert(world, client, actor, rack, slot(n)),
            RackAct::Remove(n) => remove(world, client, actor, rack, slot(n)),
            RackAct::Screw(n) => {
                start_step(world, client, actor, rack, slot(n), ToolKind::Screwdriver)
            }
            RackAct::Weld(n) => start_step(world, client, actor, rack, slot(n), ToolKind::Welder),
            RackAct::Multitool(n) => {
                start_step(world, client, actor, rack, slot(n), ToolKind::Multitool);
            }
            RackAct::LinkSilicon(mob) => {
                let listed = world.get::<LawRack>(rack).is_some_and(|r| {
                    r.kind == RackKind::Base && r.linkable.iter().any(|(_, e)| *e == mob)
                });
                if !operational(world, rack) || !listed {
                    continue;
                }
                if link_silicon(world, rack, mob, true) {
                    let name = name_of(world, mob);
                    tell(world, client, Span::Notice, format!("linked to {name}"));
                    rack_sound(world, rack, TERMINAL_ON);
                    info!(
                        "{} ({actor}) linked {name} to {}",
                        name_of(world, actor),
                        name_of(world, rack)
                    );
                }
            }
            RackAct::UnlinkSilicon(index) => {
                if !operational(world, rack) {
                    continue;
                }
                let Some((name, mob)) = world.get::<LawRack>(rack).and_then(|r| {
                    let n = r.linked_mobs.len();
                    (r.kind == RackKind::Base && n > 0)
                        .then(|| r.linked_mobs[index.min(n - 1)].clone())
                }) else {
                    continue;
                };
                match mob {
                    Some(mob) => unlink_silicon(world, rack, mob),
                    None => {
                        if let Some(mut r) = world.get_mut::<LawRack>(rack) {
                            r.linked_mobs.retain(|(n, m)| !(m.is_none() && *n == name));
                        }
                    }
                }
                tell(world, client, Span::Notice, format!("unlinked {name}"));
                rack_sound(world, rack, TERMINAL_OFF);
                info!(
                    "{} ({actor}) unlinked {name} from {}",
                    name_of(world, actor),
                    name_of(world, rack)
                );
            }
            RackAct::LinkRack(base) => {
                let ok = world.get::<LawRack>(rack).is_some_and(|r| {
                    r.kind == RackKind::Broadcaster
                        && r.parent.is_none()
                        && r.linkable.iter().any(|(_, e)| *e == base)
                }) && world
                    .get::<LawRack>(base)
                    .is_some_and(|b| b.kind == RackKind::Base);
                if !operational(world, rack) || !ok {
                    continue;
                }
                if !operational(world, base) {
                    tell(world, client, Span::Warning, "failed to link!");
                    continue;
                }
                link_child(world, base, rack);
                let name = name_of(world, base);
                tell(world, client, Span::Notice, format!("linked to {name}"));
                rack_sound(world, rack, TERMINAL_ON);
            }
            RackAct::UnlinkRack => {
                let Some(base) = world
                    .get::<LawRack>(rack)
                    .filter(|r| r.kind == RackKind::Broadcaster)
                    .and_then(|r| r.parent.as_ref().map(|(p, _)| *p))
                else {
                    continue;
                };
                if !operational(world, rack) {
                    continue;
                }
                if !operational(world, base) {
                    tell(world, client, Span::Warning, "failed to unlink!");
                    continue;
                }
                unlink_child(world, base, rack);
                let name = name_of(world, base);
                tell(world, client, Span::Notice, format!("unlinked from {name}"));
                rack_sound(world, rack, TERMINAL_OFF);
            }
            RackAct::Refresh => {
                let t = now(world);
                if !operational(world, rack)
                    || world
                        .get::<LawRack>(rack)
                        .is_none_or(|r| t < r.refresh_ready_at)
                {
                    continue;
                }
                if let Some(mut r) = world.get_mut::<LawRack>(rack) {
                    r.refresh_ready_at = t + REFRESH_COOLDOWN_SECS;
                }
                refresh_linkable(world, rack);
            }
        }
    }
}

/// tg `insert_module`.
fn insert(world: &mut World, client: Entity, user: Entity, rack: Entity, index: usize) {
    let held = world
        .get::<Inventory>(user)
        .and_then(Inventory::active_item);
    let Some(module) = held.filter(|&h| ensure_module(world, h)) else {
        tell(
            world,
            client,
            Span::Warning,
            "You need to hold an AI module to insert it!",
        );
        return;
    };
    let path = item_path(world, module).unwrap_or_default();
    if let Some(refusal) = world
        .get::<LawModule>(module)
        .and_then(|m| install_refusal(&path, m))
    {
        tell(world, client, Span::Warning, refusal);
        return;
    }
    let Some((has_core_slot, free)) = world.get::<LawRack>(rack).map(|r| {
        (
            r.has_core_slot,
            r.slots.get(index).is_some_and(Option::is_none),
        )
    }) else {
        return;
    };
    let rack_name = name_of(world, rack);
    let is_core = module_kind(&path) == ModuleKind::Core;
    if is_core && !has_core_slot {
        tell(
            world,
            client,
            Span::Warning,
            format!("{rack_name} has no slots for core modules!"),
        );
        return;
    }
    if is_core && index != 0 {
        tell(
            world,
            client,
            Span::Warning,
            "You can't install a core module in a non-core slot!",
        );
        return;
    }
    if !is_core && has_core_slot && index == 0 {
        tell(
            world,
            client,
            Span::Warning,
            "You can only install core modules in the core slot!",
        );
        return;
    }
    // tg overwrites an occupied slot (the window offers insertion only into empty ones).
    if !free {
        return;
    }
    let name = name_of(world, module);
    crate::storage::put_inside(world, module, rack);
    if let Some(mut r) = world.get_mut::<LawRack>(rack) {
        r.slots[index] = Some(Slot {
            module,
            name: name.clone(),
            security: Security::Unsecured,
        });
    }
    tell(
        world,
        client,
        Span::Notice,
        format!("inserted slot {}", index + 1),
    );
    rack_sound(world, rack, DECONSTRUCT_SOUND);
    info!(
        "{} ({user}) installed {name} into {rack_name}",
        name_of(world, user)
    );
    update_parent_lawset(world, rack);
}

/// tg `remove_module`: only an unsecured module comes out, into the hand.
fn remove(world: &mut World, client: Entity, user: Entity, rack: Entity, index: usize) {
    let Some(r) = world.get::<LawRack>(rack) else {
        return;
    };
    let Some(slot) = r.slots.get(index).cloned().flatten() else {
        return;
    };
    if slot.security != Security::Unsecured {
        return;
    }
    // `pre_user_uninstall_from_rack` of a core module in a base rack's core slot.
    let core_of_base = r.kind == RackKind::Base
        && r.has_core_slot
        && index == 0
        && world
            .get::<LawModule>(slot.module)
            .is_some_and(|m| m.kind == ModuleKind::Core);
    let mobs: Vec<Entity> = r.linked_mobs.iter().filter_map(|(_, m)| *m).collect();
    if core_of_base {
        for mob in mobs {
            let text = "Core module removed. Recalculating directives...";
            if world.get::<AiCore>(mob).is_some() {
                tell_ai(world, mob, Span::Danger, text);
            } else {
                tell_mob(world, mob, Span::Danger, text);
            }
        }
    }
    if let Some(mut r) = world.get_mut::<LawRack>(rack) {
        r.slots[index] = None;
    }
    match world.get::<Inventory>(user).cloned() {
        Some(inventory) => crate::storage::put_in_hands(world, user, &inventory, slot.module),
        None => {
            if let Some(&pos) = world.get::<TilePos>(rack) {
                crate::storage::take_out(world, slot.module, pos);
            }
        }
    }
    tell(
        world,
        client,
        Span::Notice,
        format!("removed slot {}", index + 1),
    );
    rack_sound(world, rack, DECONSTRUCT_SOUND);
    info!(
        "{} ({user}) removed {} from {}",
        name_of(world, user),
        slot.name,
        name_of(world, rack)
    );
    update_parent_lawset(world, rack);
}

/// The screwdriver, welder and multitool acts: their checks, then the timed step.
fn start_step(
    world: &mut World,
    client: Entity,
    user: Entity,
    rack: Entity,
    index: usize,
    wanted: ToolKind,
) {
    let Some(slot) = world
        .get::<LawRack>(rack)
        .and_then(|r| r.slots.get(index).cloned().flatten())
    else {
        return;
    };
    let Some(tool) = world
        .get::<Inventory>(user)
        .and_then(Inventory::active_item)
    else {
        return;
    };
    if tool_of(world, tool).map(|(k, _)| k) != Some(wanted) || world.get::<DoAfter>(user).is_some()
    {
        return;
    }
    let actor = Actor {
        client,
        mob: user,
        item: Some(tool),
    };
    let module = slot.module;
    let n = index + 1;
    match wanted {
        ToolKind::Screwdriver => {
            if slot.security == Security::Welded {
                return;
            }
            let un = if slot.security == Security::Screwed {
                "un"
            } else {
                ""
            };
            tell(
                world,
                client,
                Span::Notice,
                format!("{un}screwing slot {n}..."),
            );
            crate::tools::use_tool(
                world,
                actor,
                rack,
                SCREW_DS,
                0,
                Job::LawRack(LawRackJob::Screw {
                    slot: index,
                    module,
                }),
            );
        }
        ToolKind::Welder => {
            if slot.security == Security::Unsecured {
                return;
            }
            if !crate::tools::tool_usable(world, tool, 1, HIGH_TEMPERATURE_REQUIRED) {
                let text = crate::tools::unusable_message(world, tool, 1);
                tell(world, client, Span::Warning, text);
                return;
            }
            let un = if slot.security == Security::Welded {
                "un"
            } else {
                ""
            };
            tell(
                world,
                client,
                Span::Notice,
                format!("{un}welding slot {n}..."),
            );
            crate::tools::use_tool(
                world,
                actor,
                rack,
                WELD_DS,
                1,
                Job::LawRack(LawRackJob::Weld {
                    slot: index,
                    module,
                }),
            );
        }
        ToolKind::Multitool => {
            // tg `multitool_act`: `if(!ioned) return NONE`.
            if world.get::<LawModule>(module).is_none_or(|m| !m.ioned) {
                return;
            }
            tell(world, client, Span::Notice, "repairing ion damage...");
            crate::tools::use_tool(
                world,
                actor,
                rack,
                REPAIR_DS,
                0,
                Job::LawRack(LawRackJob::Repair { module }),
            );
        }
        _ => {}
    }
}

/// The step's effect, re-checked: the same module is still in the slot (in the rack) and its
/// security still allows it.
pub fn complete(world: &mut World, _mob: Entity, action: &DoAfter, job: LawRackJob) {
    let rack = action.target;
    let client = action.client;
    let slot_of = |world: &World, index: usize| {
        world
            .get::<LawRack>(rack)
            .and_then(|r| r.slots.get(index).cloned().flatten())
    };
    match job {
        LawRackJob::Screw { slot, module } => {
            let Some(s) = slot_of(world, slot).filter(|s| s.module == module) else {
                return;
            };
            let to = match s.security {
                Security::Unsecured => Security::Screwed,
                Security::Screwed => Security::Unsecured,
                Security::Welded => return,
            };
            set_security(world, rack, slot, to);
        }
        LawRackJob::Weld { slot, module } => {
            let Some(s) = slot_of(world, slot).filter(|s| s.module == module) else {
                return;
            };
            let to = match s.security {
                Security::Screwed => Security::Welded,
                Security::Welded => Security::Screwed,
                Security::Unsecured => return,
            };
            if !action
                .item
                .is_none_or(|i| crate::tools::consume(world, i, action.amount))
            {
                return;
            }
            set_security(world, rack, slot, to);
        }
        LawRackJob::Repair { module } => {
            let in_rack = world
                .get::<LawRack>(rack)
                .is_some_and(|r| r.modules().any(|m| m == module));
            let Some(mut m) = world.get_mut::<LawModule>(module) else {
                return;
            };
            if !in_rack || !m.ioned {
                return;
            }
            m.ioned = false;
            if let Some(saved) = m.saved.take() {
                m.laws = saved;
            }
            tell(world, client, Span::Notice, "module repaired");
            update_parent_lawset(world, rack);
        }
    }
}

fn set_security(world: &mut World, rack: Entity, index: usize, to: Security) {
    if let Some(mut r) = world.get_mut::<LawRack>(rack)
        && let Some(Some(s)) = r.slots.get_mut(index)
    {
        s.security = to;
    }
}

/// tg `on_set_is_operational`: a rack coming back on power (or losing it) updates its parent's
/// lawset. A handful of racks per map: a look at each per tick.
fn rack_power(world: &mut World) {
    let mut query = world.query::<(Entity, &LawRack, Has<Unpowered>)>();
    let flipped: Vec<(Entity, bool)> = query
        .iter(world)
        .filter(|(_, r, unpowered)| r.powered == *unpowered)
        .map(|(e, _, unpowered)| (e, !unpowered))
        .collect();
    for (rack, powered) in flipped {
        if let Some(mut r) = world.get_mut::<LawRack>(rack) {
            r.powered = powered;
        }
        update_parent_lawset(world, rack);
    }
}

/// A deleted rack: a base one unlinks its silicons ("Rack connection lost.") and frees its
/// broadcasters; a broadcaster leaves its base rack (tg `Destroy`, `COMSIG_QDELETING`).
fn rack_removed(remove: On<Remove<LawRack>>, racks: Query<&LawRack>, mut commands: Commands) {
    let Ok(rack) = racks.get(remove.entity) else {
        return;
    };
    let gone = remove.entity;
    let mobs: Vec<Entity> = rack.linked_mobs.iter().filter_map(|(_, m)| *m).collect();
    let children = rack.linked_racks.clone();
    let parent = rack.parent.as_ref().map(|(p, _)| *p);
    commands.queue(move |world: &mut World| {
        for mob in mobs {
            if world.get::<LinkedRack>(mob) != Some(&LinkedRack(gone)) {
                continue;
            }
            world.entity_mut(mob).remove::<LinkedRack>();
            let text = "Rack connection lost. Recalculating directives...";
            if world.get::<AiCore>(mob).is_some() {
                tell_ai(world, mob, Span::Danger, text);
            } else {
                tell_mob(world, mob, Span::Danger, text);
            }
            if let Some(mut laws) = world.get::<Laws>(mob).cloned() {
                laws.clear();
                world.entity_mut(mob).insert(laws);
                announce_law_change(world, mob);
            }
        }
        for child in children {
            if let Some(mut c) = world.get_mut::<LawRack>(child) {
                c.parent = None;
            }
        }
        if let Some(base) = parent {
            if let Some(mut b) = world.get_mut::<LawRack>(base) {
                b.linked_racks.retain(|&c| c != gone);
            }
            update_parent_lawset(world, base);
        }
    });
}

/// A deleted silicon leaves its rack's list (tg `clear_silicon_ref`, here without its name).
fn silicon_removed(
    remove: On<Remove<LinkedRack>>,
    links: Query<&LinkedRack>,
    mut racks: Query<&mut LawRack>,
) {
    let Ok(&LinkedRack(rack)) = links.get(remove.entity) else {
        return;
    };
    if let Ok(mut r) = racks.get_mut(rack) {
        r.linked_mobs.retain(|(_, m)| *m != Some(remove.entity));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_mapload::silicon::{AiModuleDef, Lawset};

    fn data() -> SiliconData {
        SiliconData {
            lawsets: vec![Lawset {
                id: "crewsimovpp".into(),
                name: "Крюзимов++".into(),
                inherent: vec!["c1".into(), "c2".into()],
                ..default()
            }],
            modules: vec![
                AiModuleDef {
                    path: "/obj/item/ai_module/law/core/full/crewsimovpp".into(),
                    laws: vec![],
                    law_id: Some("crewsimovpp".into()),
                },
                AiModuleDef {
                    path: "/obj/item/ai_module/law/supplied/quarantine".into(),
                    laws: vec!["q".into()],
                    law_id: None,
                },
                AiModuleDef {
                    path: FREEFORM.into(),
                    laws: vec!["".into()],
                    law_id: None,
                },
            ],
            ..default()
        }
    }

    #[test]
    fn modules_combine_like_tg() {
        let data = data();
        let core = new_module(&data, "/obj/item/ai_module/law/core/full/crewsimovpp");
        assert_eq!(core.kind, ModuleKind::Core);
        assert_eq!(
            core.laws,
            [("c1".to_owned(), false), ("c2".to_owned(), false)]
        );
        let supplied = new_module(&data, "/obj/item/ai_module/law/supplied/quarantine");
        let mut zeroth = LawModule {
            kind: ModuleKind::Zeroth,
            ..default()
        };
        configured(&mut zeroth, Configure::OneHuman, "Bob");
        let mut hacked = LawModule {
            kind: ModuleKind::Hacked,
            laws: vec![(String::new(), false)],
            ..default()
        };
        configured(&mut hacked, Configure::Hacked, "h");
        let mut laws = Laws::default();
        for m in [&core, &supplied, &zeroth, &hacked] {
            m.apply(&mut laws);
        }
        assert_eq!(
            laws.law_list(true),
            ["0: Только Bob - человек", "#: h", "1: c1", "2: c2", "3: q"]
        );
    }

    #[test]
    fn unconfigured_modules_refuse_installation() {
        let data = data();
        let mut freeform = new_module(&data, FREEFORM);
        assert_eq!(freeform.kind, ModuleKind::Supplied);
        assert!(install_refusal(FREEFORM, &freeform).is_some());
        configured(&mut freeform, Configure::Freeform, "obey");
        assert_eq!(install_refusal(FREEFORM, &freeform), None);
        let safeguard = LawModule::default();
        assert!(install_refusal(SAFEGUARD, &safeguard).is_some());
        assert_eq!(configure_of(ASIMOV_MODULE), Some(Configure::Asimov));
        assert_eq!(configure_of("/obj/item/ai_module/law/core/full/corp"), None);
    }

    #[test]
    fn ion_damage_is_repaired_from_the_saved_laws() {
        let mut m = LawModule {
            kind: ModuleKind::Core,
            laws: vec![("a".into(), false)],
            ..default()
        };
        m.set_ioned(true);
        m.laws = vec![("ion".into(), true)];
        assert_eq!(m.saved.as_deref(), Some(&[("a".to_owned(), false)][..]));
        assert_eq!(module_kind(TOY_AI), ModuleKind::Toy);
        assert_eq!(module_kind(DAMAGED_MODULE), ModuleKind::Core);
        assert_eq!(rack_spec(CORE_RACK), Some((RackKind::Base, 10, true)));
        assert_eq!(rack_spec(SMALL_RACK), Some((RackKind::Base, 3, false)));
        assert_eq!(
            rack_spec(BROADCASTER),
            Some((RackKind::Broadcaster, 3, false))
        );
    }
}
