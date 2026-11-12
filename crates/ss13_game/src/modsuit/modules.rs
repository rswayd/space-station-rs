//! MOD modules (server): tg `/obj/item/mod/module` (`modules/_module.dm`: passive, usable,
//! toggle and active modules, complexity, idle/active/use power, cooldowns, required parts,
//! pins) and the common modules whose mechanics exist here:
//!
//! - storage (`storage`, `large_capacity`, `syndicate`, `belt`, `bluespace`, any module with a
//!   `storage_type`): the control unit becomes a storage with the type's limits; uninstalling
//!   drops what it held;
//! - welding protection: the helmet's `flash_protect` becomes `FLASH_PROTECTION_WELDER_HYPER_
//!   SENSITIVE` while the module's parts are sealed;
//! - flashlight: a light on the wearer while on, its range configurable (2-5), costing
//!   `base_power × range` watts;
//! - magnetic stability (magboots): no slipping on water and ice while on, plus its
//!   `slowdown_active`;
//! - health analyzer: selected, a middle click scans the clicked mob (health or wound mode);
//! - EMP shield: `EMP_PROTECT_ALL` on the unit; the advanced one also protects the wearer and
//!   its contents while sealed.
//! - radiation protection: the parts of a suit with it sealed are radiation-protected clothing
//!   (`crate::radiation`).
//! - ion jetpacks (`jetpack`, `jetpack/advanced`, tg `/datum/component/jetpack`): while on, a
//!   weightless wearer ([`Jetpack`], `crate::spacemove`) steps without a backup, paying
//!   `use_energy_cost` per step; with stabilizers (configurable, on by default) it also stops
//!   drifting, without them it always drifts, `drift_force` times faster; an ion trail follows
//!   it and it is 0.3 ds per tile faster while floating (`jetpack/full_speed`).
//! - medical and security visors: the wearer sees the medical / security data HUD
//!   (`crate::records::hud`, [`VisorHud`]).
//! - megaphone: the wearer speaks loudly (`VoiceSpan`), each line paying `use_energy_cost`.
//! - restraint assist (`quick_cuff`): sealed gauntlets cuff in 0.75 of the time
//!   (`TRAIT_FAST_CUFFING`, [`FastCuffing`]).
//! - holster: used, it takes the gun in the active hand (bulky at most) or draws the holstered
//!   one into the empty active hand ([`Holstered`]); uninstalled, the gun drops.
//! - anti-slip (`noslip`): sealed boots do not slip on water (`TRAIT_NO_SLIP_WATER`,
//!   [`NoSlipWater`]).
//! - bike horn: used, it honks.
//! - supply modules (GPS, clamps, drill, ore bag, loader arms and magnet, disposal selector, ash
//!   accretion, sphere transform): [`super::supply`]; science modules (reagent scanners,
//!   anti-gravity, teleporter, kinesis): [`super::science`]; the pathfinder:
//!   [`super::pathfinder`].
//! - worn overlays (`generate_worn_overlay`): the module's inactive / active / use state drawn
//!   over the part of its first required slot (the unit without required slots) while its parts
//!   are sealed ([`ModOverlays`]).
//!
//! Every other module still installs, counts its complexity, toggles or selects, pays its power
//! and cooldown.
//! - the Spider Clan's modules ([`super::ninja`]): cloaks, hacker, weapon recall, energy net,
//!   adrenaline boost, EMP pulse, star dispenser, DNA locks.
//!
// shortcut: t-ray, tethers, the diagnostic, meson, thermal and night visors and the remaining
// department modules toggle and pay power but have no effect yet (no t-ray reveal, diagnostic HUD,
// vision modes); the radiation counter panel is not drawn; overlays with `mask_worn_overlay` (an
// alpha mask of the part) and BandaStation species overlay files are not drawn; no light color
// picker, the light is omnidirectional (tg: directional), no chemical scan mode effect. Add them
// with their mechanics.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::modsuit::ModuleDef;
use ss13_mapload::{DmClothing, DmItem};
use ss13_shared::chat::Span;
use ss13_shared::protocol::InventorySlot;
use ss13_shared::{Dir, IconStateId, LightSource, Optics};

use super::server::{
    ModDb, ModSuit, ModuleSelected, SHIELD_PROTECTION, buzz, check_charge, name, refresh_buttons,
    set_emp_protection, subtract_charge, tell_mob, theme_of,
};
use super::*;
use crate::emp::{PROTECT_CONTENTS, PROTECT_SELF};
use crate::health::{Health, Stat};
use crate::storage::{Storage, Stored};
use ss13_chat::speech::VoiceSpan;

/// `MODULE_ALLOW_PHASEOUT`, `MODULE_ALLOW_INCAPACITATED`, `MODULE_ALLOW_INACTIVE`,
/// `MODULE_ALLOW_UNWORN`.
pub const MODULE_ALLOW_INCAPACITATED: u32 = 1 << 1;
pub const MODULE_ALLOW_INACTIVE: u32 = 1 << 2;
pub const MODULE_ALLOW_UNWORN: u32 = 1 << 3;
/// `FLASH_PROTECTION_WELDER_HYPER_SENSITIVE`.
const WELDER_HYPER_SENSITIVE: i32 = 4;
/// The flashlight's `base_power`: `DEFAULT_CHARGE_DRAIN * 0.1` watts per tile of range.
const FLASHLIGHT_POWER_PER_RANGE: f64 = 5.0;
/// The health analyzer's `modes`.
const SCAN_MODES: [&str; 3] = ["Health", "Wound", "Chemical"];
/// The jetpacks' `drift_force`, newtons.
const JETPACK_DRIFT_FORCE: f32 = 1.5;
const ADVANCED_JETPACK_DRIFT_FORCE: f32 = 2.0;
/// `/datum/movespeed_modifier/jetpack/full_speed`, deciseconds per tile while floating.
pub const JETPACK_FULL_SPEED: f32 = -0.3;
/// `TRAIT_FAST_CUFFING`: `handcuff_time * 0.75`.
pub const FAST_CUFFING_FACTOR: f32 = 0.75;
/// The megaphone's voice span (`SPAN_COMMAND`; bold, as the handheld megaphone's).
const MEGAPHONE_VOICE: Span = Span::Bold;
const BIKEHORN_SOUND: &str = "sound/items/bikehorn.ogg";
/// The holster's sound both ways.
const HOLSTER_SOUND: &str = "sound/items/weapons/gun/revolver/empty.ogg";
/// `WEIGHT_CLASS_BULKY`: the biggest gun the holster takes.
const HOLSTER_MAX_W_CLASS: u8 = 4;
/// `overlay_icon_file`'s default.
const MODULES_WORN_DMI: &str = "icons/mob/clothing/modsuit/mod_modules.dmi";

