//! The distillation apparatus (server): tg `/obj/structure/chem_separator`
//! (`chem_separator.dm`) and its `ChemSeparator` window.
//!
//! - A 100 u flask (injectable, transparent). An open container clicked on it becomes the
//!   distillation beaker (the old one to the hands) and opens the window; right-clicked, the
//!   burner's fuel container. An empty hand takes the beaker (right: the fuel) out once the
//!   window is open. A condenser installs it; anything hot lights the burner; Alt-click puts
//!   it out; a crowbar takes it apart (wood, thermometer, burner, the condenser, the
//!   containers).
//! - The burner lights only with fuel that burns (plasma 1, oil 0.9, welding fuel and oxygen
//!   0.8, ethanol 0.7, Monkey Energy 0.6, water -0.7, anything else -1: summed, at most 1), a
//!   full flask and a beaker. Every 2 s it burns `0.01 + 0.04 × knob / 10` moles of the tile's
//!   oxygen (not with oxygen as fuel) and `0.01 + 0.19 × knob / 10` units of fuel, and heats
//!   the flask by `(1000 - T) × 2 s × 200 × (0.05 + 0.45 × knob / 10) × fuel` joules; out of
//!   either it goes out. Unlit, the flask and the beaker cool towards 300 K at 0.2 (plus the
//!   fuel's coefficient with the condenser on, burning 0.15 u of fuel).
//! - Every reagent whose boiling point (`400 + 500 × (mass - 10) / 790` K) the flask is over
//!   moves `2 + (T - bp) / 200 × 10` units into the beaker (or is lost without one).
// shortcut: no light, overlays or boiling sound; no `fire_act`/`extinguish` of the burner.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmItem, DmObj, DmReagents};
use ss13_reagents::{
    CHEMICAL_MAXIMUM_TEMPERATURE, Chemistry, DEFAULT_REAGENT_TEMPERATURE, Holder, Owner,
    SPECIFIC_HEAT_DEFAULT, flags,
};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::server::{OpenUi, ServerUi, UiActed, UiParams, UiReject};

use super::*;
use crate::chem::{Chem, Reagents};
use crate::obj_damage::is_under;
use crate::power::MachineTicks;
use crate::tools::{ToolKind, tool_of};

pub const SEPARATOR: &str = "/obj/structure/chem_separator";
pub const CONDENSER: &str = "/obj/item/assembly/igniter/condenser";
pub const WOOD: &str = "/obj/item/stack/sheet/mineral/wood";
pub const THERMOMETER: &str = "/obj/item/thermometer";
pub const BURNER: &str = "/obj/item/burner";
/// `MAX_BURNER_KNOB_SETTINGS`.
pub const MAX_KNOB: u32 = 10;
const FLASK_VOLUME: f32 = 100.0;
/// `process(seconds_per_tick)` of `SSobj`.
const TICK_SECS: f32 = 2.0;
const OXYGEN: &str = "/datum/reagent/oxygen";
/// `reagent_coefficients`, in `istype` order.
const FUELS: [(&str, f32); 7] = [
    ("/datum/reagent/toxin/plasma", 1.0),
    ("/datum/reagent/fuel/oil", 0.9),
    ("/datum/reagent/fuel", 0.8),
    (OXYGEN, 0.8),
    ("/datum/reagent/consumable/ethanol", 0.7),
    ("/datum/reagent/consumable/monkey_energy", 0.6),
    ("/datum/reagent/water", -0.7),
];

#[derive(Component, Clone, Debug, Default, PartialEq)]
#[require(Views<SeparatorData>)]
pub struct Separator {
    pub boiling: bool,
    pub distilled: Option<Entity>,
    pub fuel: Option<Entity>,
    pub burner_on: bool,
    pub condenser_installed: bool,
    pub condenser_on: bool,
    pub knob: u32,
}

