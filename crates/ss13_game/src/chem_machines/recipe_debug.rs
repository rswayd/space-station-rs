//! The chemical reaction tester (server): tg `/obj/machinery/chem_recipe_debug`
//! (`chem_recipe_debug.dm`) and its `ChemRecipeDebug` window. An admin tool: every click, act
//! and answer needs the client's `R_ADMIN`.
//!
//! - `pick_reaction` asks "All" or "Specific" (then one reaction by name) for the test list;
//!   `start_reaction` plays the next, previous or a picked one of the list: its 9000 u holder is
//!   emptied, takes the reaction's catalysts (×1) and required reagents (× `volume_multiplier`)
//!   at the chosen temperature, purity and pH, and reacts.
//! - While it is not reacting (every machine tick) and with every reaction step, the holder is
//!   pushed towards the temperature of the mode (the reaction's own, a forced one, the
//!   reaction's minimum, optimal or overheat) at 0.4 of the gap a second, and the forced purity
//!   and pH are imposed.
//! - `edit_reaction` picks a reaction whose vars (`EDIT_VARS`) the window edits for the whole
//!   round; `reset_value` restores one, `export` writes them to the server log.
//! - `eject` puts the result into the inserted container (or a new bluespace beaker) in hand.
// shortcut: a reaction's `required_container` is given as the holder's owner, no soup pot
// ingredients; `export` goes to the server log, not `chem_parse.txt`.

use std::collections::HashMap;

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmObj, DmReagents};
use ss13_reagents::{
    CHEMICAL_MAX_PH, CHEMICAL_MIN_PH, CHEMICAL_NORMAL_PH, Chemistry, DEFAULT_REAGENT_TEMPERATURE,
    Holder, NO_OVERHEAT, Owner, ReactionDef, round_to,
};
use ss13_tgui::prompt::{PromptAnswer, PromptKind};
use ss13_tgui::server::{OpenUi, ServerUi, UiActed, UiParams, UiReject};

use super::*;
use crate::chem::{Chem, Reagents};
use crate::obj_damage::is_under;
use crate::power::{MACHINE_TICK_SECS, MachineTicks};

pub const RECIPE_DEBUG: &str = "/obj/machinery/chem_recipe_debug";
/// `MAXIMUM_HOLDER_VOLUME`.
const HOLDER_VOLUME: f32 = 9000.0;

#[derive(Component, Clone, Debug, PartialEq)]
#[require(Views<RecipeDebugData>)]
pub struct RecipeDebug {
    pub forced_temp: f32,
    pub temp_mode: u8,
    pub forced_ph: f32,
    pub use_forced_ph: bool,
    pub forced_purity: f32,
    pub use_forced_purity: bool,
    pub volume_multiplier: f32,
    /// `reactions_to_test`: indexes into `Chemistry::reactions`.
    pub to_test: Vec<u32>,
    /// `current_reaction_index`, 1-based (0: none yet).
    pub index: usize,
    pub mode: u8,
    pub edit: Option<u32>,
    pub edit_var: &'static str,
    pub container: Option<Entity>,
}

/// The reactions' vars as they were before the tester edited them (`initial()`).
#[derive(Resource, Default)]
pub struct OriginalReactions(pub HashMap<u32, ReactionDef>);

pub(super) fn ensure(world: &mut World, e: Entity) -> bool {
    if world.get::<RecipeDebug>(e).is_some() {
        return true;
    }
    let Some(path) = world.get::<DmObj>(e).map(|o| o.path.clone()) else {
        return false;
    };
    if !is_under(&path, RECIPE_DEBUG) {
        return false;
    }
    world.entity_mut(e).insert((
        RecipeDebug {
            forced_temp: DEFAULT_REAGENT_TEMPERATURE,
            temp_mode: 0,
            forced_ph: CHEMICAL_NORMAL_PH,
            use_forced_ph: false,
            forced_purity: 1.0,
            use_forced_purity: false,
            volume_multiplier: 1.0,
            to_test: Vec::new(),
            index: 0,
            mode: 0,
            edit: None,
            edit_var: EDIT_VARS[0],
            container: None,
        },
        Reagents(Holder::new(
            HOLDER_VOLUME,
            0,
            Owner {
                path,
                living: false,
            },
        )),
    ));
    true
}

