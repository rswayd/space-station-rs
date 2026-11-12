//! The high-performance liquid chromatography machine (server): tg `/obj/machinery/chem_mass_spec`
//! (`chem_mass_spec.dm`) and its `MassSpec` window.
//!
//! - Two slots: an input beaker (left click with a container, Alt-click or the window to eject)
//!   and an output beaker (right click); a mapped machine starts with a large output beaker.
//! - The mass range starts at the input's lightest and heaviest reagent (an inverted reagent,
//!   purity under its `inverse_chem_val`, counts as its inverse), floored and ceiled to 50; the
//!   window's sliders narrow it. `activate` (both beakers in, work to do) starts a run of
//!   `Σ ((mass × volume + mass × inverse purity × 0.1) × 0.0035 + 10)` seconds, divided by every
//!   micro-laser's tier, over the reagents in range that are not at their purity; powered,
//!   anchored and shut, it advances 2 s a machine tick. At the end each such reagent moves to the
//!   output at its type's purity (`volume × (1 - impurity)` units), an inverted one becomes its
//!   inverse at its inverse purity.
//! - Tools and beakers are refused while it runs.
// shortcut: no absorbance graph (the window lists the reagents and their masses); Alt+right
// click does not eject the output beaker (the window does).

use std::collections::HashMap;

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmObj, DmPower, DmReagents};
use ss13_reagents::{Chemistry, Holder, ReagentId, round_to};
use ss13_shared::chat::Span;
use ss13_tgui::server::{OpenUi, ServerUi, UiActed, UiParams, UiReject};

use super::*;
use crate::chem::{Chem, Reagents};
use crate::obj_damage::{anchored, is_under};
use crate::power::{MACHINE_TICK_SECS, MachineTicks, Unpowered};
use crate::stock_parts::PartKind;
use crate::tools::{Actor, ToolKind, tool_of, use_tool};

pub const MASS_SPEC: &str = "/obj/machinery/chem_mass_spec";
pub const LARGE_BEAKER: &str = "/obj/item/reagent_containers/cup/beaker/large";
/// `default_unfasten_wrench`, deciseconds.
const WRENCH_DS: f32 = 20.0;

#[derive(Component, Clone, Debug, PartialEq)]
#[require(Views<MassSpecData>)]
pub struct MassSpec {
    pub processing: bool,
    /// Seconds.
    pub delay_time: f32,
    pub progress_time: f32,
    pub lower: f32,
    pub upper: f32,
    pub input: Option<Entity>,
    pub output: Option<Entity>,
    pub cms_coefficient: f32,
    /// `log`: by the input reagent's type.
    pub log: HashMap<ReagentId, String>,
}

pub(super) fn ensure(world: &mut World, e: Entity) -> bool {
    if world.get::<MassSpec>(e).is_some() {
        return true;
    }
    if !world
        .get::<DmObj>(e)
        .is_some_and(|o| is_under(&o.path, MASS_SPEC))
    {
        return false;
    }
    crate::stock_parts::ensure_parts(world, e);
    let mut output = None;
    if world.get::<ss13_world::Mapped>(e).is_some()
        && let Some(beaker) = crate::construction::spawn_detached(world, LARGE_BEAKER)
    {
        crate::storage::put_inside(world, beaker, e);
        output = Some(beaker);
    }
    world.entity_mut(e).insert(MassSpec {
        processing: false,
        delay_time: 0.0,
        progress_time: 0.0,
        lower: 0.0,
        upper: f32::INFINITY,
        input: None,
        output,
        cms_coefficient: 1.0,
        log: HashMap::new(),
    });
    refresh_parts(world, e);
    true
}

/// tg `RefreshParts`: 1 divided by every micro-laser's tier.
pub fn refresh_parts(world: &mut World, e: Entity) {
    if world.get::<MassSpec>(e).is_none() {
        return;
    }
    let r = crate::stock_parts::ratings(world, e);
    let c = r
        .tiers(PartKind::MicroLaser)
        .fold(1.0, |c, t| c / t.max(1) as f32);
    if let Some(mut m) = world.get_mut::<MassSpec>(e) {
        m.cms_coefficient = c;
    }
}