/// What a module does here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Storage,
    Welding,
    Flashlight,
    Magboot,
    HealthAnalyzer,
    EmpShield {
        advanced: bool,
    },
    /// `rad_protection`: its parts protect from radiation (`crate::radiation`).
    RadProtection,
    Jetpack {
        advanced: bool,
    },
    /// `visor/sechud`, `visor/medhud`.
    Visor {
        security: bool,
        medical: bool,
    },
    Megaphone,
    QuickCuff,
    Holster,
    NoSlipWater,
    Bikehorn,
    /// Supply modules (`super::supply`).
    Gps,
    Clamp {
        loader: bool,
    },
    Drill,
    OreBag,
    Hydraulic,
    DisposalConnector,
    Magnet,
    AshAccretion,
    Sphere,
    /// Science modules (`super::science`).
    ReagentScanner {
        advanced: bool,
    },
    Antigrav,
    Teleporter,
    Kinesis,
    /// `super::pathfinder`.
    Pathfinder,
    /// The Spider Clan's modules ([`super::ninja`]).
    Stealth {
        ninja: bool,
    },
    Hacker,
    WeaponRecall,
    EnergyNet,
    AdrenalineBoost,
    /// `emp_shield/pulse`: an EMP shield that also pulses.
    EmpPulse,
    StarDispenser,
    DnaLock {
        reinforced: bool,
    },
    /// `demoralizer`: the wearer demoralises those around (`crate::syndie_spraycan`).
    Demoralizer,
    Other,
}

impl Kind {
    pub fn of(def: &ModuleDef) -> Self {
        let p = def.path.as_str();
        let under =
            |root: &str| is_under(p, &format!("{}/{root}", ss13_mapload::modsuit::MODULE_ROOT));
        if def.storage.is_some() {
            Self::Storage
        } else if let Some(kind) = super::ninja::kind_of(def) {
            kind
        } else if under("welding") {
            Self::Welding
        } else if under("flashlight") && def.light.is_some() {
            Self::Flashlight
        } else if under("magboot") {
            Self::Magboot
        } else if under("health_analyzer") {
            Self::HealthAnalyzer
        } else if under("emp_shield/advanced") {
            Self::EmpShield { advanced: true }
        } else if under("emp_shield") {
            Self::EmpShield { advanced: false }
        } else if under("rad_protection") {
            Self::RadProtection
        } else if under("jetpack/advanced") {
            Self::Jetpack { advanced: true }
        } else if under("jetpack") {
            Self::Jetpack { advanced: false }
        } else if under("visor/sechud") {
            Self::Visor {
                security: true,
                medical: false,
            }
        } else if under("visor/medhud") {
            Self::Visor {
                security: false,
                medical: true,
            }
        } else if under("megaphone") {
            Self::Megaphone
        } else if under("quick_cuff") {
            Self::QuickCuff
        } else if under("holster") {
            Self::Holster
        } else if under("noslip") {
            Self::NoSlipWater
        } else if under("bikehorn") {
            Self::Bikehorn
        } else if under("gps") {
            Self::Gps
        } else if under("clamp/loader") {
            Self::Clamp { loader: true }
        } else if under("clamp") {
            Self::Clamp { loader: false }
        } else if under("drill") {
            Self::Drill
        } else if under("orebag") {
            Self::OreBag
        } else if under("hydraulic") {
            Self::Hydraulic
        } else if under("disposal_connector") {
            Self::DisposalConnector
        } else if under("magnet") {
            Self::Magnet
        } else if under("ash_accretion") {
            Self::AshAccretion
        } else if under("sphere_transform") {
            Self::Sphere
        } else if under("reagent_scanner/advanced") {
            Self::ReagentScanner { advanced: true }
        } else if under("reagent_scanner") {
            Self::ReagentScanner { advanced: false }
        } else if under("anomaly_locked/antigrav") {
            Self::Antigrav
        } else if under("anomaly_locked/teleporter") {
            Self::Teleporter
        } else if under("anomaly_locked/kinesis") {
            Self::Kinesis
        } else if under("pathfinder") {
            Self::Pathfinder
        } else if under("demoralizer") {
            Self::Demoralizer
        } else {
            Self::Other
        }
    }
}

/// A module's live state.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct ModModule {
    /// Index into [`ss13_mapload::modsuit::ModData::modules`].
    pub def: usize,
    pub kind: Kind,
    pub active: bool,
    /// `part_activated`: its required parts are sealed and `on_part_activation` ran.
    pub part_activated: bool,
    /// `COOLDOWN_DECLARE(cooldown_timer)`: game seconds.
    pub cooldown_until: f64,
    /// The health analyzer's `mode` (index into its modes).
    pub mode: usize,
    /// The flashlight's `light_range`.
    pub light_range: f32,
    /// The light this module put on the wearer.
    pub lit: Option<(Entity, LightSource)>,
    /// The jetpack's `stabilize`.
    pub stabilize: bool,
}

/// On a wearer: `TRAIT_NO_SLIP_WATER` and `TRAIT_NO_SLIP_ICE` from active magboots.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct NoSlip;

/// On a wearer: an active ion jetpack (tg `/datum/component/jetpack` on the wearer's signals).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Jetpack {
    pub control: Entity,
    pub module: Entity,
    pub stabilize: bool,
    /// Newtons; drifting is this many times faster (`inertia_move_multiplier_active`).
    pub drift_force: f32,
    /// `use_energy_cost`, joules per thrust.
    pub cost: f64,
}

/// On a wearer: the data HUDs of an active visor (`TRAIT_SECURITY_HUD`, `TRAIT_MEDICAL_HUD`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VisorHud {
    pub security: bool,
    pub medical: bool,
}

/// On a wearer: `TRAIT_NO_SLIP_WATER` from the anti-slip module.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct NoSlipWater;

/// On a wearer: `TRAIT_FAST_CUFFING` from the restraint assist module.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct FastCuffing;

/// On a wearer: an active megaphone module (`COMSIG_MOB_SAY` registered).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ModMegaphone {
    pub control: Entity,
    pub module: Entity,
}

/// On a part or the control unit: the worn overlays of the modules drawn on it
/// (`COMSIG_ITEM_GET_SEPARATE_WORN_OVERLAYS`), drawn just above the item's own sprite.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ModOverlays(pub Vec<IconStateId>);

/// Whether `mob` keeps its footing on a wet floor slip of this knockdown (`crate::janitor`:
/// water 6 s, ice 12 s; lube still gets it).
pub fn resists_slip(world: &World, mob: Entity, walking_safe: bool, knockdown: f32) -> bool {
    world.get::<NoSlip>(mob).is_some() && (walking_safe || knockdown >= 12.0)
}

/// Whether the anti-slip module keeps `mob` up on a wet floor of strength `wet` (water only).
pub fn resists_wet_slip(world: &World, mob: Entity, wet: u8) -> bool {
    wet == crate::janitor::TURF_WET_WATER && world.get::<NoSlipWater>(mob).is_some()
}