pub(super) fn ensure(world: &mut World, e: Entity) -> bool {
    if world.get::<Separator>(e).is_some() {
        return true;
    }
    let Some(path) = world.get::<DmObj>(e).map(|o| o.path.clone()) else {
        return false;
    };
    if !is_under(&path, SEPARATOR) {
        return false;
    }
    world.entity_mut(e).insert((
        Separator {
            knob: 1,
            ..default()
        },
        Reagents(Holder::new(
            FLASK_VOLUME,
            flags::TRANSPARENT | flags::INJECTABLE,
            Owner {
                path,
                living: false,
            },
        )),
    ));
    true
}

fn distilled(world: &World, e: Entity) -> Option<Entity> {
    inside(world, e, world.get::<Separator>(e)?.distilled)
}

fn fuel(world: &World, e: Entity) -> Option<Entity> {
    inside(world, e, world.get::<Separator>(e)?.fuel)
}

fn holder(world: &World, e: Option<Entity>) -> Option<Holder> {
    world.get::<Reagents>(e?).map(|r| r.0.clone())
}

/// `get_boiling_point`.
pub fn boiling_point(mass: f32) -> f32 {
    400.0 + 500.0 * (mass - 10.0) / 790.0
}

/// `get_ignition_coefficient` of a fuel holder.
pub fn ignition_coefficient(chem: &Chemistry, fuel: Option<&Holder>) -> f32 {
    let Some(fuel) = fuel else {
        return 0.0;
    };
    let total: f32 = fuel
        .reagents
        .iter()
        .map(|r| {
            FUELS
                .iter()
                .find(|(path, _)| chem.is_type(r.id, path))
                .map_or(-1.0, |&(_, c)| c)
        })
        .sum();
    total.clamp(0.0, 1.0)
}

/// `toggle_burner(state)`.
fn toggle_burner(world: &mut World, e: Entity, on: bool) {
    if !on {
        if let Some(mut s) = world.get_mut::<Separator>(e) {
            s.burner_on = false;
        }
        return;
    }
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let fuel = holder(world, fuel(world, e));
    if ignition_coefficient(&chem, fuel.as_ref()) <= 0.0
        || world
            .get::<Reagents>(e)
            .is_none_or(|r| r.0.total_volume <= 0.0)
        || distilled(world, e).is_none()
    {
        return;
    }
    if let Some(mut s) = world.get_mut::<Separator>(e) {
        s.burner_on = true;
    }
}

fn open(world: &mut World, client: Entity, mob: Entity, e: Entity) {
    world.write_message(OpenUi::new::<ChemSeparatorUi>(
        client,
        mob,
        e,
        title(world, e),
    ));
}

fn window_open(world: &mut World, client: Entity, e: Entity) -> bool {
    let mut q = world.query::<&ss13_tgui::server::UiWindow>();
    q.iter(world).any(|w| {
        w.client == client
            && w.source == e
            && w.interface == <ChemSeparatorUi as ss13_tgui::UiInterface>::NAME
    })
}

/// `attack_hand` (the beaker out) and `attack_hand_secondary` (the fuel out): the window
/// first if it is not open.
pub(super) fn interact(
    world: &mut World,
    client: Entity,
    mob: Entity,
    e: Entity,
    right: bool,
) -> bool {
    if world.get::<Separator>(e).is_none() {
        return false;
    }
    let item = if right {
        fuel(world, e)
    } else {
        distilled(world, e)
    };
    let Some(item) = item else {
        return true;
    };
    if !window_open(world, client, e) {
        open(world, client, mob, e);
        return true;
    }
    hand_out(world, Some(mob), e, item);
    if right {
        toggle_burner(world, e, false);
        notice(world, client, "Вы вынимаете ёмкость с топливом горелки.");
    } else {
        notice(world, client, "Вы вынимаете приёмную колбу.");
    }
    true
}

