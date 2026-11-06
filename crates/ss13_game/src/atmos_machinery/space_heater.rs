//! The space heater: tg `/obj/machinery/space_heater` (`code/game/machinery/spaceheater.dm`) and
//! its `SpaceHeater` window.
//!
//! - On, every SSair fire it compares its tile's air with the target temperature (±1 K): below
//!   it heats, above it cools (unless the mode is locked to the other one), moving
//!   `min(|dT| * heat capacity, 40 kJ, charge * efficiency / turfs)` joules into (or out of) its
//!   tile and every atmos-adjacent one, paid from its cell at `efficiency` joules of heat per
//!   joule of charge. Broken, or with a cell at 1 J or less, it switches off.
//! - An empty hand opens the window, a right click turns it on or off; a screwdriver opens the
//!   hatch, with it open a cell goes in, the window ejects it, sets the mode and the target
//!   (`30 °C ± 30`); a wrench (2 s) fastens or unfastens it. Mapped heaters start with a
//!   high-capacity cell.
// shortcut: the stock parts are the board's tier 1 ones (one micro laser, one capacitor), there
// is no crowbar deconstruction (machine frames and parts: docs/parity/engineering/machines.md),
// no emissive overlays, no climbing onto it, no EMP of the cell. The improvised chem heater
// (a beaker bath) is not this module's. Add each with its mechanic.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_chat::Span;
use ss13_mapload::{DmAtom, DmObj, IconIndex};
use ss13_shared::{Appearance, Overlay, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::space_heater::{SpaceHeaterData, SpaceHeaterUi};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use crate::construction::Job;
use crate::decor::DecorJob;
use crate::mecha::core::ItemCell;
use crate::obj_damage::is_under;
use crate::sound::{PlaySound, playsound};
use crate::storage::StoredIn;
use crate::tools::{Actor, ToolKind, tell, tool_of, use_tool};

pub const SPACE_HEATER: &str = "/obj/machinery/space_heater";
const IMPROVISED: &str = "/obj/machinery/space_heater/improvised_chem_heater";
/// The `cell` var.
const CELL: &str = "/obj/item/stock_parts/power_store/cell/high";
const SCREWDRIVER_SOUND: &str = "sound/items/tools/screwdriver.ogg";
const T0C: f64 = 273.15;
const T20C: f64 = 293.15;
const TCMB: f64 = 2.7;
/// `BASE_HEATING_ENERGY` times the micro laser tier.
const HEATING_ENERGY: f64 = 40_000.0;
/// `RefreshParts` with a tier 1 capacitor: `(1 + 1) * (20 MJ / STANDARD_CELL_CHARGE) * 0.5`.
const EFFICIENCY: f64 = 2000.0;
const TOLERANCE: f64 = 1.0;
/// `settable_temperature_median`, and `settable_temperature_range` with a tier 1 capacitor.
const MEDIAN: f64 = 30.0 + T0C;
const RANGE: f64 = 30.0;
/// `default_unfasten_wrench`: 2 s.
const WRENCH_DS: f32 = 20.0;

/// `set_mode`: what the heater may do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SetMode {
    #[default]
    Auto,
    Heat,
    Cool,
}

/// `mode`: what it does now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Standby,
    Heat,
    Cool,
}

/// `/obj/machinery/space_heater`: `on`, `mode`, `set_mode`, `target_temperature`, `panel_open`,
/// `cell`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct SpaceHeater {
    pub on: bool,
    pub mode: Mode,
    pub set_mode: SetMode,
    /// Kelvin.
    pub target_temperature: f64,
    pub panel_open: bool,
    /// `cell`; [`cell_of`] checks it is still inside.
    pub cell: Option<Entity>,
}

impl Default for SpaceHeater {
    fn default() -> Self {
        Self {
            on: false,
            mode: Mode::Standby,
            set_mode: SetMode::Auto,
            target_temperature: T20C,
            panel_open: false,
            cell: None,
        }
    }
}

pub fn plugin(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    app.add_click_handler(ClickPhase::EmptyHand, 1790, |w, c| {
        interact(w, c.client, c.mob, c.target, c.modifiers.right)
    });
    app.add_click_handler(ClickPhase::HeldOnTarget, 3985, |w, c| {
        c.held
            .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target))
    });
    app.register_ui::<SpaceHeaterWindow>()
        .add_observer(add_space_heater)
        .add_systems(ss13_world::MapInit, fill_mapped_heaters)
        .add_systems(
            Update,
            (
                heater_acts.in_set(UiSystems::Handle),
                process_space_heaters
                    .run_if(ss13_atmos::air_tick_due)
                    .in_set(ss13_atmos::AtmosSystems::Machinery),
                space_heater_looks.after(ss13_atmos::AtmosSystems::Machinery),
            ),
        );
}