/// Gives a module item its [`ModModule`] (idempotent).
pub fn init_module(world: &mut World, module: Entity) {
    if world.get::<ModModule>(module).is_some() {
        return;
    }
    let Some(path) = world.get::<DmItem>(module).map(|i| i.path.clone()) else {
        return;
    };
    if !is_under(&path, ss13_mapload::modsuit::MODULE_ROOT) {
        return;
    }
    let Some(db) = world.get_resource::<ModDb>() else {
        return;
    };
    let Some(def) = db.module(&path) else {
        return;
    };
    let module_def = &db.data.modules[def];
    let state = ModModule {
        def,
        kind: Kind::of(module_def),
        active: false,
        part_activated: false,
        cooldown_until: 0.0,
        mode: 0,
        light_range: module_def.light.as_ref().map_or(0.0, |l| l.0),
        lit: None,
        stabilize: true,
    };
    let kind = state.kind;
    world.entity_mut(module).insert(state);
    super::supply::init(world, module, kind);
    super::pathfinder::init(world, module, kind);
}

pub fn def_of(world: &World, module: Entity) -> Option<ModuleDef> {
    let i = world.get::<ModModule>(module)?.def;
    world.get_resource::<ModDb>()?.data.modules.get(i).cloned()
}

fn kind(world: &World, module: Entity) -> Kind {
    world
        .get::<ModModule>(module)
        .map_or(Kind::Other, |m| m.kind)
}

/// tg `has_required_parts`: every `required_slots` entry has one of its slots among `slots`.
pub fn has_required_parts(def: &ModuleDef, slots: u32) -> bool {
    def.required_slots.iter().all(|&needed| needed & slots != 0)
}

fn suit(world: &World, control: Entity) -> Option<ModSuit> {
    world.get::<ModSuit>(control).cloned()
}

use crate::tools::now;

fn tell_opt(world: &mut World, mob: Option<Entity>, span: Span, text: impl Into<String>) {
    if let Some(mob) = mob {
        tell_mob(world, mob, span, text);
    }
}

// ---------------------------------------------------------------------------------------------
// Install hooks

/// `on_install`.
pub fn on_install(world: &mut World, control: Entity, module: Entity) {
    match kind(world, module) {
        Kind::Storage => {
            if let Some(limits) = def_of(world, module).and_then(|d| d.storage) {
                world.entity_mut(control).insert(Storage {
                    max_slots: limits.max_slots,
                    max_specific_storage: limits.max_specific_storage,
                    max_total_storage: limits.max_total_storage,
                });
            }
        }
        Kind::EmpShield { .. } | Kind::EmpPulse => {
            set_emp_protection(world, control, SHIELD_PROTECTION, true)
        }
        k @ (Kind::AdrenalineBoost | Kind::DnaLock { .. }) => {
            super::ninja::on_install(world, control, module, k)
        }
        _ => {}
    }
}

/// `on_uninstall`: the storage's contents fall out (`atom_storage.remove_all(drop_location())`).
pub fn on_uninstall(world: &mut World, control: Entity, module: Entity) {
    match kind(world, module) {
        Kind::Storage => {
            world.entity_mut(control).remove::<Storage>();
            let held: Vec<Entity> = world
                .get::<Stored>(control)
                .map(|s| s.items().to_vec())
                .unwrap_or_default();
            if let Some(pos) = crate::chem::map_pos(world, control) {
                for item in held {
                    crate::storage::take_out(world, item, pos);
                }
            }
        }
        Kind::EmpShield { .. } | Kind::EmpPulse => {
            set_emp_protection(world, control, SHIELD_PROTECTION, false)
        }
        // `holstered.forceMove(mod.drop_location())`.
        Kind::Holster => {
            if let Some(gun) = holstered(world, module)
                && let Some(pos) = crate::chem::map_pos(world, control)
            {
                world.entity_mut(module).remove::<Holstered>();
                crate::storage::take_out(world, gun, pos);
            }
        }
        _ => {}
    }
}

/// The gun in a MOD holster (tg `holstered`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Holstered(pub Entity);

/// The holstered gun, if it is still inside the module (`Exited` forgets it).
fn holstered(world: &World, module: Entity) -> Option<Entity> {
    let gun = world.get::<Holstered>(module)?.0;
    world
        .get::<crate::storage::StoredIn>(gun)
        .is_some_and(|s| s.0 == module)
        .then_some(gun)
}

/// tg `holster/on_use`: the gun in the active hand goes in, or the holstered one comes out.
fn holster_use(world: &mut World, control: Entity, module: Entity) {
    let Some(wearer) = suit(world, control).and_then(|s| s.wearer) else {
        return;
    };
    let Some(inventory) = world.get::<crate::inventory::Inventory>(wearer).cloned() else {
        return;
    };
    let say = |world: &mut World, text: &str| tell_mob(world, wearer, Span::Notice, text);
    let sound = |world: &mut World| {
        if let Some(pos) = crate::chem::map_pos(world, control) {
            crate::sound::playsound(
                world,
                crate::sound::PlaySound::new(pos, HOLSTER_SOUND, 100.0, true),
            );
        }
    };
    match holstered(world, module) {
        None => {
            world.entity_mut(module).remove::<Holstered>();
            let Some(held) = inventory.active_item() else {
                say(world, "нечего убирать в кобуру!");
                return;
            };
            let w_class = world
                .get::<ss13_mapload::DmWeight>(held)
                .map_or(0, |w| w.w_class);
            if world.get::<crate::guns::Gun>(held).is_none() || w_class > HOLSTER_MAX_W_CLASS {
                say(world, "не подходит!");
                return;
            }
            // `transferItemToLoc(force = FALSE)`: not an item stuck to the hand.
            if crate::inventory::no_drop(world, held) {
                return;
            }
            if let Some(mut inv) = world.get_mut::<crate::inventory::Inventory>(wearer) {
                let active = inv.active;
                inv.hands[active] = None;
            }
            crate::storage::put_inside(world, held, module);
            world.entity_mut(module).insert(Holstered(held));
            say(world, "оружие убрано в кобуру");
            sound(world);
        }
        Some(gun) if inventory.active_item().is_none() => {
            world.entity_mut(module).remove::<Holstered>();
            world
                .entity_mut(gun)
                .remove::<(crate::storage::StoredIn, ss13_shared::TilePos)>()
                .insert(crate::inventory::CarriedBy(wearer));
            if let Some(mut inv) = world.get_mut::<crate::inventory::Inventory>(wearer) {
                let active = inv.active;
                inv.hands[active] = Some(gun);
            }
            say(world, "оружие извлечено");
            sound(world);
        }
        Some(_) => say(world, "кобура занята!"),
    }
}

/// The part covering the head (`get_part_from_slot(ITEM_SLOT_HEAD)`, else mask, else eyes).
fn head_cover(suit: &ModSuit) -> Option<(Entity, i32)> {
    [
        InventorySlot::Head,
        InventorySlot::Mask,
        InventorySlot::Eyes,
    ]
    .into_iter()
    .find_map(|slot| suit.parts.iter().find(|p| p.slot == slot))
    .map(|p| (p.item, p.flash_protect))
}