/// `extract_reaction_name`: the last path segment, words capitalized.
pub fn reaction_name(path: &str) -> String {
    path.rsplit('/')
        .next()
        .unwrap_or_default()
        .split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `all_reaction_list`: one reaction per name (the last of a name wins, as in the assoc list),
/// sorted by name.
fn all_reactions(chem: &Chemistry) -> Vec<(String, u32)> {
    let mut by_name: std::collections::BTreeMap<String, u32> = Default::default();
    for (i, r) in chem.reactions.iter().enumerate() {
        by_name.insert(reaction_name(&r.def.path), i as u32);
    }
    by_name.into_iter().collect()
}

/// The reaction var a window label names (`decode_var`).
fn var_mut<'a>(def: &'a mut ReactionDef, var: &str) -> Option<&'a mut f32> {
    Some(match var {
        "Required Temp" => &mut def.required_temp,
        "Optimal Temp" => &mut def.optimal_temp,
        "Overheat Temp" => &mut def.overheat_temp,
        "Optimal Min Ph" => &mut def.optimal_ph_min,
        "Optimal Max Ph" => &mut def.optimal_ph_max,
        "Ph Range" => &mut def.determin_ph_range,
        "Temp Exp Factor" => &mut def.temp_exponent_factor,
        "Ph Exp Factor" => &mut def.ph_exponent_factor,
        "Thermic Constant" => &mut def.thermic_constant,
        "H Ion Release" => &mut def.h_ion_release,
        "Rate Up Limit" => &mut def.rate_up_lim,
        "Purity Min" => &mut def.purity_min,
        _ => return None,
    })
}

fn var_of(def: &ReactionDef, var: &str) -> f32 {
    let mut copy = def.clone();
    var_mut(&mut copy, var).map_or(0.0, |v| *v)
}

/// `decode_target_temperature`: `None` leaves the temperature alone.
fn target_temperature(chem: &Chemistry, d: &RecipeDebug) -> Option<f32> {
    match d.temp_mode {
        0 => None,
        1 => Some(d.forced_temp),
        mode => {
            let i = *d.to_test.get(d.index.max(1) - 1)?;
            let def = &chem.reactions.get(i as usize)?.def;
            Some(match mode {
                2 => def.required_temp,
                3 => def.optimal_temp,
                _ => def.overheat_temp,
            })
        }
    }
}

/// `adjust_environment(seconds)`.
fn adjust_environment(chem: &Chemistry, d: &RecipeDebug, h: &mut Holder, seconds: f32) {
    if let Some(target) = target_temperature(chem, d) {
        let energy = (target - h.chem_temp) * 0.4 * seconds * h.heat_capacity(chem);
        h.adjust_thermal_energy(
            chem,
            energy,
            0.0,
            ss13_reagents::CHEMICAL_MAXIMUM_TEMPERATURE,
        );
    }
    if d.use_forced_purity {
        for r in &mut h.reagents {
            r.purity = d.forced_purity;
        }
    }
    if d.use_forced_ph {
        for r in &mut h.reagents {
            r.ph = d.forced_ph.clamp(CHEMICAL_MIN_PH, CHEMICAL_MAX_PH);
        }
    }
    h.update_total();
}

/// `process`: a holder that is not reacting is adjusted and reacts.
// Cost: one pass over the testers (admin-spawned, rarely any) per machine tick.
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
    let mut q = world.query::<(Entity, &RecipeDebug, &Reagents)>();
    let idle: Vec<(Entity, RecipeDebug, Holder)> = q
        .iter(world)
        .filter(|(_, _, r)| !r.0.is_reacting && r.0.total_volume > 0.0)
        .map(|(e, d, r)| (e, d.clone(), r.0.clone()))
        .collect();
    for (e, d, mut h) in idle {
        adjust_environment(&chem, &d, &mut h, MACHINE_TICK_SECS as f32);
        h.handle_reactions(&chem);
        world.entity_mut(e).insert(Reagents(h));
    }
}

/// `on_reaction_step` (from `crate::chem`'s reaction processing, before the step).
pub fn reaction_step(world: &mut World, e: Entity, seconds: f32) {
    let Some(d) = world.get::<RecipeDebug>(e).cloned() else {
        return;
    };
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    if let Some(mut r) = world.get_mut::<Reagents>(e) {
        adjust_environment(&chem, &d, &mut r.0, seconds);
    }
}