pub(super) fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    e: Entity,
    right: bool,
) -> bool {
    if world.get::<Separator>(e).is_none() {
        return false;
    }
    if world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0)
    {
        return false;
    }
    if tool_of(world, held).is_some_and(|(k, _)| k == ToolKind::Crowbar) {
        crate::obj_damage::deconstruct(world, e, true);
        return true;
    }
    let open_container = world.get::<DmReagents>(held).is_some()
        && world
            .get::<Reagents>(held)
            .is_some_and(|r| r.0.is_open_container());
    if open_container {
        let old = if right {
            fuel(world, e)
        } else {
            distilled(world, e)
        };
        if let Some(old) = old {
            hand_out(world, Some(mob), e, old);
        }
        take_in(world, mob, e, held);
        if let Some(mut s) = world.get_mut::<Separator>(e) {
            if right {
                s.fuel = Some(held);
            } else {
                s.distilled = Some(held);
            }
        }
        let text = if right {
            "ёмкость с топливом добавлена"
        } else {
            "приёмная колба добавлена"
        };
        crate::balloon::balloon_alert(world, mob, e, text);
        open(world, client, mob, e);
        return true;
    }
    if right {
        return false;
    }
    let path = world
        .get::<DmItem>(held)
        .map(|i| i.path.clone())
        .unwrap_or_default();
    if is_under(&path, CONDENSER) {
        if world
            .get::<Separator>(e)
            .is_some_and(|s| s.condenser_installed)
        {
            return false;
        }
        consume_held(world, mob, held);
        if let Some(mut s) = world.get_mut::<Separator>(e) {
            s.condenser_installed = true;
        }
        crate::balloon::balloon_alert(world, mob, e, "конденсатор установлен");
        return true;
    }
    if crate::fireplace::is_hot(world, held) {
        notice(world, client, "Вы поджигаете горелку.");
        toggle_burner(world, e, true);
        return true;
    }
    false
}

/// `click_alt`: the burner goes out.
pub(super) fn click_alt(world: &mut World, e: Entity) -> bool {
    let Some(on) = world.get::<Separator>(e).map(|s| s.burner_on) else {
        return false;
    };
    if on {
        toggle_burner(world, e, false);
    }
    true
}

/// `atom_deconstruct` (from `crate::obj_damage::deconstruct`): its parts and containers.
pub fn deconstruct(world: &mut World, e: Entity, obj: &DmObj, pos: TilePos) -> bool {
    if !is_under(&obj.path, SEPARATOR) {
        return false;
    }
    crate::construction::spawn_stack(world, WOOD, pos, 1);
    crate::construction::spawn_obj(world, THERMOMETER, pos);
    crate::construction::spawn_obj(world, BURNER, pos);
    if world
        .get::<Separator>(e)
        .is_some_and(|s| s.condenser_installed)
    {
        crate::construction::spawn_obj(world, CONDENSER, pos);
    }
    for item in [distilled(world, e), fuel(world, e)].into_iter().flatten() {
        crate::storage::take_out(world, item, pos);
    }
    true
}

/// `get_cool_coefficient`: 0.2, plus the fuel's with the condenser on (0.15 u of fuel).
fn cool_coefficient(world: &mut World, e: Entity, chem: &Chemistry) -> f32 {
    let mut coefficient = 0.2;
    let Some(s) = world.get::<Separator>(e).cloned() else {
        return coefficient;
    };
    let fuel_e = fuel(world, e);
    let fuel_coefficient = ignition_coefficient(chem, holder(world, fuel_e).as_ref());
    if s.condenser_installed
        && s.condenser_on
        && fuel_coefficient > 0.0
        && let Some(fuel_e) = fuel_e
        && let Some(mut f) = world.get_mut::<Reagents>(fuel_e)
    {
        if f.0.remove_all(0.15) > 0.0 {
            coefficient += fuel_coefficient;
        } else if let Some(mut s) = world.get_mut::<Separator>(e) {
            s.condenser_on = false;
        }
    }
    coefficient
}