/// `on_part_activation`: every required part is sealed.
pub fn on_part_activation(world: &mut World, control: Entity, module: Entity) {
    let Some(suit) = suit(world, control) else {
        return;
    };
    match kind(world, module) {
        Kind::Welding => {
            if let Some((helmet, _)) = head_cover(&suit)
                && let Some(mut c) = world.get_mut::<DmClothing>(helmet)
            {
                c.flash_protect = WELDER_HYPER_SENSITIVE;
            }
        }
        Kind::EmpShield { advanced: true } => {
            if let Some(wearer) = suit.wearer {
                set_emp_protection(world, wearer, PROTECT_SELF | PROTECT_CONTENTS, true);
            }
        }
        Kind::QuickCuff => {
            if let Some(wearer) = suit.wearer {
                world.entity_mut(wearer).insert(FastCuffing);
            }
        }
        Kind::NoSlipWater => {
            if let Some(wearer) = suit.wearer {
                world.entity_mut(wearer).insert(NoSlipWater);
            }
        }
        Kind::AshAccretion => super::supply::ash_sealed(world, control, module),
        Kind::Demoralizer => {
            if let Some(wearer) = suit.wearer {
                world
                    .entity_mut(wearer)
                    .insert(crate::syndie_spraycan::Demoraliser(
                        &crate::syndie_spraycan::MODULE,
                    ));
            }
        }
        Kind::WeaponRecall => {
            super::ninja::on_part_activation(world, control, module, Kind::WeaponRecall)
        }
        _ => {}
    }
}

/// `on_part_deactivation`.
pub fn on_part_deactivation(world: &mut World, control: Entity, module: Entity) {
    let Some(suit) = suit(world, control) else {
        return;
    };
    match kind(world, module) {
        Kind::Welding => {
            if let Some((helmet, initial)) = head_cover(&suit)
                && let Some(mut c) = world.get_mut::<DmClothing>(helmet)
            {
                c.flash_protect = initial;
            }
        }
        Kind::EmpShield { advanced: true } => {
            if let Some(wearer) = suit.wearer.filter(|&w| world.get_entity(w).is_ok()) {
                set_emp_protection(world, wearer, PROTECT_SELF | PROTECT_CONTENTS, false);
            }
        }
        Kind::QuickCuff => {
            if let Some(wearer) = suit.wearer.filter(|&w| world.get_entity(w).is_ok()) {
                world.entity_mut(wearer).remove::<FastCuffing>();
            }
        }
        Kind::NoSlipWater => {
            if let Some(wearer) = suit.wearer.filter(|&w| world.get_entity(w).is_ok()) {
                world.entity_mut(wearer).remove::<NoSlipWater>();
            }
        }
        Kind::Demoralizer => {
            if let Some(wearer) = suit.wearer.filter(|&w| world.get_entity(w).is_ok()) {
                world
                    .entity_mut(wearer)
                    .remove::<crate::syndie_spraycan::Demoraliser>();
            }
        }
        Kind::Clamp { .. } => super::supply::drop_crates(world, control, module),
        Kind::AshAccretion => super::supply::shed_ash(world, control, module),
        k @ (Kind::WeaponRecall | Kind::EnergyNet) => {
            super::ninja::on_part_deactivation(world, control, module, k)
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------------------------
// Using modules

/// tg `on_select(activator)`: toggles a toggle or active module, uses a usable one.
pub fn on_select(world: &mut World, control: Entity, module: Entity, activator: Entity) {
    let (Some(suit), Some(def)) = (suit(world, control), def_of(world, module)) else {
        return;
    };
    let who = Some(activator);
    let allow_flags = allow_flags(&def, kind(world, module));
    if suit.wearer.is_none() && allow_flags & MODULE_ALLOW_UNWORN == 0 {
        tell_opt(world, who, Span::Warning, "не надет!");
        return;
    }
    let off = !suit.active || suit.activating;
    if (off && allow_flags & (MODULE_ALLOW_INACTIVE | MODULE_ALLOW_UNWORN) == 0)
        || def.module_type == MODULE_PASSIVE
    {
        tell_opt(world, who, Span::Warning, "не активен!");
        return;
    }
    let theme = theme_of(world, &suit);
    if !has_required_parts(&def, suit.part_slots(&theme, true))
        && allow_flags & MODULE_ALLOW_UNWORN == 0
    {
        tell_opt(world, who, Span::Warning, "нужные части не активны!");
        buzz(world, control);
        return;
    }
    if def.module_type == MODULE_USABLE {
        if used(world, control, module, who) {
            on_use(world, control, module, activator);
        }
    } else if world.get::<ModModule>(module).is_some_and(|m| m.active) {
        deactivate(world, control, module, true);
    } else {
        activate(world, control, module, who);
    }
}

/// The module's `allow_flags`. The pathfinder's are written as a `list(...)` in tg, which the
/// converter cannot read as flags; its intent (`MODULE_ALLOW_INACTIVE|MODULE_ALLOW_UNWORN`) is
/// used.
fn allow_flags(def: &ModuleDef, kind: Kind) -> u32 {
    if kind == Kind::Pathfinder {
        def.allow_flags | MODULE_ALLOW_INACTIVE | MODULE_ALLOW_UNWORN
    } else {
        def.allow_flags
    }
}

/// tg `activate(activator)`. True when it turned on.
pub fn activate(
    world: &mut World,
    control: Entity,
    module: Entity,
    activator: Option<Entity>,
) -> bool {
    let (Some(suit), Some(def), Some(state)) = (
        suit(world, control),
        def_of(world, module),
        world.get::<ModModule>(module).cloned(),
    ) else {
        return false;
    };
    if now(world) < state.cooldown_until {
        tell_opt(world, activator, Span::Warning, "на перезарядке!");
        return false;
    }
    if !suit.active || suit.activating || !check_charge(world, control, f64::MIN_POSITIVE) {
        tell_opt(world, activator, Span::Warning, "нет питания!");
        return false;
    }
    let Some(wearer) = suit.wearer else {
        return false;
    };
    // `COMSIG_MODULE_TRIGGERED`.
    if !super::anomalock::may_trigger(world, module, Some(wearer)) {
        return false;
    }
    // `sphere_transform/activate`.
    if state.kind == Kind::Sphere && !super::supply::has_gravity(world, wearer) {
        tell_opt(world, activator, Span::Warning, "нет гравитации!");
        return false;
    }
    if def.module_type == MODULE_ACTIVE {
        if let Some(old) = suit.selected.filter(|&s| s != module) {
            deactivate(world, control, old, false);
        }
        if let Some(mut s) = world.get_mut::<ModSuit>(control) {
            s.selected = Some(module);
        }
        world.entity_mut(wearer).insert(ModuleSelected);
        let text = format!(
            "{} активирован, используйте средней кнопкой мыши",
            name(world, module)
        );
        tell_mob(world, wearer, Span::Notice, text);
    }
    if let Some(mut m) = world.get_mut::<ModModule>(module) {
        m.active = true;
    }
    on_activation(world, control, module);
    refresh_buttons(world, suit.holder);
    true
}

/// tg `deactivate(activator, display_message)`.
pub fn deactivate(world: &mut World, control: Entity, module: Entity, display: bool) -> bool {
    let (Some(suit), Some(def)) = (suit(world, control), def_of(world, module)) else {
        return false;
    };
    if let Some(mut m) = world.get_mut::<ModModule>(module) {
        m.active = false;
    }
    if def.module_type == MODULE_ACTIVE {
        if let Some(mut s) = world.get_mut::<ModSuit>(control)
            && s.selected == Some(module)
        {
            s.selected = None;
        }
        if let Some(wearer) = suit.wearer.filter(|&w| world.get_entity(w).is_ok()) {
            world.entity_mut(wearer).remove::<ModuleSelected>();
            if display {
                let text = format!("{} деактивирован", name(world, module));
                tell_mob(world, wearer, Span::Notice, text);
            }
        }
    }
    on_deactivation(world, control, module);
    refresh_buttons(world, suit.holder);
    true
}

/// tg `used(activator)`: the cooldown starts. True when it could be used.
pub fn used(world: &mut World, control: Entity, module: Entity, activator: Option<Entity>) -> bool {
    let (Some(def), Some(state)) = (
        def_of(world, module),
        world.get::<ModModule>(module).cloned(),
    ) else {
        return false;
    };
    let now = now(world);
    if now < state.cooldown_until {
        tell_opt(world, activator, Span::Warning, "на перезарядке!");
        return false;
    }
    if !check_charge(world, control, def.use_energy_cost) {
        tell_opt(world, activator, Span::Warning, "недостаточно заряда!");
        return false;
    }
    // `COMSIG_MODULE_TRIGGERED`.
    let wearer = suit(world, control).and_then(|s| s.wearer);
    if !super::anomalock::may_trigger(world, module, wearer) {
        return false;
    }
    // The adrenaline boost's own `used()`: its radium.
    if !super::ninja::may_use(world, control, module, state.kind) {
        return false;
    }
    if let Some(mut m) = world.get_mut::<ModModule>(module) {
        m.cooldown_until = now + f64::from(def.cooldown_time) / 10.0;
    }
    true
}

/// tg `on_use(activator)` of the usable modules ported here (the others do nothing).
fn on_use(world: &mut World, control: Entity, module: Entity, activator: Entity) {
    match kind(world, module) {
        Kind::Bikehorn => {}
        Kind::Holster => return holster_use(world, control, module),
        Kind::Gps => return super::supply::gps_use(world, control, module),
        Kind::OreBag => return super::supply::orebag_use(world, control, module),
        Kind::Pathfinder => return super::pathfinder::on_use(world, control, module, activator),
        k => {
            super::ninja::on_use(world, control, module, k);
            return;
        }
    }
    // `playsound(src, 'bikehorn.ogg', 100, FALSE)`.
    if let Some(pos) = crate::chem::map_pos(world, control) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, BIKEHORN_SOUND, 100.0, false),
        );
    }
    let cost = def_of(world, module).map_or(0.0, |d| d.use_energy_cost);
    drain_power(world, control, cost);
}

