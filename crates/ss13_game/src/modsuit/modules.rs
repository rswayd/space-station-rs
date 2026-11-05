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
//!
//! Every other module still installs, counts its complexity, toggles or selects, pays its power
//! and cooldown.
// shortcut: jetpacks, magboots' `TRAIT_NEGATES_GRAVITY`, t-ray, radiation protection, tethers,
// visors and the department modules toggle and pay power but have no effect yet (no zero-g
// drift, t-ray reveal, radiation, HUDs); no worn overlays, no light color picker, the light is
// omnidirectional (tg: directional), no chemical scan mode effect. Add them with their mechanics.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::modsuit::ModuleDef;
use ss13_mapload::{DmClothing, DmItem};
use ss13_shared::chat::Span;
use ss13_shared::protocol::InventorySlot;
use ss13_shared::{Dir, LightSource, Optics};

use super::server::{
    ModDb, ModSuit, ModuleSelected, SHIELD_PROTECTION, buzz, check_charge, name, refresh_buttons,
    set_emp_protection, subtract_charge, tell_mob, theme_of,
};
use super::*;
use crate::emp::{PROTECT_CONTENTS, PROTECT_SELF};
use crate::health::{Health, Stat};
use crate::storage::{Storage, Stored};

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

/// What a module does here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Storage,
    Welding,
    Flashlight,
    Magboot,
    HealthAnalyzer,
    EmpShield { advanced: bool },
    Other,
}

impl Kind {
    pub fn of(def: &ModuleDef) -> Self {
        let p = def.path.as_str();
        let under =
            |root: &str| is_under(p, &format!("{}/{root}", ss13_mapload::modsuit::MODULE_ROOT));
        if def.storage.is_some() {
            Self::Storage
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
}

/// On a wearer: `TRAIT_NO_SLIP_WATER` and `TRAIT_NO_SLIP_ICE` from active magboots.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct NoSlip;

/// Whether `mob` keeps its footing on a wet floor slip of this knockdown (`crate::janitor`:
/// water 6 s, ice 12 s; lube still gets it).
pub fn resists_slip(world: &World, mob: Entity, walking_safe: bool, knockdown: f32) -> bool {
    world.get::<NoSlip>(mob).is_some() && (walking_safe || knockdown >= 12.0)
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
    };
    world.entity_mut(module).insert(state);
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

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

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
        Kind::EmpShield { .. } => set_emp_protection(world, control, SHIELD_PROTECTION, true),
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
        Kind::EmpShield { .. } => set_emp_protection(world, control, SHIELD_PROTECTION, false),
        _ => {}
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
    if suit.wearer.is_none() && def.allow_flags & MODULE_ALLOW_UNWORN == 0 {
        tell_opt(world, who, Span::Warning, "не надет!");
        return;
    }
    let off = !suit.active || suit.activating;
    if (off && def.allow_flags & (MODULE_ALLOW_INACTIVE | MODULE_ALLOW_UNWORN) == 0)
        || def.module_type == MODULE_PASSIVE
    {
        tell_opt(world, who, Span::Warning, "не активен!");
        return;
    }
    let theme = theme_of(world, &suit);
    if !has_required_parts(&def, suit.part_slots(&theme, true))
        && def.allow_flags & MODULE_ALLOW_UNWORN == 0
    {
        tell_opt(world, who, Span::Warning, "нужные части не активны!");
        buzz(world, control);
        return;
    }
    if def.module_type == MODULE_USABLE {
        used(world, control, module, who);
    } else if world.get::<ModModule>(module).is_some_and(|m| m.active) {
        deactivate(world, control, module, true);
    } else {
        activate(world, control, module, who);
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
// The usable modules' own `on_use` effects (and their `drain_power`) are not ported.
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
    if let Some(mut m) = world.get_mut::<ModModule>(module) {
        m.cooldown_until = now + f64::from(def.cooldown_time) / 10.0;
    }
    true
}

/// tg `drain_power(amount)`: nothing is taken when there is not enough.
fn drain_power(world: &mut World, control: Entity, amount: f64) -> bool {
    if !check_charge(world, control, amount) {
        return false;
    }
    subtract_charge(world, control, amount);
    true
}

/// tg `on_select_use(target)` of a selected module without a device: the health analyzer's
/// scan.
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
    if !used(world, control, module, Some(wearer)) {
        return;
    }
    if kind(world, module) != Kind::HealthAnalyzer || world.get::<Health>(target).is_none() {
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
        _ => {}
    }
}

/// `on_deactivation`.
fn on_deactivation(world: &mut World, control: Entity, module: Entity) {
    let wearer = suit(world, control).and_then(|s| s.wearer);
    match kind(world, module) {
        Kind::Flashlight => light_off(world, module),
        Kind::Magboot => {
            if let Some(w) = wearer.filter(|&w| world.get_entity(w).is_ok()) {
                world.entity_mut(w).remove::<NoSlip>();
            }
        }
        _ => {}
    }
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
        _ => Vec::new(),
    }
}

/// `configure_edit(key, value)`, the value checked against the module's limits.
pub fn configure_edit(world: &mut World, _control: Entity, module: Entity, key: &str, value: &str) {
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
        assert_eq!(Kind::of(&def("/obj/item/mod/module/jetpack")), Kind::Other);
        let mut storage = def("/obj/item/mod/module/storage/bluespace");
        storage.storage = Some(Default::default());
        assert_eq!(Kind::of(&storage), Kind::Storage);
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