fn input(world: &World, e: Entity) -> Option<Entity> {
    inside(world, e, world.get::<MassSpec>(e)?.input)
}

fn output(world: &World, e: Entity) -> Option<Entity> {
    inside(world, e, world.get::<MassSpec>(e)?.output)
}

fn holder(world: &World, e: Option<Entity>) -> Option<Holder> {
    world.get::<Reagents>(e?).map(|r| r.0.clone())
}

/// `get_inverse_purity(purity)`.
pub fn inverse_purity(chem: &Chemistry, id: ReagentId, purity: f32) -> f32 {
    let def = chem.def(id);
    if def.inverse_chem.is_none() || def.inverse_chem_val == 0.0 {
        return 0.0;
    }
    (1.0 - def.inverse_chem_val + purity + 0.01).min(1.0)
}

/// The inverse a reagent counts as (`inverse_chem && inverse_chem_val > purity`).
fn inverted(chem: &Chemistry, id: ReagentId, purity: f32) -> Option<ReagentId> {
    let def = chem.def(id);
    if def.inverse_chem_val > purity {
        def.inverse_chem.as_deref().and_then(|p| chem.id(p))
    } else {
        None
    }
}

/// `calculate_mass(smallest)` of the input.
pub fn calculate_mass(chem: &Chemistry, masses: &[f32], h: Option<&Holder>, smallest: bool) -> f32 {
    let Some(h) = h else {
        return 0.0;
    };
    let mut result = 0.0_f32;
    for r in &h.reagents {
        let target = inverted(chem, r.id, r.purity).unwrap_or(r.id);
        let mass = masses.get(usize::from(target)).copied().unwrap_or(0.0);
        result = if result == 0.0 {
            mass
        } else if smallest {
            result.min(mass)
        } else {
            result.max(mass)
        };
    }
    if smallest {
        (result / 50.0).floor() * 50.0
    } else {
        (result / 50.0).ceil() * 50.0
    }
}

/// `estimate_time`: seconds of work on the input in range.
pub fn estimate_time(
    chem: &Chemistry,
    masses: &[f32],
    h: Option<&Holder>,
    lower: f32,
    upper: f32,
    coefficient: f32,
) -> f32 {
    let Some(h) = h else {
        return 0.0;
    };
    let mut delay = 0.0;
    for r in &h.reagents {
        let inverse = inverted(chem, r.id, r.purity);
        let target = inverse.unwrap_or(r.id);
        let mass = masses.get(usize::from(target)).copied().unwrap_or(0.0);
        if mass < lower || mass > upper {
            continue;
        }
        if inverse.is_none() && chem.def(r.id).purity - r.purity <= 0.0 {
            continue;
        }
        delay +=
            (mass * r.volume + mass * inverse_purity(chem, r.id, r.purity) * 0.1) * 0.0035 + 10.0;
    }
    delay * coefficient
}

/// Recomputes the range (`calculate_mass`) and the delay of the input.
fn reset_range(world: &mut World, e: Entity) {
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let masses = masses(world);
    let h = holder(world, input(world, e));
    let lower = calculate_mass(&chem, &masses, h.as_ref(), true);
    let upper = calculate_mass(&chem, &masses, h.as_ref(), false);
    if let Some(mut m) = world.get_mut::<MassSpec>(e) {
        m.lower = lower;
        m.upper = upper;
        m.delay_time = estimate_time(&chem, &masses, h.as_ref(), lower, upper, m.cms_coefficient);
    }
}

fn estimate(world: &mut World, e: Entity) {
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let masses = masses(world);
    let h = holder(world, input(world, e));
    if let Some(mut m) = world.get_mut::<MassSpec>(e) {
        m.delay_time = estimate_time(
            &chem,
            &masses,
            h.as_ref(),
            m.lower,
            m.upper,
            m.cms_coefficient,
        );
    }
}