/// tg `drain_power(amount)`: nothing is taken when there is not enough.
pub(crate) fn drain_power(world: &mut World, control: Entity, amount: f64) -> bool {
    if !check_charge(world, control, amount) {
        return false;
    }
    subtract_charge(world, control, amount);
    true
}

/// What a selected module's middle click is about (tg `on_select_use(target)`).
#[derive(Clone, Copy, Debug)]
pub struct SelectUse {
    pub control: Entity,
    pub module: Entity,
    pub client: Entity,
    pub wearer: Entity,
    pub target: Entity,
}

/// tg `on_select_use(target)` of a selected module without a device: the health analyzer's
/// scan and the modules of `super::supply` and `super::science`.
pub fn on_select_use(
    world: &mut World,
    control: Entity,
    module: Entity,
    client: Entity,
    target: Entity,
) {
    let (Some(suit), Some(def)) = (suit(world, control), def_of(world, module)) else {
        return;
    };
    let Some(wearer) = suit.wearer else {
        return;
    };
    let traits = crate::status_effects::incapacitation(world, wearer);
    if def.allow_flags & MODULE_ALLOW_INCAPACITATED == 0
        && (traits.incapacitated || world.get::<Stat>(wearer) != Some(&Stat::Conscious))
    {
        return;
    }
    let kind = kind(world, module);
    // `sphere_transform/used`: `lavaland_equipment_pressure_check`.
    if kind == Kind::Sphere && !super::supply::low_pressure(world, wearer) {
        tell_mob(world, wearer, Span::Warning, "слишком высокое давление!");
        if let Some(pos) = crate::chem::map_pos(world, control) {
            crate::sound::playsound(
                world,
                crate::sound::PlaySound::new(pos, super::supply::DRY_FIRE, 25.0, true),
            );
        }
        return;
    }
    if !used(world, control, module, Some(wearer)) {
        return;
    }
    let ctx = SelectUse {
        control,
        module,
        client,
        wearer,
        target,
    };
    match kind {
        Kind::Clamp { loader } => return super::supply::clamp(world, ctx, loader),
        Kind::Drill => return super::supply::drill(world, ctx),
        Kind::Hydraulic => return super::supply::hydraulic(world, ctx),
        Kind::Magnet => return super::supply::magnet(world, ctx),
        Kind::Sphere => return super::supply::sphere_fire(world, ctx),
        Kind::Teleporter => return super::science::teleporter(world, ctx),
        Kind::Kinesis => return super::science::kinesis(world, ctx),
        _ => {}
    }
    if super::ninja::on_select_use(world, control, module, kind, target) {
        return;
    }
    if kind != Kind::HealthAnalyzer || world.get::<Health>(target).is_none() {
        return;
    }
    let mode = world.get::<ModModule>(module).map_or(0, |m| m.mode);
    let message = match mode {
        0 => crate::health_scan::healthscan(world, target),
        1 => crate::health_scan::woundscan(world, None, target, false),
        _ => {
            tell_mob(
                world,
                wearer,
                Span::Warning,
                "Химический анализ недоступен.",
            );
            None
        }
    };
    if let Some(message) = message {
        world.write_message(ss13_net::chat::server::SendChat {
            clients: vec![client],
            message,
        });
    }
    drain_power(world, control, def.use_energy_cost);
}