pub(super) fn interact(
    world: &mut World,
    client: Entity,
    mob: Entity,
    e: Entity,
    right: bool,
) -> bool {
    if world.get::<RecipeDebug>(e).is_none() {
        return false;
    }
    if !is_admin(world, client) {
        crate::tools::tell(
            world,
            client,
            ss13_shared::chat::Span::Warning,
            "Если вы это видите — позовите администрацию.",
        );
        return true;
    }
    if !right {
        world.write_message(OpenUi::new::<RecipeDebugUi>(
            client,
            mob,
            e,
            title(world, e),
        ));
    }
    true
}

pub(super) fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    e: Entity,
) -> bool {
    if world.get::<RecipeDebug>(e).is_none() {
        return false;
    }
    let open_container = world.get::<DmReagents>(held).is_some()
        && world
            .get::<Reagents>(held)
            .is_some_and(|r| r.0.is_open_container());
    if !open_container || !is_admin(world, client) {
        return false;
    }
    if let Some(old) = inside(
        world,
        e,
        world.get::<RecipeDebug>(e).and_then(|d| d.container),
    ) {
        hand_out(world, Some(mob), e, old);
    }
    take_in(world, mob, e, held);
    if let Some(mut d) = world.get_mut::<RecipeDebug>(e) {
        d.container = Some(held);
    }
    true
}

pub(super) fn view(world: &mut World, e: Entity, _user: Entity) -> Option<RecipeDebugData> {
    let d = world.get::<RecipeDebug>(e)?.clone();
    let chem = world.get_resource::<Chem>()?.0.clone();
    let h = world.get::<Reagents>(e)?.0.clone();
    let current = d
        .to_test
        .get(d.index.max(1) - 1)
        .and_then(|&i| chem.reactions.get(i as usize));
    let (active, flashing) = crate::chem_heater::active_reactions(&chem, &h);
    let active_reactions = active
        .into_iter()
        .map(|r| DebugReaction {
            name: r.name,
            danger: r.danger,
            overheat: r.overheat,
            purity_alert: r.purity_alert,
            quality: r.quality,
            inverse: r.inverse,
            min_pure: r.min_pure,
            reacted_vol: r.reacted_vol,
            target_vol: r.target_vol,
        })
        .collect();
    let edit_reaction = d
        .edit
        .and_then(|i| chem.reactions.get(i as usize))
        .map(|r| {
            let name = r
                .results
                .first()
                .map_or_else(|| r.def.path.clone(), |&(id, _)| chem.def(id).name.clone());
            EditReaction {
                name,
                edit_var: d.edit_var.to_owned(),
                edit_value: var_of(&r.def, d.edit_var),
            }
        });
    let has_container = inside(world, e, d.container).is_some();
    let beaker = (has_container || h.total_volume > 0.0).then(|| DebugBeaker {
        max_volume: h.maximum_volume,
        ph: round_to(h.ph, 0.01),
        current_volume: h.total_volume,
        current_temp: h.chem_temp.round(),
        purity: round_to(h.average_purity(), 0.001),
        contents: lines(&chem, &h),
    });
    Some(RecipeDebugData {
        forced_temp: d.forced_temp,
        temp_mode: d.temp_mode,
        forced_ph: d.forced_ph,
        use_forced_ph: d.use_forced_ph,
        forced_purity: d.forced_purity,
        use_forced_purity: d.use_forced_purity,
        volume_multiplier: d.volume_multiplier,
        current_reaction_name: current
            .map_or_else(|| "N/A".to_owned(), |r| reaction_name(&r.def.path)),
        current_reaction_mode: d.mode,
        active_reactions,
        is_flashing: flashing,
        is_reacting: h.is_reacting,
        edit_reaction,
        beaker,
    })
}

impl ServerUi for RecipeDebugUi {
    type Source = &'static Views<RecipeDebugData>;
    type UserParam = ();
    type Act = DebugAct;

    fn static_data(_: &Views<RecipeDebugData>) {}