/// `replace_beaker(user, is_input, new_beaker)`.
fn replace_beaker(world: &mut World, mob: Entity, e: Entity, is_input: bool, new: Option<Entity>) {
    let old = if is_input {
        input(world, e)
    } else {
        output(world, e)
    };
    if let Some(old) = old {
        hand_out(world, Some(mob), e, old);
    }
    if let Some(new) = new {
        take_in(world, mob, e, new);
    }
    if let Some(mut m) = world.get_mut::<MassSpec>(e) {
        if is_input {
            m.input = new;
        } else {
            m.output = new;
            if new.is_some() {
                m.log.clear();
            }
        }
    }
    if is_input && new.is_some() {
        reset_range(world, e);
    }
}

fn container(world: &World, item: Entity) -> bool {
    world.get::<DmReagents>(item).is_some() && world.get::<Reagents>(item).is_some()
}

fn open(world: &mut World, client: Entity, mob: Entity, e: Entity) {
    world.write_message(OpenUi::new::<MassSpecUi>(client, mob, e, title(world, e)));
}

pub(super) fn interact(
    world: &mut World,
    client: Entity,
    mob: Entity,
    e: Entity,
    right: bool,
) -> bool {
    if world.get::<MassSpec>(e).is_none() {
        return false;
    }
    if !right && anchored(world, e) {
        open(world, client, mob, e);
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
    let Some(processing) = world.get::<MassSpec>(e).map(|m| m.processing) else {
        return false;
    };
    if let Some((kind, _)) = tool_of(world, held) {
        if !matches!(
            kind,
            ToolKind::Wrench | ToolKind::Screwdriver | ToolKind::Crowbar
        ) {
            return false;
        }
        if processing {
            crate::balloon::balloon_alert(world, mob, e, "ещё работает!");
            return true;
        }
        if kind != ToolKind::Wrench {
            return false;
        }
        let actor = Actor {
            client,
            mob,
            item: Some(held),
        };
        let anchored = anchored(world, e);
        use_tool(
            world,
            actor,
            e,
            WRENCH_DS,
            0,
            crate::construction::Job::Unfasten { anchored },
        );
        return true;
    }
    if !container(world, held) {
        return false;
    }
    if processing {
        crate::balloon::balloon_alert(world, mob, e, "ещё работает!");
        return true;
    }
    replace_beaker(world, mob, e, !right, Some(held));
    let slot = if right {
        "выходной"
    } else {
        "входной"
    };
    notice(
        world,
        client,
        format!("Вы вставляете ёмкость в {slot} слот."),
    );
    open(world, client, mob, e);
    true
}

/// `click_alt`: the input beaker out.
pub(super) fn click_alt(world: &mut World, _client: Entity, mob: Entity, e: Entity) -> bool {
    let Some(processing) = world.get::<MassSpec>(e).map(|m| m.processing) else {
        return false;
    };
    if processing {
        crate::balloon::balloon_alert(world, mob, e, "ещё работает!");
    } else {
        replace_beaker(world, mob, e, true, None);
    }
    true
}

/// `process`, every machine tick while running.
// Cost: one pass over the machines per machine tick (2 s).
pub(super) fn process(world: &mut World, mut last: Local<Option<u64>>) {
    let ticks = world.resource::<MachineTicks>().0;
    let from = last.unwrap_or(ticks);
    *last = Some(ticks);
    if from == ticks {
        return;
    }
    let mut q = world.query::<(Entity, &MassSpec)>();
    let running: Vec<Entity> = q
        .iter(world)
        .filter(|(_, m)| m.processing)
        .map(|(e, _)| e)
        .collect();
    for e in running {
        step(world, e);
    }
}

fn step(world: &mut World, e: Entity) {
    let open = world
        .get::<crate::construction::PanelOpen>(e)
        .is_some_and(|p| p.0);
    if world.get::<Unpowered>(e).is_some() || open || !anchored(world, e) {
        return;
    }
    let secs = MACHINE_TICK_SECS as f32;
    let Some(mut m) = world.get::<MassSpec>(e).cloned() else {
        return;
    };
    m.progress_time += secs;
    if m.progress_time < m.delay_time {
        world.entity_mut(e).insert(m);
        let active = world
            .get::<DmPower>(e)
            .map_or(0.0, |p| p.active_power_usage);
        crate::kitchen::machine_use_energy(world, e, active * f64::from(secs));
        return;
    }
    m.processing = false;
    m.progress_time = 0.0;
    m.log.clear();
    let (Some(in_e), Some(out_e)) = (input(world, e), output(world, e)) else {
        world.entity_mut(e).insert(m);
        return;
    };
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let masses = masses(world);
    let (Some(mut a), Some(mut b)) = (holder(world, Some(in_e)), holder(world, Some(out_e))) else {
        return;
    };
    let mass = |id: ReagentId| masses.get(usize::from(id)).copied().unwrap_or(0.0);
    for r in a.reagents.clone() {
        let volume = r.volume;
        if let Some(inverse) = inverted(&chem, r.id, r.purity) {
            if mass(inverse) < m.lower || mass(inverse) > m.upper {
                continue;
            }
            let purity = inverse_purity(&chem, r.id, r.purity);
            a.remove_reagent(r.id, volume);
            b.add_reagent(
                &chem,
                inverse,
                volume,
                a.chem_temp,
                Some(purity),
                Some(r.ph),
                false,
                false,
            );
            m.log.insert(
                r.id,
                format!("Purified to {}%", crate::chem::dm_num(purity * 100.0)),
            );
            continue;
        }
        if mass(r.id) < m.lower || mass(r.id) > m.upper {
            continue;
        }
        let pure = chem.def(r.id).purity;
        let delta = pure - r.purity;
        if delta <= 0.0 {
            continue;
        }
        a.remove_reagent(r.id, volume);
        b.add_reagent(
            &chem,
            r.id,
            volume * (1.0 - delta),
            a.chem_temp,
            Some(pure),
            Some(r.ph),
            false,
            false,
        );
        m.log.insert(
            r.id,
            format!("Purified to {}%", crate::chem::dm_num(pure * 100.0)),
        );
    }
    world.entity_mut(in_e).insert(Reagents(a));
    world.entity_mut(out_e).insert(Reagents(b));
    world.entity_mut(e).insert(m);
    reset_range(world, e);
}

fn beaker_view(
    chem: &Chemistry,
    masses: &[f32],
    h: &Holder,
    log: Option<&HashMap<ReagentId, String>>,
    peak: &mut f32,
) -> MassSpecBeaker {
    let contents = h
        .reagents
        .iter()
        .map(|r| {
            let volume = round_to(r.volume, ss13_reagents::CHEMICAL_VOLUME_ROUNDING);
            let (target, purity, kind, line) = match log {
                // The output: as it is, with the run's log.
                Some(log) => (
                    r.id,
                    r.purity,
                    "Clean",
                    log.get(&r.id).cloned().unwrap_or_default(),
                ),
                None => match inverted(chem, r.id, r.purity) {
                    Some(inverse) => (
                        inverse,
                        inverse_purity(chem, r.id, r.purity),
                        "Inverted",
                        "Ready".to_owned(),
                    ),
                    None => {
                        let initial = chem.def(r.id).purity;
                        let line = if initial - r.purity <= 0.0 {
                            format!("Cannot purify above {}%", (initial * 100.0).round())
                        } else {
                            "Ready".to_owned()
                        };
                        (r.id, r.purity, "Clean", line)
                    }
                },
            };
            if log.is_none() {
                *peak = peak.max(r.volume);
            }
            MassSpecReagent {
                name: chem.def(target).name.clone(),
                volume,
                mass: masses.get(usize::from(target)).copied().unwrap_or(0.0),
                purity: (purity * 100.0).round(),
                kind: kind.to_owned(),
                log: line,
            }
        })
        .collect();
    MassSpecBeaker {
        current_volume: h.total_volume,
        max_volume: h.maximum_volume,
        contents,
    }
}

pub(super) fn view(world: &mut World, e: Entity, user: Entity) -> Option<MassSpecData> {
    let m = world.get::<MassSpec>(e)?.clone();
    let chem = world.get_resource::<Chem>()?.0.clone();
    let masses = masses(world);
    let a = holder(world, input(world, e));
    let b = holder(world, output(world, e));
    let mut peak = 0.0;
    let beaker1 = a
        .as_ref()
        .map(|h| beaker_view(&chem, &masses, h, None, &mut peak));
    let beaker2 = b
        .as_ref()
        .map(|h| beaker_view(&chem, &masses, h, Some(&m.log), &mut peak));
    let has_beaker_in_hand = held_item(world, user).is_some_and(|h| container(world, h));
    Some(MassSpecData {
        lower_range: m.lower,
        upper_range: m.upper,
        processing: m.processing,
        eta: m.delay_time - m.progress_time,
        peak_height: peak,
        graph_upper_range: calculate_mass(&chem, &masses, a.as_ref(), false),
        has_beaker_in_hand,
        beaker1,
        beaker2,
    })
}

impl ServerUi for MassSpecUi {
    type Source = &'static Views<MassSpecData>;
    type UserParam = ();
    type Act = MassSpecAct;

    fn static_data(_: &Views<MassSpecData>) {}

    fn data(views: &Views<MassSpecData>, user: Entity, _: &()) -> MassSpecData {
        views.of(user)
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<MassSpecAct, UiReject> {
        let value = || params.number("value").map(|v| v as f32);
        Ok(match action {
            "activate" => MassSpecAct::Activate,
            "leftSlider" => MassSpecAct::Left(value()?),
            "rightSlider" => MassSpecAct::Right(value()?),
            "centerSlider" => MassSpecAct::Center(value()?),
            "eject1" => MassSpecAct::Eject(true),
            "eject2" => MassSpecAct::Eject(false),
            "insert1" => MassSpecAct::Insert(true),
            "insert2" => MassSpecAct::Insert(false),
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum MassSpecAct {
    Activate,
    Left(f32),
    Right(f32),
    Center(f32),
    /// `true`: the input.
    Eject(bool),
    Insert(bool),
}

pub(super) fn acts(world: &mut World, mut cursor: Local<MessageCursor<UiActed<MassSpecUi>>>) {
    let pending: Vec<(Entity, Entity, MassSpecAct)> = cursor
        .read(world.resource::<Messages<UiActed<MassSpecUi>>>())
        .map(|a| (a.actor, a.source, a.act.clone()))
        .collect();
    for (actor, e, act) in pending {
        apply_act(world, actor, e, act);
    }
}

/// tg `ui_act`: nothing while running; checked at the moment of effect.
pub fn apply_act(world: &mut World, actor: Entity, e: Entity, act: MassSpecAct) {
    let Some(m) = world.get::<MassSpec>(e).cloned() else {
        return;
    };
    if m.processing {
        return;
    }
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let masses = masses(world);
    let h = holder(world, input(world, e));
    let lowest = calculate_mass(&chem, &masses, h.as_ref(), true);
    let highest = calculate_mass(&chem, &masses, h.as_ref(), false);
    let finite = |v: f32| v.is_finite();
    match act {
        MassSpecAct::Activate => {
            if input(world, e).is_none() {
                say(world, e, "Missing input beaker!");
                return;
            }
            if output(world, e).is_none() {
                say(world, e, "Missing output beaker!");
                return;
            }
            estimate(world, e);
            let delay = world.get::<MassSpec>(e).map_or(0.0, |m| m.delay_time);
            if delay <= 0.0 {
                say(world, e, "No work to be done!");
                return;
            }
            if let Some(mut m) = world.get_mut::<MassSpec>(e) {
                m.progress_time = 0.0;
                m.processing = true;
            }
        }
        MassSpecAct::Left(v) if finite(v) => {
            let mid = (m.lower + m.upper) / 2.0;
            if let Some(mut s) = world.get_mut::<MassSpec>(e) {
                s.lower = v.max(lowest).min(mid.max(lowest));
            }
            estimate(world, e);
        }
        MassSpecAct::Right(v) if finite(v) => {
            let mid = (m.lower + m.upper) / 2.0;
            if let Some(mut s) = world.get_mut::<MassSpec>(e) {
                s.upper = v.max(mid).min(highest.max(mid));
            }
            estimate(world, e);
        }
        MassSpecAct::Center(v) if finite(v) => {
            let delta = (m.lower + m.upper) / 2.0 - v;
            if let Some(mut s) = world.get_mut::<MassSpec>(e) {
                s.lower = (m.lower - delta).clamp(lowest, highest.max(lowest));
                s.upper = (m.upper - delta).clamp(lowest, highest.max(lowest));
            }
            estimate(world, e);
        }
        MassSpecAct::Left(_) | MassSpecAct::Right(_) | MassSpecAct::Center(_) => {}
        MassSpecAct::Eject(is_input) => replace_beaker(world, actor, e, is_input, None),
        MassSpecAct::Insert(is_input) => {
            if let Some(held) = held_item(world, actor).filter(|&h| container(world, h)) {
                replace_beaker(world, actor, e, is_input, Some(held));
            }
        }
    }
}

/// `examine`.
pub fn examine_lines(world: &World, e: Entity, lines: &mut Vec<(Span, String)>) {
    if world.get::<MassSpec>(e).is_none() {
        return;
    }
    let num = crate::chem::dm_num;
    match input(world, e).and_then(|b| world.get::<Reagents>(b)) {
        Some(r) => lines.push((
            Span::Notice,
            format!(
                "Вставлена входная ёмкость на {} ед. Извлечь — Alt+ЛКМ.",
                num(r.0.maximum_volume)
            ),
        )),
        None => lines.push((
            Span::Warning,
            "Нет входной ёмкости, вставьте её ЛКМ.".to_owned(),
        )),
    }
    match output(world, e).and_then(|b| world.get::<Reagents>(b)) {
        Some(r) => lines.push((
            Span::Notice,
            format!(
                "Вставлена выходная ёмкость на {} ед.",
                num(r.0.maximum_volume)
            ),
        )),
        None => lines.push((
            Span::Warning,
            "Нет выходной ёмкости, вставьте её ПКМ.".to_owned(),
        )),
    }
    if !anchored(world, e) {
        lines.push((
            Span::Warning,
            "Для работы её нужно прикрутить ключом.".to_owned(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_reagents::{ChemData, Owner, ReagentDef};

    fn reagent(path: &str) -> ReagentDef {
        ReagentDef {
            path: path.into(),
            name: path.into(),
            description: String::new(),
            color: "#AAAAAA".into(),
            taste_description: String::new(),
            taste_mult: 1.0,
            metabolization_rate: 0.2,
            overdose_threshold: 0.0,
            ph: 7.0,
            specific_heat: 200.0,
            chemical_flags: 0,
            purity: 1.0,
            creation_purity: 1.0,
            inverse_chem: None,
            inverse_chem_val: 0.25,
            purge_multiplier: 1.0,
            toxpwr: 0.0,
            health_required: 0.0,
            liver_tolerance_multiplier: 1.0,
            boozepwr: 0.0,
            brute_heal: 0.0,
            nutriment_factor: 0.0,
            penetrates_skin: 4,
            default_container: String::new(),
            addiction_types: Vec::new(),
        }
    }

    #[test]
    fn range_and_work_time_follow_mass_volume_and_purity() {
        let chem = Chemistry::new(ChemData {
            reagents: vec![reagent("/datum/reagent/a"), reagent("/datum/reagent/b")],
            ..Default::default()
        });
        let masses = [120.0, 430.0];
        let mut h = Holder::new(100.0, 0, Owner::default());
        h.add_reagent(&chem, 0, 30.0, 300.0, Some(0.5), None, true, false);
        h.add_reagent(&chem, 1, 10.0, 300.0, Some(1.0), None, true, false);
        // Floored and ceiled to 50.
        assert_eq!(calculate_mass(&chem, &masses, Some(&h), true), 100.0);
        assert_eq!(calculate_mass(&chem, &masses, Some(&h), false), 450.0);
        // Only the impure one: (120 × 30) × 0.0035 + 10 = 22.6 s; two lasers of tier 2: / 4.
        let t = estimate_time(&chem, &masses, Some(&h), 0.0, 1000.0, 1.0);
        assert!((t - 22.6).abs() < 1e-3, "{t}");
        let t = estimate_time(&chem, &masses, Some(&h), 0.0, 1000.0, 0.25);
        assert!((t - 5.65).abs() < 1e-3, "{t}");
        // Out of the range: nothing to do.
        assert_eq!(
            estimate_time(&chem, &masses, Some(&h), 200.0, 1000.0, 1.0),
            0.0
        );
    }
}