/// `on_activation`.
fn on_activation(world: &mut World, control: Entity, module: Entity) {
    let Some(wearer) = suit(world, control).and_then(|s| s.wearer) else {
        return;
    };
    match kind(world, module) {
        Kind::Flashlight => light_on(world, module, wearer),
        Kind::Magboot => {
            world.entity_mut(wearer).insert(NoSlip);
        }
        Kind::Jetpack { advanced } => {
            let stabilize = world.get::<ModModule>(module).is_none_or(|m| m.stabilize);
            let cost = def_of(world, module).map_or(0.0, |d| d.use_energy_cost);
            let drift_force = if advanced {
                ADVANCED_JETPACK_DRIFT_FORCE
            } else {
                JETPACK_DRIFT_FORCE
            };
            // `setup_trail`: `trail_follow/ion/grav_allowed`, generated only while floating
            // (`should_trigger`), which the plain ion trail checks by itself.
            world.entity_mut(wearer).insert((
                Jetpack {
                    control,
                    module,
                    stabilize,
                    drift_force,
                    cost,
                },
                crate::effects::trail::Trail::new(crate::effects::trail::TrailKind::Ion),
            ));
        }
        Kind::Visor { security, medical } => {
            world
                .entity_mut(wearer)
                .insert(VisorHud { security, medical });
        }
        Kind::Megaphone => {
            world
                .entity_mut(wearer)
                .insert((ModMegaphone { control, module }, VoiceSpan(MEGAPHONE_VOICE)));
        }
        Kind::ReagentScanner { advanced } => {
            world
                .entity_mut(wearer)
                .insert(super::science::ReagentScanner { module, advanced });
        }
        Kind::Antigrav => super::science::antigrav_on(world, control, wearer),
        Kind::Sphere => super::supply::sphere_on(world, control, module, wearer),
        k @ Kind::Stealth { .. } => super::ninja::on_activation(world, control, module, k),
        _ => {}
    }
}

/// Takes off `wearer` what the module's `on_activation` gave it.
fn strip_wearer(world: &mut World, module: Entity, kind: Kind, wearer: Entity) {
    match kind {
        Kind::Magboot => {
            world.entity_mut(wearer).remove::<NoSlip>();
        }
        Kind::Jetpack { .. } => {
            if world
                .get::<Jetpack>(wearer)
                .is_some_and(|j| j.module == module)
            {
                world
                    .entity_mut(wearer)
                    .remove::<(Jetpack, crate::effects::trail::Trail)>();
            }
        }
        Kind::Visor { .. } => {
            world.entity_mut(wearer).remove::<VisorHud>();
        }
        Kind::ReagentScanner { .. }
            if world
                .get::<super::science::ReagentScanner>(wearer)
                .is_some_and(|r| r.module == module) =>
        {
            world
                .entity_mut(wearer)
                .remove::<super::science::ReagentScanner>();
        }
        Kind::Antigrav => super::science::antigrav_off(world, module, wearer),
        Kind::Sphere => super::supply::sphere_off(world, module, wearer),
        Kind::Magnet => super::supply::magnet_off(world, wearer),
        Kind::Stealth { .. } => super::ninja::strip(world, module, kind, wearer),
        Kind::Megaphone
            if world
                .get::<ModMegaphone>(wearer)
                .is_some_and(|m| m.module == module) =>
        {
            {
                world.entity_mut(wearer).remove::<ModMegaphone>();
                // A handheld megaphone in the active hand keeps its voice.
                let handheld = world
                    .get::<crate::inventory::Inventory>(wearer)
                    .and_then(crate::inventory::Inventory::active_item)
                    .and_then(|i| world.get::<crate::devices::Megaphone>(i))
                    .is_some_and(|m| m.voiced == Some(wearer));
                if !handheld {
                    world.entity_mut(wearer).remove::<VoiceSpan>();
                }
            }
        }
        _ => {}
    }
}

/// `on_deactivation`.
fn on_deactivation(world: &mut World, control: Entity, module: Entity) {
    let wearer = suit(world, control).and_then(|s| s.wearer);
    let kind = kind(world, module);
    if kind == Kind::Flashlight {
        light_off(world, module);
    }
    if kind == Kind::Kinesis {
        super::science::clear_grab(world, module, true);
    }
    if let Some(w) = wearer.filter(|&w| world.get_entity(w).is_ok()) {
        strip_wearer(world, module, kind, w);
    }
}

/// `jetpack/stabilize` (`COMSIG_MOVABLE_SPACEMOVE`) with `check_on_move.Invoke(FALSE)`: an
/// active jetpack with the charge for a thrust holds a weightless wearer when stabilized or on
/// its own step; the own step pays the thrust (`move_react`'s `check_on_move.Invoke(TRUE)`).
pub fn jetpack_holds(world: &mut World, mob: Entity, dir: Option<Dir>, continuous: bool) -> bool {
    let Some(jet) = world.get::<Jetpack>(mob).copied() else {
        return false;
    };
    let own_step = !continuous && dir.is_some();
    if !(jet.stabilize || own_step) || !check_charge(world, jet.control, jet.cost) {
        return false;
    }
    if own_step {
        subtract_charge(world, jet.control, jet.cost);
    }
    true
}

/// Whether a weightless `mob` can thrust at all (its HUD does not say it is stuck).
pub fn jetpack_ready(world: &World, mob: Entity) -> bool {
    world
        .get::<Jetpack>(mob)
        .is_some_and(|j| check_charge(world, j.control, j.cost))
}

/// `TRAIT_NOGRAV_ALWAYS_DRIFT`: an active jetpack without stabilizers.
pub fn always_drifts(world: &World, mob: Entity) -> bool {
    world.get::<Jetpack>(mob).is_some_and(|j| !j.stabilize)
}

/// `min(inertia_move_multiplier_passive, inertia_move_multiplier_active)`: a jetpack divides the
/// drift's loop delay by its `drift_force`.
pub fn inertia_multiplier(world: &World, e: Entity) -> f64 {
    world
        .get::<Jetpack>(e)
        .map_or(1.0, |j| 1.0 / f64::from(j.drift_force.max(1.0)))
}

/// `megaphone/handle_speech`: every line of a wearer with an active megaphone module pays
/// `use_energy_cost`.
pub fn megaphone_spoke(world: &mut World, speaker: Entity) {
    let Some(m) = world.get::<ModMegaphone>(speaker).copied() else {
        return;
    };
    let cost = def_of(world, m.module).map_or(0.0, |d| d.use_energy_cost);
    drain_power(world, m.control, cost);
}

/// The data HUDs a worn active visor gives `mob`: (security, medical).
pub fn visor_huds(world: &World, mob: Entity) -> (bool, bool) {
    world
        .get::<VisorHud>(mob)
        .map_or((false, false), |v| (v.security, v.medical))
}

/// `get_current_overlay_state`.
fn overlay_state(def: &ModuleDef, state: &ModModule, now: f64) -> Option<String> {
    let pick = if !def.overlay_use.is_empty() && now < state.cooldown_until {
        &def.overlay_use
    } else if !def.overlay_active.is_empty() && state.active {
        &def.overlay_active
    } else {
        &def.overlay_inactive
    };
    (!pick.is_empty()).then(|| pick.clone())
}