    fn data(views: &Views<RecipeDebugData>, user: Entity, _: &()) -> RecipeDebugData {
        views.of(user)
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<DebugAct, UiReject> {
        let number = || params.number("target").map(|v| v as f32);
        let pick = |list: &[&'static str]| -> Result<u8, UiReject> {
            let text = params.text("target")?;
            list.iter()
                .position(|m| *m == text)
                .map(|i| i as u8)
                .ok_or_else(|| UiReject::BadParams("target: unknown".into()))
        };
        Ok(match action {
            "forced_temp" => DebugAct::ForcedTemp(number()?),
            "temp_mode" => DebugAct::TempMode(pick(&TEMP_MODES)?),
            "forced_ph" => DebugAct::ForcedPh(number()?),
            "toggle_forced_ph" => DebugAct::ToggleForcedPh,
            "forced_purity" => DebugAct::ForcedPurity(number()?),
            "toggle_forced_purity" => DebugAct::ToggleForcedPurity,
            "volume_multiplier" => DebugAct::VolumeMultiplier(number()?),
            "pick_reaction" => DebugAct::PickReaction,
            "reaction_mode" => DebugAct::ReactionMode(pick(&REACTION_MODES)?),
            "start_reaction" => DebugAct::Start,
            "edit_reaction" => DebugAct::EditReaction,
            "edit_var" => DebugAct::EditVar(EDIT_VARS[pick(&EDIT_VARS)? as usize]),
            "edit_value" => DebugAct::EditValue(number()?),
            "reset_value" => DebugAct::ResetValue,
            "export" => DebugAct::Export,
            "eject" => DebugAct::Eject,
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum DebugAct {
    ForcedTemp(f32),
    TempMode(u8),
    ForcedPh(f32),
    ToggleForcedPh,
    ForcedPurity(f32),
    ToggleForcedPurity,
    VolumeMultiplier(f32),
    PickReaction,
    ReactionMode(u8),
    Start,
    EditReaction,
    EditVar(&'static str),
    EditValue(f32),
    ResetValue,
    Export,
    Eject,
}

pub(super) fn acts(world: &mut World, mut cursor: Local<MessageCursor<UiActed<RecipeDebugUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, DebugAct)> = cursor
        .read(world.resource::<Messages<UiActed<RecipeDebugUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, e, act) in pending {
        apply_act(world, client, actor, e, act);
    }
}

fn set(world: &mut World, e: Entity, f: impl FnOnce(&mut RecipeDebug)) {
    if let Some(mut d) = world.get_mut::<RecipeDebug>(e) {
        f(&mut d);
    }
}

/// tg `ui_act`, admins only.
pub fn apply_act(world: &mut World, client: Entity, actor: Entity, e: Entity, act: DebugAct) {
    let Some(d) = world.get::<RecipeDebug>(e).cloned() else {
        return;
    };
    if !is_admin(world, client) {
        return;
    }
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    match act {
        DebugAct::ForcedTemp(v) if v.is_finite() => {
            set(world, e, |d| {
                d.forced_temp = v.clamp(0.0, ss13_reagents::CHEMICAL_MAXIMUM_TEMPERATURE)
            });
        }
        DebugAct::TempMode(m) => set(world, e, |d| d.temp_mode = m),
        DebugAct::ForcedPh(v) if v.is_finite() => set(world, e, |d| d.forced_ph = v),
        DebugAct::ToggleForcedPh => set(world, e, |d| d.use_forced_ph = !d.use_forced_ph),
        DebugAct::ForcedPurity(v) if v.is_finite() => {
            set(world, e, |d| d.forced_purity = v.clamp(0.0, 1.0));
        }
        DebugAct::ToggleForcedPurity => {
            set(world, e, |d| d.use_forced_purity = !d.use_forced_purity);
        }
        DebugAct::VolumeMultiplier(v) if v.is_finite() && v > 0.0 => {
            set(world, e, |d| d.volume_multiplier = v);
        }
        DebugAct::PickReaction => ask(
            world,
            client,
            actor,
            e,
            "Play all or a specific reaction?",
            PromptKind::Alert {
                buttons: vec!["All".into(), "Specific".into()],
                autofocus: true,
            },
            Asked::DebugAllOrOne,
        ),
        DebugAct::ReactionMode(m) => set(world, e, |d| d.mode = m),
        DebugAct::Start => {
            let len = d.to_test.len();
            if len > 1 {
                match d.mode {
                    0 => set(world, e, |d| d.index = d.index % len + 1),
                    1 => set(world, e, |d| d.index = d.index.saturating_sub(1).max(1)),
                    _ => {
                        let items = d
                            .to_test
                            .iter()
                            .map(|&i| reaction_name(&chem.reactions[i as usize].def.path))
                            .collect();
                        ask(
                            world,
                            client,
                            actor,
                            e,
                            "Select Reaction",
                            PromptKind::List {
                                items,
                                default: None,
                            },
                            Asked::DebugPlay,
                        );
                        return;
                    }
                }
            } else if len == 1 {
                set(world, e, |d| d.index = 1);
            } else {
                return;
            }
            start(world, e);
        }
        DebugAct::EditReaction => {
            let all = all_reactions(&chem);
            let items = all.iter().map(|(n, _)| n.clone()).collect();
            ask(
                world,
                client,
                actor,
                e,
                "Select Reaction",
                PromptKind::List {
                    items,
                    default: None,
                },
                Asked::DebugReaction {
                    edit: true,
                    listed: all.into_iter().map(|(_, i)| i).collect(),
                },
            );
        }
        DebugAct::EditVar(var) => set(world, e, |d| d.edit_var = var),
        DebugAct::EditValue(v) if v.is_finite() => {
            if let Some(i) = d.edit {
                edit(world, i, d.edit_var, Some(v));
            }
        }
        DebugAct::ResetValue => {
            if let Some(i) = d.edit {
                edit(world, i, d.edit_var, None);
            }
        }
        DebugAct::Export => {
            let Some(r) = d.edit.and_then(|i| chem.reactions.get(i as usize)) else {
                return;
            };
            let mut text = format!(
                "{}\n\tis_cold_recipe = {}\n",
                r.def.path,
                u8::from(r.def.is_cold_recipe)
            );
            for (var, label) in [
                ("required_temp", "Required Temp"),
                ("optimal_temp", "Optimal Temp"),
                ("overheat_temp", "Overheat Temp"),
                ("optimal_ph_min", "Optimal Min Ph"),
                ("optimal_ph_max", "Optimal Max Ph"),
                ("determin_ph_range", "Ph Range"),
                ("temp_exponent_factor", "Temp Exp Factor"),
                ("ph_exponent_factor", "Ph Exp Factor"),
                ("thermic_constant", "Thermic Constant"),
                ("H_ion_release", "H Ion Release"),
                ("rate_up_lim", "Rate Up Limit"),
                ("purity_min", "Purity Min"),
            ] {
                text.push_str(&format!("\t{var} = {}\n", var_of(&r.def, label)));
            }
            info!("chem_parse:\n{text}");
            notice(world, client, "Сохранено в лог сервера (chem_parse).");
        }
        DebugAct::Eject => eject(world, actor, e),
        DebugAct::ForcedTemp(_)
        | DebugAct::ForcedPh(_)
        | DebugAct::ForcedPurity(_)
        | DebugAct::VolumeMultiplier(_)
        | DebugAct::EditValue(_) => {}
    }
}

/// Sets (`Some`) or restores (`None`, `initial()`) a var of reaction `i` for the round.
fn edit(world: &mut World, i: u32, var: &str, value: Option<f32>) {
    let Some(original) = world
        .get_resource::<Chem>()
        .and_then(|c| c.0.reactions.get(i as usize))
        .map(|r| r.def.clone())
    else {
        return;
    };
    let initial = world
        .get_resource_or_init::<OriginalReactions>()
        .0
        .entry(i)
        .or_insert(original)
        .clone();
    let new = match value {
        Some(v) => v,
        None => var_of(&initial, var),
    };
    let mut chem = world.resource_mut::<Chem>();
    let chemistry = std::sync::Arc::make_mut(&mut chem.0);
    if let Some(slot) = chemistry
        .reactions
        .get_mut(i as usize)
        .and_then(|r| var_mut(&mut r.def, var))
    {
        *slot = new;
    }
}

/// `start_reaction` of the reaction at `index`.
fn start(world: &mut World, e: Entity) {
    let Some(d) = world.get::<RecipeDebug>(e).cloned() else {
        return;
    };
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let Some(&i) = d.to_test.get(d.index.max(1) - 1) else {
        return;
    };
    let Some(reaction) = chem.reactions.get(i as usize) else {
        return;
    };
    let Some(mut h) = world.get::<Reagents>(e).map(|r| r.0.clone()) else {
        return;
    };
    h.force_stop_reacting(&chem);
    h.clear();
    let machine_path = world
        .get::<DmObj>(e)
        .map(|o| o.path.clone())
        .unwrap_or_default();
    h.owner.path = reaction
        .def
        .required_container
        .clone()
        .unwrap_or(machine_path);
    let temp = if d.temp_mode == 0 {
        DEFAULT_REAGENT_TEMPERATURE
    } else {
        target_temperature(&chem, &d).unwrap_or(DEFAULT_REAGENT_TEMPERATURE)
    };
    let temp = if temp == NO_OVERHEAT {
        DEFAULT_REAGENT_TEMPERATURE
    } else {
        temp
    };
    let purity = d.use_forced_purity.then_some(d.forced_purity);
    let ph = d.use_forced_ph.then_some(d.forced_ph);
    for &(id, amount) in &reaction.catalysts {
        h.add_reagent(&chem, id, amount, temp, purity, ph, true, false);
    }
    for &(id, amount) in &reaction.required {
        let multiplier = if reaction.catalysts.iter().any(|&(c, _)| c == id) {
            1.0
        } else {
            d.volume_multiplier
        };
        h.add_reagent(
            &chem,
            id,
            amount * multiplier,
            temp,
            purity,
            ph,
            true,
            false,
        );
    }
    h.handle_reactions(&chem);
    world.entity_mut(e).insert(Reagents(h));
}

/// `eject`: the result into the container (a new bluespace beaker without one), to the hands.
fn eject(world: &mut World, actor: Entity, e: Entity) {
    let Some(d) = world.get::<RecipeDebug>(e).cloned() else {
        return;
    };
    let container = match inside(world, e, d.container) {
        Some(c) => c,
        None => {
            let Some(c) =
                crate::construction::spawn_detached(world, super::synthesizer::BLUESPACE_BEAKER)
            else {
                return;
            };
            crate::storage::put_inside(world, c, e);
            c
        }
    };
    if let Some(mut r) = world.get_mut::<Reagents>(container) {
        r.0.clear();
    }
    let total = world.get::<Reagents>(e).map_or(0.0, |r| r.0.total_volume);
    crate::chem::transfer(world, e, container, total);
    set(world, e, |d| d.container = None);
    hand_out(world, Some(actor), e, container);
}

/// The answers to the tester's prompts (from [`super::prompt_answers`]).
pub(super) fn answer(
    world: &mut World,
    client: Entity,
    actor: Entity,
    e: Entity,
    asked: Asked,
    answer: PromptAnswer,
) {
    if world.get::<RecipeDebug>(e).is_none() {
        return;
    }
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    match (asked, answer) {
        (Asked::DebugAllOrOne, PromptAnswer::Button(0)) => {
            let all: Vec<u32> = all_reactions(&chem).into_iter().map(|(_, i)| i).collect();
            set(world, e, |d| {
                d.to_test = all;
                d.index = 0;
            });
        }
        (Asked::DebugAllOrOne, PromptAnswer::Button(_)) => {
            let all = all_reactions(&chem);
            let items = all.iter().map(|(n, _)| n.clone()).collect();
            ask(
                world,
                client,
                actor,
                e,
                "Select Reaction",
                PromptKind::List {
                    items,
                    default: None,
                },
                Asked::DebugReaction {
                    edit: false,
                    listed: all.into_iter().map(|(_, i)| i).collect(),
                },
            );
        }
        (Asked::DebugReaction { edit, listed }, PromptAnswer::Item(n)) => {
            let Some(&i) = listed.get(n) else {
                return;
            };
            if edit {
                set(world, e, |d| {
                    d.edit = Some(i);
                    d.edit_var = EDIT_VARS[0];
                });
            } else {
                set(world, e, |d| {
                    d.to_test = vec![i];
                    d.index = 0;
                });
            }
        }
        (Asked::DebugPlay, PromptAnswer::Item(n)) => {
            let len = world.get::<RecipeDebug>(e).map_or(0, |d| d.to_test.len());
            if n < len {
                set(world, e, |d| d.index = n + 1);
                start(world, e);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reaction_names_read_like_the_window() {
        assert_eq!(
            reaction_name("/datum/chemical_reaction/medicine/salbutamol_fast"),
            "Salbutamol Fast"
        );
        assert_eq!(reaction_name("/datum/chemical_reaction/water"), "Water");
    }
}