/// `process(2 s)`.
// Cost: one pass over the apparatuses per 2 s; each works through its few reagents.
pub(super) fn process(world: &mut World, mut last: Local<Option<u64>>) {
    let ticks = world.resource::<MachineTicks>().0;
    let from = last.unwrap_or(ticks);
    *last = Some(ticks);
    if from == ticks {
        return;
    }
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let mut q = world.query_filtered::<Entity, With<Separator>>();
    let all: Vec<Entity> = q.iter(world).collect();
    for e in all {
        tick(world, e, &chem);
    }
}

fn tick(world: &mut World, e: Entity, chem: &Chemistry) {
    let Some(s) = world.get::<Separator>(e).cloned() else {
        return;
    };
    let Some(mut flask) = world.get::<Reagents>(e).map(|r| r.0.clone()) else {
        return;
    };
    let beaker = distilled(world, e);
    if flask.total_volume <= 0.0 {
        let beaker_empty = holder(world, beaker).is_none_or(|h| h.total_volume <= 0.0);
        if beaker_empty {
            if s.boiling || s.burner_on {
                if let Some(mut s) = world.get_mut::<Separator>(e) {
                    s.boiling = false;
                    s.burner_on = false;
                }
            }
            return;
        }
    }
    if s.burner_on {
        let fuel_e = fuel(world, e);
        let fuel_coefficient = ignition_coefficient(chem, holder(world, fuel_e).as_ref());
        let knob_ratio = s.knob as f32 / MAX_KNOB as f32;
        let mut burning = fuel_coefficient > 0.0 && fuel_e.is_some();
        if burning {
            let oxygen_fuel = chem
                .id(OXYGEN)
                .is_some_and(|id| holder(world, fuel_e).is_some_and(|h| h.has(id).is_some()));
            if !oxygen_fuel {
                burning = burn_oxygen(world, e, 0.01 + 0.04 * f64::from(knob_ratio));
            }
        }
        if burning
            && let Some(fuel_e) = fuel_e
            && let Some(mut f) = world.get_mut::<Reagents>(fuel_e)
        {
            burning = f.0.remove_all(0.01 + 0.19 * knob_ratio) > 0.0;
        }
        if burning {
            let energy = (1000.0 - flask.chem_temp)
                * TICK_SECS
                * SPECIFIC_HEAT_DEFAULT
                * (0.05 + 0.45 * knob_ratio)
                * fuel_coefficient;
            flask.adjust_thermal_energy(chem, energy, 2.7, CHEMICAL_MAXIMUM_TEMPERATURE);
            flask.handle_reactions(chem);
        } else {
            toggle_burner(world, e, false);
        }
    } else if flask.chem_temp > DEFAULT_REAGENT_TEMPERATURE {
        let energy = (DEFAULT_REAGENT_TEMPERATURE - flask.chem_temp)
            * TICK_SECS
            * SPECIFIC_HEAT_DEFAULT
            * cool_coefficient(world, e, chem);
        flask.adjust_thermal_energy(chem, energy, 2.7, CHEMICAL_MAXIMUM_TEMPERATURE);
        flask.handle_reactions(chem);
    }
    let mut target = holder(world, beaker);
    if let Some(t) = target.as_mut()
        && t.chem_temp > DEFAULT_REAGENT_TEMPERATURE
    {
        let energy = (DEFAULT_REAGENT_TEMPERATURE - t.chem_temp)
            * TICK_SECS
            * SPECIFIC_HEAT_DEFAULT
            * cool_coefficient(world, e, chem);
        t.adjust_thermal_energy(chem, energy, 2.7, CHEMICAL_MAXIMUM_TEMPERATURE);
        t.handle_reactions(chem);
    }
    let masses = masses(world);
    let mut boiling = false;
    let ids: Vec<_> = flask.reagents.iter().map(|r| r.id).collect();
    for id in ids {
        let bp = boiling_point(masses.get(usize::from(id)).copied().unwrap_or(10.0));
        if flask.chem_temp <= bp {
            continue;
        }
        let amount = 2.0 + (flask.chem_temp - bp) / 200.0 * 10.0;
        match target.as_mut() {
            Some(t) => {
                flask.trans_to(chem, t, amount, 1.0, Some(id), false, false);
            }
            None => {
                flask.remove_reagent(id, amount);
            }
        }
        boiling = true;
    }
    world.entity_mut(e).insert(Reagents(flask));
    if let (Some(b), Some(t)) = (beaker, target) {
        world.entity_mut(b).insert(Reagents(t));
    }
    if let Some(mut s) = world.get_mut::<Separator>(e)
        && s.boiling != boiling
    {
        s.boiling = boiling;
    }
}