/// `on_install`'s overlay target: the part in the first required slot, else the unit.
fn overlay_target(world: &World, suit: &ModSuit, def: &ModuleDef, control: Entity) -> Entity {
    let Some(&first) = def.required_slots.first() else {
        return control;
    };
    let theme = theme_of(world, suit);
    if theme.slot_flags & first != 0 {
        return control;
    }
    suit.parts
        .iter()
        .find(|p| crate::inventory::slot_bit(p.slot) & first != 0)
        .map_or(control, |p| p.item)
}

/// The worn overlays of every module (`generate_worn_overlay`) onto their items' [`ModOverlays`];
/// true when one changed (the wearer is drawn again).
// Cost: per suit per tick, a pass over its modules (a handful) and an icon lookup each.
pub fn refresh_overlays(world: &mut World, control: Entity) -> bool {
    let Some(suit) = suit(world, control) else {
        return false;
    };
    let theme = theme_of(world, &suit);
    let sealed = suit.part_slots(&theme, true);
    let now = now(world);
    let mut wanted: Vec<(Entity, Vec<IconStateId>)> = suit
        .parts
        .iter()
        .map(|p| p.item)
        .chain([control])
        .map(|e| (e, Vec::new()))
        .collect();
    for &module in &suit.modules {
        let (Some(def), Some(state)) = (
            def_of(world, module),
            world.get::<ModModule>(module).cloned(),
        ) else {
            continue;
        };
        if def.mask_worn_overlay || !has_required_parts(&def, sealed) {
            continue;
        }
        let Some(icon_state) = overlay_state(&def, &state, now) else {
            continue;
        };
        let dmi = if def.overlay_icon.is_empty() {
            MODULES_WORN_DMI
        } else {
            def.overlay_icon.as_str()
        };
        let Some(id) = world
            .get_resource::<ss13_mapload::fidelity::IconIndex>()
            .and_then(|i| i.get(dmi, &icon_state))
        else {
            continue;
        };
        let target = overlay_target(world, &suit, &def, control);
        if let Some((_, list)) = wanted.iter_mut().find(|(e, _)| *e == target) {
            list.push(id);
        }
    }
    let mut changed = false;
    for (e, list) in wanted {
        let now_has = world.get::<ModOverlays>(e).map(|o| o.0.clone());
        match (now_has, list.is_empty()) {
            (None, true) => {}
            (Some(old), _) if old == list => {}
            (Some(_), true) => {
                world.entity_mut(e).remove::<ModOverlays>();
                changed = true;
            }
            (_, false) => {
                world.entity_mut(e).insert(ModOverlays(list));
                changed = true;
            }
        }
    }
    changed
}

/// tg `on_process(seconds_per_tick)`: with its parts sealed, an active module pays its active
/// cost (or turns off without it), an idle one its idle cost.
pub fn on_process(world: &mut World, control: Entity, module: Entity, dt: f64) {
    let (Some(def), Some(state)) = (
        def_of(world, module),
        world.get::<ModModule>(module).cloned(),
    ) else {
        return;
    };
    if !state.part_activated {
        return;
    }
    if state.active {
        if !drain_power(world, control, active_power(world, module) * dt) {
            deactivate(world, control, module, true);
            return;
        }
        match state.kind {
            // `sphere_transform/on_active_process`: no gravity, no ball; `on_statchange`: an
            // unconscious wearer neither.
            Kind::Sphere => {
                if let Some(wearer) = suit(world, control).and_then(|s| s.wearer)
                    && (!super::supply::has_gravity(world, wearer)
                        || world.get::<Stat>(wearer) != Some(&Stat::Conscious))
                {
                    deactivate(world, control, module, true);
                }
            }
            Kind::Kinesis => super::science::kinesis_process(world, control, module, dt),
            _ => {}
        }
    } else {
        drain_power(world, control, def.idle_power_cost * dt);
    }
}

/// `active_power_cost` (the flashlight's follows its range).
pub fn active_power(world: &World, module: Entity) -> f64 {
    let Some(state) = world.get::<ModModule>(module) else {
        return 0.0;
    };
    if state.kind == Kind::Flashlight {
        return FLASHLIGHT_POWER_PER_RANGE * f64::from(state.light_range);
    }
    def_of(world, module).map_or(0.0, |d| d.active_power_cost)
}

/// `COMSIG_MOD_UPDATE_SPEED`: what the module adds to the suit's slowdown.
pub fn slowdown(world: &World, module: Entity) -> f32 {
    match world.get::<ModModule>(module) {
        Some(m) if m.active && m.kind == Kind::Magboot => {
            def_of(world, module).map_or(0.0, |d| d.slowdown_active)
        }
        Some(m) if m.kind == Kind::AshAccretion => super::supply::ash_slowdown(world, module),
        // `/datum/movespeed_modifier/sphere` on the wearer.
        Some(m) if m.active && m.kind == Kind::Sphere => super::supply::SPHERE_SPEED,
        _ => 0.0,
    }
}

// ---------------------------------------------------------------------------------------------
// Configuration

/// `get_configuration`.
pub fn configuration(world: &World, module: Entity) -> Vec<ModuleConfig> {
    let Some(state) = world.get::<ModModule>(module) else {
        return Vec::new();
    };
    match state.kind {
        Kind::Flashlight => {
            let (min, max) = def_of(world, module)
                .and_then(|d| d.light)
                .map_or((2.0, 5.0), |l| (l.3, l.4));
            vec![ModuleConfig {
                key: "light_range".into(),
                display_name: "Дальность света".into(),
                kind: "number".into(),
                value: format!("{}", state.light_range),
                values: vec![format!("{min}"), format!("{max}")],
            }]
        }
        Kind::HealthAnalyzer => vec![ModuleConfig {
            key: "mode".into(),
            display_name: "Режим сканирования".into(),
            kind: "list".into(),
            value: SCAN_MODES[state.mode.min(SCAN_MODES.len() - 1)].into(),
            values: SCAN_MODES.iter().map(|m| (*m).to_owned()).collect(),
        }],
        Kind::DisposalConnector => super::supply::disposal_configuration(world, module),
        Kind::Jetpack { .. } => vec![ModuleConfig {
            key: "stabilizers".into(),
            display_name: "Стабилизаторы".into(),
            kind: "bool".into(),
            value: if state.stabilize { "1" } else { "0" }.into(),
            values: Vec::new(),
        }],
        _ => Vec::new(),
    }
}