fn add_space_heater(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    if let Ok(dm) = objs.get(add.entity)
        && is_under(&dm.path, SPACE_HEATER)
        && !is_under(&dm.path, IMPROVISED)
    {
        commands.entity(add.entity).insert(SpaceHeater::default());
    }
}

/// `Initialize`: `cell = new cell(src)`. Only mapped heaters: a built one has its cell taken out
/// (`on_construction`).
// shortcut: an admin-spawned heater starts without a cell; give it one when spawning runs
// `Initialize` for machines.
fn fill_mapped_heaters(world: &mut World) {
    let heaters: Vec<Entity> = world
        .query_filtered::<(Entity, &SpaceHeater), With<ss13_world::Mapped>>()
        .iter(world)
        .filter(|(_, h)| h.cell.is_none())
        .map(|(e, _)| e)
        .collect();
    for heater in heaters {
        if let Some(cell) = crate::construction::spawn_detached(world, CELL) {
            crate::storage::put_inside(world, cell, heater);
            if let Some(mut h) = world.get_mut::<SpaceHeater>(heater) {
                h.cell = Some(cell);
            }
        }
    }
}

/// The cell still inside.
pub fn cell_of(world: &World, e: Entity) -> Option<Entity> {
    let cell = world.get::<SpaceHeater>(e)?.cell?;
    world
        .get::<StoredIn>(cell)
        .is_some_and(|s| s.0 == e)
        .then_some(cell)
}

fn charge(world: &World, e: Entity) -> Option<f64> {
    cell_of(world, e)
        .and_then(|c| world.get::<ItemCell>(c))
        .map(|c| c.0.charge)
}

/// The settable range, kelvin.
pub fn temperature_bounds() -> (f64, f64) {
    ((MEDIAN - RANGE).max(TCMB), MEDIAN + RANGE)
}

/// `process_atmos`'s mode choice.
pub fn choose_mode(set_mode: SetMode, air: f64, target: f64) -> Mode {
    if set_mode != SetMode::Cool && air < target - TOLERANCE {
        Mode::Heat
    } else if set_mode != SetMode::Heat && air > target + TOLERANCE {
        Mode::Cool
    } else {
        Mode::Standby
    }
}

/// `process_atmos`: joules moved into each turf (negative: out of it).
pub fn energy_per_turf(
    mode: Mode,
    air: f64,
    target: f64,
    heat_capacity: f64,
    charge: f64,
    turfs: usize,
) -> f64 {
    let required = ((air - target).abs() * heat_capacity)
        .min(HEATING_ENERGY)
        .min(charge * EFFICIENCY / turfs.max(1) as f64);
    match mode {
        _ if required < 1.0 => 0.0,
        Mode::Standby => 0.0,
        Mode::Heat => required,
        Mode::Cool => -required,
    }
}

fn set_mode_now(world: &mut World, e: Entity, mode: Mode) {
    if let Some(mut h) = world.get_mut::<SpaceHeater>(e)
        && h.mode != mode
    {
        h.mode = mode;
    }
}

/// `process_atmos` of every heater that is on.
pub fn process_space_heaters(world: &mut World) {
    let on: Vec<(Entity, SetMode, f64)> = world
        .query::<(Entity, &SpaceHeater)>()
        .iter(world)
        .filter(|(_, h)| h.on)
        .map(|(e, h)| (e, h.set_mode, h.target_temperature))
        .collect();
    for (e, set_mode, target) in on {
        let broken = world
            .get::<crate::obj_damage::Integrity>(e)
            .is_some_and(|i| i.broken);
        let charge = charge(world, e).unwrap_or(0.0);
        if broken || charge <= 1.0 {
            if let Some(mut h) = world.get_mut::<SpaceHeater>(e) {
                h.on = false;
            }
            continue;
        }
        // Inside something (`!istype(loc, /turf)`): standby.
        let Some(&pos) = world.get::<TilePos>(e) else {
            set_mode_now(world, e, Mode::Standby);
            continue;
        };
        let atmos = world.resource::<ss13_atmos::Atmos>();
        let Some((air, heat_capacity)) = atmos.air(pos).map(|a| (a.temperature, a.heat_capacity()))
        else {
            set_mode_now(world, e, Mode::Standby);
            continue;
        };
        let mut turfs = atmos.adjacent_turfs(pos, false);
        turfs.push(pos);
        let mode = choose_mode(set_mode, air, target);
        set_mode_now(world, e, mode);
        let delta = energy_per_turf(mode, air, target, heat_capacity, charge, turfs.len());
        if delta == 0.0 {
            continue;
        }
        let mut atmos = world.resource_mut::<ss13_atmos::Atmos>();
        for &t in &turfs {
            atmos.modify_air(t, |a| {
                let capacity = a.heat_capacity();
                a.temperature += delta / capacity;
            });
        }
        let used = delta.abs() * turfs.len() as f64 / EFFICIENCY;
        if let Some(mut cell) = cell_of(world, e).and_then(|c| world.get_mut::<ItemCell>(c)) {
            cell.0.take(used, true);
        }
    }
}