/// `air.remove_specific(/datum/gas/oxygen, moles)` on its tile: false when there was none.
fn burn_oxygen(world: &mut World, e: Entity, moles: f64) -> bool {
    let Some(&pos) = world.get::<TilePos>(e) else {
        return false;
    };
    let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() else {
        return true;
    };
    atmos
        .modify_air(pos, |air| {
            let o2 = air.get(ss13_atmos::GasId::O2);
            if o2 <= 0.0 {
                return false;
            }
            air.set(ss13_atmos::GasId::O2, (o2 - moles).max(0.0));
            air.garbage_collect();
            true
        })
        .unwrap_or(false)
}

fn bar(chem: &Chemistry, h: &Holder) -> HolderBar {
    let color = ss13_reagents::mix_color(chem, &h.reagents).unwrap_or([0, 0, 0, 0]);
    HolderBar {
        total_volume: h.total_volume,
        maximum_volume: h.maximum_volume,
        temp: h.chem_temp,
        color: [color[0], color[1], color[2]],
    }
}

pub(super) fn view(world: &mut World, e: Entity, _user: Entity) -> Option<SeparatorData> {
    let s = world.get::<Separator>(e)?.clone();
    let chem = world.get_resource::<Chem>()?.0.clone();
    let flask = world.get::<Reagents>(e)?.0.clone();
    Some(SeparatorData {
        flask: bar(&chem, &flask),
        beaker: holder(world, distilled(world, e)).map(|h| bar(&chem, &h)),
        fuel: holder(world, fuel(world, e)).map(|h| bar(&chem, &h)),
        burner_on: s.burner_on,
        knob: s.knob,
        condenser_on: s.condenser_on,
    })
}

impl ServerUi for ChemSeparatorUi {
    type Source = (&'static Views<SeparatorData>, &'static Separator);
    type UserParam = ();
    type Act = SeparatorAct;

    fn static_data((_, s): (&Views<SeparatorData>, &Separator)) -> SeparatorStatic {
        SeparatorStatic {
            condenser_installed: s.condenser_installed,
            max_burner_knob_settings: MAX_KNOB,
        }
    }