/// `configure_edit(key, value)`, the value checked against the module's limits.
pub fn configure_edit(world: &mut World, control: Entity, module: Entity, key: &str, value: &str) {
    let Some(state) = world.get::<ModModule>(module).cloned() else {
        return;
    };
    match (state.kind, key) {
        (Kind::Flashlight, "light_range") => {
            let Ok(range) = value.trim().parse::<f32>() else {
                return;
            };
            let (min, max) = def_of(world, module)
                .and_then(|d| d.light)
                .map_or((2.0, 5.0), |l| (l.3, l.4));
            if !range.is_finite() {
                return;
            }
            if let Some(mut m) = world.get_mut::<ModModule>(module) {
                m.light_range = range.clamp(min, max);
            }
            if let Some((target, _)) = state.lit {
                light_off(world, module);
                light_on(world, module, target);
            }
        }
        (Kind::HealthAnalyzer, "mode") => {
            if let Some(i) = SCAN_MODES.iter().position(|m| *m == value)
                && let Some(mut m) = world.get_mut::<ModModule>(module)
            {
                m.mode = i;
            }
        }
        (Kind::DisposalConnector, "disposal_tag") => {
            super::supply::disposal_configure(world, module, value);
        }
        // `configure_jetpack(text2num(value))`: an active jetpack's wearer follows at once.
        (Kind::Jetpack { .. }, "stabilizers") => {
            let stabilize = matches!(value.trim(), "1" | "true" | "TRUE");
            if let Some(mut m) = world.get_mut::<ModModule>(module) {
                m.stabilize = stabilize;
            }
            if let Some(wearer) = suit(world, control).and_then(|s| s.wearer)
                && let Some(mut jet) = world.get_mut::<Jetpack>(wearer)
                && jet.module == module
            {
                jet.stabilize = stabilize;
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------------------------
// The flashlight

/// `#rrggbb` to RGB (white when malformed).
fn parse_color(text: &str) -> [u8; 3] {
    let hex = text.trim_start_matches('#');
    let byte = |i: usize| {
        hex.get(i..i + 2)
            .and_then(|h| u8::from_str_radix(h, 16).ok())
    };
    match (byte(0), byte(2), byte(4)) {
        (Some(r), Some(g), Some(b)) => [r, g, b],
        _ => [255, 255, 255],
    }
}

fn light_on(world: &mut World, module: Entity, wearer: Entity) {
    let (Some(state), Some(def)) = (
        world.get::<ModModule>(module).cloned(),
        def_of(world, module),
    ) else {
        return;
    };
    let Some((_, power, color, ..)) = def.light else {
        return;
    };
    let light = LightSource {
        range: state.light_range,
        power,
        color: parse_color(&color),
        height: 1.0,
        angle: 360,
        dir: Dir::South,
        offset: [0, 0],
    };
    // Another light on the wearer stays (one `Optics` light per entity).
    match world.get_mut::<Optics>(wearer) {
        Some(mut o) if o.light.is_none() => o.light = Some(light),
        Some(_) => return,
        None => {
            world.entity_mut(wearer).insert(Optics {
                light: Some(light),
                ..default()
            });
        }
    }
    if let Some(mut m) = world.get_mut::<ModModule>(module) {
        m.lit = Some((wearer, light));
    }
}

fn light_off(world: &mut World, module: Entity) {
    let Some((target, light)) = world
        .get_mut::<ModModule>(module)
        .and_then(|mut m| m.lit.take())
    else {
        return;
    };
    if world.get_entity(target).is_err() {
        return;
    }
    if let Some(mut o) = world.get_mut::<Optics>(target)
        && o.light == Some(light)
    {
        if !o.opaque && o.tile.is_none() {
            world.entity_mut(target).remove::<Optics>();
        } else {
            o.light = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(path: &str) -> ModuleDef {
        ModuleDef {
            path: path.into(),
            ..Default::default()
        }
    }

    #[test]
    fn kinds_follow_module_types() {
        assert_eq!(
            Kind::of(&def("/obj/item/mod/module/welding")),
            Kind::Welding
        );
        assert_eq!(
            Kind::of(&def("/obj/item/mod/module/magboot/advanced")),
            Kind::Magboot
        );
        assert_eq!(
            Kind::of(&def("/obj/item/mod/module/emp_shield/advanced")),
            Kind::EmpShield { advanced: true }
        );
        assert_eq!(
            Kind::of(&def("/obj/item/mod/module/jetpack/advanced")),
            Kind::Jetpack { advanced: true }
        );
        assert_eq!(
            Kind::of(&def("/obj/item/mod/module/visor/sechud")),
            Kind::Visor {
                security: true,
                medical: false
            }
        );
        assert_eq!(
            Kind::of(&def("/obj/item/mod/module/visor/thermal")),
            Kind::Other
        );
        assert_eq!(Kind::of(&def("/obj/item/mod/module/t_ray")), Kind::Other);
        assert_eq!(
            Kind::of(&def("/obj/item/mod/module/holster")),
            Kind::Holster
        );
        assert_eq!(
            Kind::of(&def("/obj/item/mod/module/noslip")),
            Kind::NoSlipWater
        );
        assert_eq!(
            Kind::of(&def("/obj/item/mod/module/clamp/loader")),
            Kind::Clamp { loader: true }
        );
        assert_eq!(Kind::of(&def("/obj/item/mod/module/magnet")), Kind::Magnet);
        assert_eq!(
            Kind::of(&def("/obj/item/mod/module/reagent_scanner/advanced")),
            Kind::ReagentScanner { advanced: true }
        );
        assert_eq!(
            Kind::of(&def(
                "/obj/item/mod/module/anomaly_locked/teleporter/prebuilt"
            )),
            Kind::Teleporter
        );
        assert_eq!(
            Kind::of(&def("/obj/item/mod/module/pathfinder")),
            Kind::Pathfinder
        );
        let mut storage = def("/obj/item/mod/module/storage/bluespace");
        storage.storage = Some(Default::default());
        assert_eq!(Kind::of(&storage), Kind::Storage);
    }

    /// The anti-slip module's trait stops water slips only.
    #[test]
    fn anti_slip_resists_water_only() {
        use crate::janitor::{TURF_WET_LUBE, TURF_WET_WATER};
        let mut world = World::new();
        let mob = world.spawn_empty().id();
        assert!(!resists_wet_slip(&world, mob, TURF_WET_WATER));
        world.entity_mut(mob).insert(NoSlipWater);
        assert!(resists_wet_slip(&world, mob, TURF_WET_WATER));
        assert!(!resists_wet_slip(&world, mob, TURF_WET_LUBE));
    }

    #[test]
    fn required_parts_need_one_slot_of_each_entry() {
        let mut d = def("/obj/item/mod/module/flashlight");
        // `ITEM_SLOT_HEAD|ITEM_SLOT_MASK`.
        d.required_slots = vec![(1 << 6) | (1 << 5)];
        assert!(has_required_parts(&d, 1 << 6));
        assert!(!has_required_parts(&d, 1 << 10));
        d.required_slots.push(1 << 2);
        assert!(!has_required_parts(&d, 1 << 6));
        assert!(has_required_parts(&d, (1 << 6) | (1 << 2)));
        assert!(has_required_parts(&def("/obj/item/mod/module/storage"), 0));
    }

    #[test]
    fn light_colors_parse() {
        assert_eq!(parse_color("#FF8000"), [255, 128, 0]);
        assert_eq!(parse_color("nonsense"), [255, 255, 255]);
    }
}