/// `update_icon_state` and `update_overlays`.
pub fn space_heater_looks(
    icons: Option<Res<IconIndex>>,
    mut q: Query<(&SpaceHeater, &DmAtom, &mut Appearance), Changed<SpaceHeater>>,
) {
    let Some(icons) = icons else {
        return;
    };
    for (h, atom, mut appearance) in &mut q {
        let state = match (h.on, h.mode) {
            (false, _) => "sheater-off",
            (true, Mode::Standby) => "sheater-standby",
            (true, Mode::Heat) => "sheater-heat",
            (true, Mode::Cool) => "sheater-cool",
        };
        let mut new = appearance.clone();
        if let Some(id) = icons.get(&atom.icon, state) {
            new.icon_state = id;
        }
        new.overlays.clear();
        if h.panel_open
            && let Some(id) = icons.get(&atom.icon, "sheater-open")
        {
            new.overlays.push(Overlay::new(id));
        }
        if *appearance != new {
            *appearance = new;
        }
    }
}

/// `toggle_power(user)`: it switches even without a cell (the next fire switches it back off).
fn toggle_power(world: &mut World, client: Entity, e: Entity) {
    let charge = charge(world, e);
    let broken = world
        .get::<crate::obj_damage::Integrity>(e)
        .is_some_and(|i| i.broken);
    let Some(mut h) = world.get_mut::<SpaceHeater>(e) else {
        return;
    };
    h.on = !h.on;
    h.mode = Mode::Standby;
    let on = h.on;
    let text = match charge {
        None => "нет элемента питания!",
        Some(c) if c <= 0.0 => "нет заряда!",
        _ if broken => "не работает!",
        _ if on => "включено",
        _ => "выключено",
    };
    tell(world, client, Span::Notice, text);
}

fn title(world: &World, e: Entity) -> String {
    world
        .get::<ss13_mapload::DmName>(e)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default()
}

/// An empty hand: a click opens the window, a right click (`attack_hand_secondary`) toggles it.
pub fn interact(
    world: &mut World,
    client: Entity,
    mob: Entity,
    target: Entity,
    right: bool,
) -> bool {
    if world.get::<SpaceHeater>(target).is_none() {
        return false;
    }
    if right {
        toggle_power(world, client, target);
    } else {
        let title = title(world, target);
        world.write_message(OpenUi::new::<SpaceHeaterWindow>(client, mob, target, title));
    }
    true
}

/// `item_interaction` (a cell), `screwdriver_act` and `wrench_act`. True when the click was
/// theirs.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(h) = world.get::<SpaceHeater>(target) else {
        return false;
    };
    let open = h.panel_open;
    if world.get::<ItemCell>(held).is_some() {
        if !open {
            tell(
                world,
                client,
                Span::Warning,
                "Чтобы вставить элемент питания, откройте люк!",
            );
        } else if cell_of(world, target).is_some() {
            tell(
                world,
                client,
                Span::Warning,
                "Внутри уже есть элемент питания!",
            );
        } else {
            crate::storage::put_inside(world, held, target);
            if let Some(mut h) = world.get_mut::<SpaceHeater>(target) {
                h.cell = Some(held);
            }
            tell(
                world,
                client,
                Span::Notice,
                "Вы вставляете элемент питания.",
            );
        }
        return true;
    }
    // `base_item_interaction`: tool acts only out of combat mode.
    let combat = world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0);
    if combat {
        return false;
    }
    match tool_of(world, held).map(|(t, _)| t) {
        // `default_deconstruction_screwdriver`: instant, with the tool's sound.
        Some(ToolKind::Screwdriver) => {
            if let Some(mut h) = world.get_mut::<SpaceHeater>(target) {
                h.panel_open = !open;
            }
            if let Some(&pos) = world.get::<TilePos>(target) {
                playsound(world, PlaySound::new(pos, SCREWDRIVER_SOUND, 50.0, true));
            }
            let text = if open {
                "Вы закрываете люк обогревателя."
            } else {
                "Вы открываете люк обогревателя."
            };
            tell(world, client, Span::Notice, text);
            true
        }
        Some(ToolKind::Wrench) => {
            let actor = Actor {
                client,
                mob,
                item: Some(held),
            };
            use_tool(
                world,
                actor,
                target,
                WRENCH_DS,
                0,
                Job::Decor(DecorJob::Unfasten),
            );
            true
        }
        _ => false,
    }
}