    fn data(
        (views, _): (&Views<SeparatorData>, &Separator),
        user: Entity,
        _: &(),
    ) -> SeparatorData {
        views.of(user)
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<SeparatorAct, UiReject> {
        Ok(match action {
            "drain" => SeparatorAct::Drain,
            "filter" => SeparatorAct::Filter,
            "knob" => SeparatorAct::Knob(params.number("amount")?),
            "cool" => SeparatorAct::Cool,
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SeparatorAct {
    Drain,
    Filter,
    Knob(f64),
    Cool,
}

pub(super) fn acts(world: &mut World, mut cursor: Local<MessageCursor<UiActed<ChemSeparatorUi>>>) {
    let pending: Vec<(Entity, SeparatorAct)> = cursor
        .read(world.resource::<Messages<UiActed<ChemSeparatorUi>>>())
        .map(|a| (a.source, a.act.clone()))
        .collect();
    for (e, act) in pending {
        apply_act(world, e, act);
    }
}

/// tg `ui_act`, checked at the moment of effect.
pub fn apply_act(world: &mut World, e: Entity, act: SeparatorAct) {
    let Some(s) = world.get::<Separator>(e).cloned() else {
        return;
    };
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    match act {
        SeparatorAct::Drain | SeparatorAct::Filter => {
            let Some(beaker) = distilled(world, e) else {
                return;
            };
            let (from, to) = if act == SeparatorAct::Drain {
                (e, beaker)
            } else {
                (beaker, e)
            };
            let Some(mut a) = world.get::<Reagents>(from).map(|r| r.0.clone()) else {
                return;
            };
            let Some(mut b) = world.get::<Reagents>(to).map(|r| r.0.clone()) else {
                return;
            };
            let moved = a.trans_to(&chem, &mut b, FLASK_VOLUME, 1.0, None, false, false);
            world.entity_mut(from).insert(Reagents(a));
            world.entity_mut(to).insert(Reagents(b));
            if moved > 0.0 && act == SeparatorAct::Drain {
                toggle_burner(world, e, false);
            }
        }
        SeparatorAct::Knob(value) => {
            if value.is_finite() && value != 0.0 {
                let knob = value.clamp(1.0, f64::from(MAX_KNOB)) as u32;
                if let Some(mut s) = world.get_mut::<Separator>(e) {
                    s.knob = knob;
                }
            }
        }
        SeparatorAct::Cool => {
            if s.condenser_installed
                && let Some(mut s) = world.get_mut::<Separator>(e)
            {
                s.condenser_on = !s.condenser_on;
            }
        }
    }
}

/// `examine`.
pub fn examine_lines(world: &World, e: Entity, lines: &mut Vec<(Span, String)>) {
    let Some(s) = world.get::<Separator>(e) else {
        return;
    };
    let num = crate::chem::dm_num;
    if let Some(r) = world.get::<Reagents>(e).filter(|r| r.0.total_volume > 0.0) {
        lines.push((
            Span::Notice,
            format!(
                "Перегонная колба: {}/{} ед.",
                num(r.0.total_volume),
                num(r.0.maximum_volume)
            ),
        ));
    }
    match distilled(world, e).and_then(|b| world.get::<Reagents>(b)) {
        Some(r) => lines.push((
            Span::Notice,
            format!(
                "Приёмная колба: {}/{} ед. Вынуть — ЛКМ.",
                num(r.0.total_volume),
                num(r.0.maximum_volume)
            ),
        )),
        None => lines.push((
            Span::Warning,
            "Нет приёмной колбы, вставьте её ЛКМ.".to_owned(),
        )),
    }
    match fuel(world, e).and_then(|b| world.get::<Reagents>(b)) {
        Some(r) => lines.push((
            Span::Notice,
            format!(
                "Топливо горелки: {}/{} ед. Вынуть — ПКМ.",
                num(r.0.total_volume),
                num(r.0.maximum_volume)
            ),
        )),
        None => lines.push((
            Span::Warning,
            "Нет ёмкости с топливом для горелки, вставьте её ПКМ.".to_owned(),
        )),
    }
    if s.burner_on {
        lines.push((Span::Notice, "Погасить горелку — Alt+ЛКМ.".to_owned()));
    } else {
        lines.push((
            Span::Notice,
            "Горелку можно поджечь чем-нибудь горящим.".to_owned(),
        ));
    }
    if s.condenser_installed {
        lines.push((
            Span::Notice,
            "Встроенный конденсатор ускоряет охлаждение, но тратит топливо.".to_owned(),
        ));
    } else {
        lines.push((
            Span::Notice,
            "Можно установить конденсатор для быстрого охлаждения.".to_owned(),
        ));
    }
    lines.push((Span::Notice, "Аппарат можно разобрать ломом.".to_owned()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boiling_points_span_400_to_900_kelvin() {
        assert_eq!(boiling_point(10.0), 400.0);
        assert_eq!(boiling_point(800.0), 900.0);
        // 2 + (T - bp) / 200 * 10 units a tick: 12 at 200 K over.
        let bp = boiling_point(405.0);
        assert!((2.0 + (bp + 200.0 - bp) / 200.0 * 10.0 - 12.0).abs() < 1e-4);
    }
}