pub struct SpaceHeaterWindow;

impl UiInterface for SpaceHeaterWindow {
    const NAME: &'static str = SpaceHeaterUi::NAME;
    type Data = SpaceHeaterData;
    type StaticData = ();
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpaceHeaterAct {
    Power,
    Mode(SetMode),
    /// °C.
    Target(f64),
    Eject,
}

fn mode_name(m: SetMode) -> &'static str {
    match m {
        SetMode::Auto => "auto",
        SetMode::Heat => "heat",
        SetMode::Cool => "cool",
    }
}

impl ServerUi for SpaceHeaterWindow {
    type Source = (Entity, &'static SpaceHeater, Option<&'static TilePos>);
    type UserParam = (
        Query<'static, 'static, (&'static ItemCell, &'static StoredIn)>,
        Option<Res<'static, ss13_atmos::Atmos>>,
    );
    type Act = SpaceHeaterAct;

    fn static_data(_: (Entity, &SpaceHeater, Option<&TilePos>)) {}

    fn data(
        (e, h, pos): (Entity, &SpaceHeater, Option<&TilePos>),
        _: Entity,
        (cells, atmos): &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> SpaceHeaterData {
        let cell = h
            .cell
            .and_then(|c| cells.get(c).ok())
            .filter(|(_, inside)| inside.0 == e)
            .map(|(c, _)| c.0.percent());
        let (min, max) = temperature_bounds();
        let current = atmos
            .as_ref()
            .zip(pos)
            .and_then(|(a, &p)| a.temperature(p))
            .map(|t| (t - T0C).round() as f32);
        SpaceHeaterData {
            open: h.panel_open,
            on: h.on,
            mode: mode_name(h.set_mode).to_owned(),
            has_powercell: cell.is_some(),
            power_level: cell.map(|p| p.round() as f32),
            target_temp: (h.target_temperature - T0C).round() as f32,
            min_temp: (min - T0C) as f32,
            max_temp: (max - T0C) as f32,
            current_temp: current,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<SpaceHeaterAct, UiReject> {
        match action {
            "power" => Ok(SpaceHeaterAct::Power),
            "eject" => Ok(SpaceHeaterAct::Eject),
            "target" => Ok(SpaceHeaterAct::Target(params.number("target")?)),
            // tg stores any string and treats an unknown one as auto; only the three the
            // window sends are taken.
            "mode" => match params.text("mode")?.as_str() {
                "auto" => Ok(SpaceHeaterAct::Mode(SetMode::Auto)),
                "heat" => Ok(SpaceHeaterAct::Mode(SetMode::Heat)),
                "cool" => Ok(SpaceHeaterAct::Mode(SetMode::Cool)),
                _ => Err(UiReject::BadParams("mode: auto, heat or cool".into())),
            },
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// tg `ui_act`, every check at the moment of effect.
fn heater_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<SpaceHeaterWindow>>>) {
    let pending: Vec<(Entity, Entity, Entity, SpaceHeaterAct)> = acts
        .read(world.resource::<Messages<UiActed<SpaceHeaterWindow>>>())
        .map(|a| (a.client, a.actor, a.source, a.act))
        .collect();
    for (client, actor, source, act) in pending {
        let Some(open) = world.get::<SpaceHeater>(source).map(|h| h.panel_open) else {
            continue;
        };
        match act {
            SpaceHeaterAct::Power => toggle_power(world, client, source),
            SpaceHeaterAct::Mode(mode) => {
                if let Some(mut h) = world.get_mut::<SpaceHeater>(source) {
                    h.set_mode = mode;
                }
            }
            SpaceHeaterAct::Target(celsius) => {
                if !open {
                    continue;
                }
                let (min, max) = temperature_bounds();
                if let Some(mut h) = world.get_mut::<SpaceHeater>(source) {
                    h.target_temperature = (celsius + T0C).round().clamp(min, max);
                }
            }
            SpaceHeaterAct::Eject => {
                if open && let Some(cell) = cell_of(world, source) {
                    if let Some(mut h) = world.get_mut::<SpaceHeater>(source) {
                        h.cell = None;
                    }
                    crate::cooking::give_to(world, actor, source, cell);
                }
            }
        }
    }
}

/// `examine`.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(Span, String)>) {
    let Some(h) = world.get::<SpaceHeater>(target) else {
        return;
    };
    let on = if h.on {
        "включён"
    } else {
        "выключен"
    };
    let hatch = if h.panel_open {
        "открыт"
    } else {
        "закрыт"
    };
    lines.push((Span::Plain, format!("Обогреватель {on}, люк {hatch}.")));
    let cell = cell_of(world, target)
        .and_then(|c| world.get::<ItemCell>(c))
        .map(|c| c.0.percent());
    lines.push(match cell {
        Some(p) => (
            Span::Plain,
            format!("Индикатор заряда показывает {}%.", p.round()),
        ),
        None => (Span::Warning, "Элемент питания не установлен.".to_owned()),
    });
    // `heating_examine`.
    let (min, max) = temperature_bounds();
    lines.push((
        Span::Notice,
        format!(
            "Дисплей показывает: мощность нагрева <b>{} кВт при эффективности {}%</b>, целевая температура <b>{}°C</b> ({}°C – {}°C).",
            HEATING_ENERGY / 1000.0 * 2.0,
            // tg shows `efficiency / 20 * 100`, written before cells were rescaled to 10 kJ
            // (10000% now); the share of the tier 1 efficiency is what it meant.
            EFFICIENCY / 2000.0 * 100.0,
            (h.target_temperature - T0C).round(),
            (min - T0C).round(),
            (max - T0C).round(),
        ),
    ));
    let toggle = if h.on {
        "выключить"
    } else {
        "включить"
    };
    lines.push((Span::Notice, format!("<b>ПКМ</b>, чтобы {toggle}.")));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_follows_the_air_and_the_lock() {
        assert_eq!(choose_mode(SetMode::Auto, 280.0, T20C), Mode::Heat);
        assert_eq!(choose_mode(SetMode::Auto, 310.0, T20C), Mode::Cool);
        assert_eq!(choose_mode(SetMode::Auto, T20C + 0.5, T20C), Mode::Standby);
        assert_eq!(choose_mode(SetMode::Cool, 280.0, T20C), Mode::Standby);
        assert_eq!(choose_mode(SetMode::Heat, 310.0, T20C), Mode::Standby);
    }

    #[test]
    fn energy_is_capped_by_the_power_and_the_cell() {
        // 10 K short of a 2000 J/K room: 20 kJ, under the 40 kJ cap.
        assert_eq!(
            energy_per_turf(Mode::Heat, 283.15, T20C, 2000.0, 1e5, 5),
            20_000.0
        );
        // A huge gap: the 40 kJ cap, negative when cooling.
        assert_eq!(
            energy_per_turf(Mode::Cool, 400.0, T20C, 2000.0, 1e5, 5),
            -40_000.0
        );
        // 10 J of charge over 5 turfs: 4 kJ each.
        assert_eq!(
            energy_per_turf(Mode::Heat, 200.0, T20C, 2000.0, 10.0, 5),
            4000.0
        );
        assert_eq!(
            energy_per_turf(Mode::Standby, 200.0, T20C, 2000.0, 1e5, 5),
            0.0
        );
    }

    #[test]
    fn the_settable_range_is_zero_to_sixty_celsius() {
        let (min, max) = temperature_bounds();
        assert!((min - T0C).abs() < 1e-9 && (max - T0C - 60.0).abs() < 1e-9);
    }

    #[test]
    fn only_the_window_modes_are_taken() {
        let heat = [("mode".to_owned(), "heat".to_owned())];
        let params = UiParams::new(&heat);
        assert_eq!(
            SpaceHeaterWindow::parse_act("mode", &params),
            Ok(SpaceHeaterAct::Mode(SetMode::Heat))
        );
        let melt = [("mode".to_owned(), "melt".to_owned())];
        let bad = UiParams::new(&melt);
        assert!(SpaceHeaterWindow::parse_act("mode", &bad).is_err());
        assert!(SpaceHeaterWindow::parse_act("spawn", &bad).is_err());
    }
}
