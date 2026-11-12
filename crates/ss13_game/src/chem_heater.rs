//! The reaction chamber (server): tg `/obj/machinery/chem_heater`
//! (`reagents/chemistry/machinery/chem_heater.dm`) and its `ChemHeater` window.
//!
//! - An open container clicked on it goes in (the old one to the user's hands) and opens the
//!   window; a syringe on it with a beaker inside injects into or draws from the beaker; a right
//!   click with an empty hand takes the beaker out.
//! - Turned on and powered, it moves the beaker's temperature towards the target: `(target -
//!   temp) * heater_coefficient * seconds * heat_capacity` joules, `heater_coefficient` 0.1 times
//!   each micro-laser's tier. Every machine tick (2 s) when the beaker is not reacting, then its
//!   reactions start; every reaction step (0.25 s) while it reacts.
//! - Its own 200 u holder never reacts and keeps acidic and basic buffers: `acidBuffer` and
//!   `basicBuffer` inject `dispense_volume` (1-10) into the beaker, or (`target` < 0) draw the
//!   beaker's buffer up to half the holder.
//! - The window shows the beaker, its temperature and pH and every running reaction with its
//!   danger (overheating, purity under `purity_min`), quality and progress.
// shortcut: no pH colour gauge (see the window); the debug chamber is not upgraded (tg sets its
// coefficient by hand in `Initialize`).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::{DmName, DmObj, DmPower, DmReagents};
use ss13_reagents::{Chemistry, Holder, NO_OVERHEAT, Owner, flags, round_to};
use ss13_shared::chat::Span;
use ss13_shared::protocol::ClickModifiers;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::chem_heater::{
    ActiveReaction, ChemHeaterData, ChemHeaterUi, HeaterBeaker,
};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use crate::chem::{Chem, Reagents};
use crate::chem_dispenser::{InsertedBeaker, beaker_of, chem_container, replace_beaker};
use crate::inventory::Inventory;
use crate::obj_damage::is_under;
use crate::power::{MACHINE_TICK_SECS, MachineTicks, Unpowered};
use crate::stock_parts::PartKind;
use crate::storage::StoredIn;

pub const HEATER_ROOT: &str = "/obj/machinery/chem_heater";
const WITH_BUFFER: &str = "/obj/machinery/chem_heater/withbuffer";
const DEBUG: &str = "/obj/machinery/chem_heater/debug";
pub const ACIDIC_BUFFER: &str = "/datum/reagent/reaction_agent/acidic_buffer";
pub const BASIC_BUFFER: &str = "/datum/reagent/reaction_agent/basic_buffer";
const SYRINGE: &str = "/obj/item/reagent_containers/syringe";
const DROPPER: &str = "/obj/item/reagent_containers/dropper";
/// `create_reagents(200, NO_REACT)`.
const BUFFER_VOLUME: f32 = 200.0;
/// `heater_coefficient` before the micro-lasers.
const BASE_COEFFICIENT: f32 = 0.1;
const MAX_TARGET: f32 = 1000.0;

/// A reaction chamber's live state; its buffers are its [`Reagents`].
#[derive(Component, Clone, Debug, PartialEq)]
#[require(InsertedBeaker)]
pub struct ChemHeater {
    pub on: bool,
    pub target_temperature: f32,
    pub heater_coefficient: f32,
    pub dispense_volume: f32,
}

pub struct ChemHeaterPlugin;

impl Plugin for ChemHeaterPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::EmptyHand, 1500, |w, c| {
            crate::chem_heater::interact(w, c.client, c.mob, c.target, c.modifiers.right)
        });
        app.add_click_handler(ClickPhase::HeldOnTarget, 2600, |w, c| {
            c.held.is_some_and(|h| {
                crate::chem_heater::item_interaction(w, c.client, c.mob, h, c.target, c.modifiers)
            })
        });
        app.register_ui::<HeaterWindow>()
            .add_systems(
                ss13_world::MapInit,
                setup_mapped
                    .after(crate::power::init_power)
                    .after(crate::mecha::core::MechaLoaded),
            )
            .add_systems(
                Update,
                (
                    heater_acts.in_set(UiSystems::Handle),
                    process_heaters.after(crate::power::PowerSystems),
                ),
            );
    }
}

pub fn is_heater(path: &str) -> bool {
    is_under(path, HEATER_ROOT)
}

fn setup_mapped(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &DmObj), With<ss13_world::Mapped>>();
    let found: Vec<Entity> = query
        .iter(world)
        .filter(|(_, o)| is_heater(&o.path))
        .map(|(e, _)| e)
        .collect();
    for e in found {
        setup(world, e);
    }
}

/// `Initialize`: the buffer holder (with the starting buffers of the `withbuffer` and `debug`
/// kinds), then `RefreshParts`.
pub fn setup(world: &mut World, e: Entity) {
    let Some(path) = world.get::<DmObj>(e).map(|o| o.path.clone()) else {
        return;
    };
    crate::stock_parts::ensure_parts(world, e);
    let debug = is_under(&path, DEBUG);
    let mut holder = Holder::new(
        if debug { 2000.0 } else { BUFFER_VOLUME },
        flags::NO_REACT,
        Owner {
            path: path.clone(),
            living: false,
        },
    );
    let start = if debug {
        1000.0
    } else if is_under(&path, WITH_BUFFER) {
        20.0
    } else {
        0.0
    };
    if start > 0.0
        && let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone())
    {
        let list: Vec<_> = [BASIC_BUFFER, ACIDIC_BUFFER]
            .iter()
            .filter_map(|p| chem.id(p).map(|id| (id, start)))
            .collect();
        holder.add_reagent_list(&chem, &list, None, true);
    }
    world.entity_mut(e).insert((
        ChemHeater {
            on: false,
            target_temperature: 300.0,
            heater_coefficient: BASE_COEFFICIENT,
            dispense_volume: 1.0,
        },
        Reagents(holder),
    ));
    refresh_parts(world, e);
}

/// tg `chem_heater/RefreshParts`: 0.1 times each micro-laser's tier.
pub fn refresh_parts(world: &mut World, e: Entity) {
    if world.get::<ChemHeater>(e).is_none() {
        return;
    }
    let r = crate::stock_parts::ratings(world, e);
    let coefficient = r
        .tiers(PartKind::MicroLaser)
        .fold(BASE_COEFFICIENT, |c, t| c * t as f32);
    if let Some(mut h) = world.get_mut::<ChemHeater>(e) {
        h.heater_coefficient = coefficient;
    }
}

fn operational(world: &World, e: Entity) -> bool {
    world.get::<Unpowered>(e).is_none()
}

/// `update_icon_state`: `mixer1b` with a beaker and the panel closed, else `mixer0b`.
fn update_icon(world: &mut World, e: Entity) {
    let open = world
        .get::<crate::construction::PanelOpen>(e)
        .is_some_and(|p| p.0);
    let loaded = beaker_of(world, e).is_some() && !open;
    let state = if loaded { "mixer1b" } else { "mixer0b" };
    crate::obj_damage::set_icon_state(world, e, state);
}

/// `item_interaction`: a syringe or dropper works on the beaker inside; a container goes in and
/// the window opens. True when the click was the heater's.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    modifiers: ClickModifiers,
) -> bool {
    if world.get::<ChemHeater>(target).is_none() {
        return false;
    }
    let held_path = world
        .get::<ss13_mapload::DmItem>(held)
        .map(|i| i.path.clone())
        .unwrap_or_default();
    if let Some(beaker) = beaker_of(world, target)
        && (is_under(&held_path, SYRINGE) || is_under(&held_path, DROPPER))
    {
        crate::chem::item_interaction(world, client, mob, held, beaker, modifiers);
        return true;
    }
    if !chem_container(world, held) {
        return false;
    }
    replace_beaker(world, mob, target, Some(held));
    update_icon(world, target);
    world.write_message(OpenUi::new::<HeaterWindow>(
        client,
        mob,
        target,
        title(world, target),
    ));
    crate::tools::tell(world, client, Span::Notice, "ёмкость добавлена");
    true
}

fn title(world: &World, e: Entity) -> String {
    world
        .get::<DmName>(e)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default()
}

/// `attack_hand` (the window) and `attack_hand_secondary` (the beaker out).
pub fn interact(
    world: &mut World,
    client: Entity,
    mob: Entity,
    target: Entity,
    right: bool,
) -> bool {
    if world.get::<ChemHeater>(target).is_none() {
        return false;
    }
    if right {
        replace_beaker(world, mob, target, None);
        update_icon(world, target);
    } else {
        let title = title(world, target);
        world.write_message(OpenUi::new::<HeaterWindow>(client, mob, target, title));
    }
    true
}

/// `heat_reagents`: towards the target, the energy drawn from the area. False when it did not
/// heat (off, unpowered, no beaker or an empty one).
pub fn heat(
    chem: &Chemistry,
    heater: &ChemHeater,
    beaker: &mut Holder,
    seconds: f32,
) -> Option<f32> {
    if !heater.on || beaker.total_volume == 0.0 {
        return None;
    }
    let energy = (heater.target_temperature - beaker.chem_temp)
        * heater.heater_coefficient
        * seconds
        * beaker.heat_capacity(chem);
    beaker.adjust_thermal_energy(
        chem,
        energy,
        0.0,
        ss13_reagents::CHEMICAL_MAXIMUM_TEMPERATURE,
    );
    Some(energy)
}

/// `use_energy(active_power_usage + abs(ROUND_UP(energy) / 120))`.
fn use_heating_power(world: &mut World, e: Entity, energy: f32) {
    let active = world
        .get::<DmPower>(e)
        .map_or(0.0, |p| p.active_power_usage);
    crate::kitchen::machine_use_energy(world, e, active + f64::from(energy.ceil().abs() / 120.0));
}

/// tg `on_reaction_step`: a reacting beaker inside a heater is heated with every reaction step
/// (from `crate::chem`'s reaction processing, before the step).
pub fn reaction_step(world: &mut World, beaker: Entity, seconds: f32) {
    let Some(machine) = world.get::<StoredIn>(beaker).map(|s| s.0) else {
        return;
    };
    let Some(heater) = world.get::<ChemHeater>(machine).cloned() else {
        return;
    };
    if beaker_of(world, machine) != Some(beaker) || !operational(world, machine) {
        return;
    }
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let Some(mut r) = world.get_mut::<Reagents>(beaker) else {
        return;
    };
    let energy = heat(&chem, &heater, &mut r.0, seconds);
    if let Some(energy) = energy {
        use_heating_power(world, machine, energy);
    }
}

/// `process`: every machine tick a beaker that is not reacting is heated and its reactions
/// start (a reacting one is heated by [`reaction_step`]).
// Cost: one pass over the heaters per machine tick (2 s).
fn process_heaters(world: &mut World, mut last: Local<Option<u64>>) {
    let ticks = world.resource::<MachineTicks>().0;
    let from = last.unwrap_or(ticks);
    *last = Some(ticks);
    if from == ticks {
        return;
    }
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let mut q = world.query::<(Entity, &ChemHeater)>();
    let heaters: Vec<(Entity, ChemHeater)> = q.iter(world).map(|(e, h)| (e, h.clone())).collect();
    for _ in from..ticks {
        for (e, heater) in &heaters {
            if !operational(world, *e) {
                continue;
            }
            let Some(beaker) = beaker_of(world, *e) else {
                continue;
            };
            let Some(mut holder) = world.get::<Reagents>(beaker).map(|r| r.0.clone()) else {
                continue;
            };
            if holder.is_reacting {
                continue;
            }
            let Some(energy) = heat(&chem, heater, &mut holder, MACHINE_TICK_SECS as f32) else {
                continue;
            };
            holder.handle_reactions(&chem);
            world.entity_mut(beaker).insert(Reagents(holder));
            use_heating_power(world, *e, energy);
        }
    }
}

pub struct HeaterWindow;

impl UiInterface for HeaterWindow {
    const NAME: &'static str = ChemHeaterUi::NAME;
    type Data = ChemHeaterData;
    type StaticData = ();
}

#[derive(Clone, Debug, PartialEq)]
pub enum HeaterAct {
    Power,
    Temperature(f32),
    Eject,
    Insert,
    AcidBuffer(f32),
    BasicBuffer(f32),
    DispenseVolume(f32),
}

/// `activeReactions` and `isFlashing` of a beaker's running reactions.
pub fn active_reactions(chem: &Chemistry, h: &Holder) -> (Vec<ActiveReaction>, bool) {
    let mut out: Vec<ActiveReaction> = Vec::new();
    let mut flashing = false;
    for eq in &h.reactions {
        let reaction = &chem.reactions[eq.reaction as usize];
        let Some(&(product, _)) = reaction.results.first() else {
            continue;
        };
        let Some(reagent) = h.has(product) else {
            continue;
        };
        let def = &reaction.def;
        let mut danger = false;
        let mut overheat = false;
        let purity_alert = reagent.purity < def.purity_min;
        if purity_alert {
            danger = true;
        }
        if def.optimal_ph_min > h.ph || def.optimal_ph_max < h.ph {
            flashing = true;
        }
        let hot = if def.is_cold_recipe {
            def.overheat_temp > h.chem_temp && def.overheat_temp != NO_OVERHEAT
        } else {
            def.overheat_temp < h.chem_temp
        };
        if hot {
            danger = true;
            overheat = true;
        }
        let name = chem.def(product).name.clone();
        let competitive =
            def.reaction_flags & ss13_reagents::reaction_flags::REACTION_COMPETITIVE != 0;
        // A competitive reaction for a product already listed merges into its entry.
        if competitive && let Some(entry) = out.iter_mut().find(|r| r.name == name) {
            entry.reacted_vol = eq.reacted_vol();
            entry.target_vol = eq.target_vol().round();
            entry.quality = (entry.quality + eq.reaction_quality) / 2.0;
            continue;
        }
        out.push(ActiveReaction {
            name,
            danger,
            overheat,
            purity_alert,
            quality: eq.reaction_quality,
            inverse: chem.def(product).inverse_chem_val,
            min_pure: def.purity_min,
            reacted_vol: eq.reacted_vol(),
            target_vol: eq.target_vol().round(),
        });
    }
    (out, flashing)
}

impl ServerUi for HeaterWindow {
    type Source = (
        Entity,
        &'static ChemHeater,
        &'static Reagents,
        &'static InsertedBeaker,
    );
    type UserParam = (
        Query<'static, 'static, &'static Reagents>,
        Query<'static, 'static, &'static DmReagents>,
        Query<'static, 'static, &'static Inventory>,
        Query<'static, 'static, &'static StoredIn>,
        Option<Res<'static, Chem>>,
    );
    type Act = HeaterAct;

    fn static_data(_: (Entity, &ChemHeater, &Reagents, &InsertedBeaker)) {}

    fn data(
        (e, heater, buffer, inserted): (Entity, &ChemHeater, &Reagents, &InsertedBeaker),
        user: Entity,
        (reagents, dm_reagents, inventories, stored, chem): &SystemParamItem<
            '_,
            '_,
            Self::UserParam,
        >,
    ) -> ChemHeaterData {
        let beaker = inserted
            .0
            .filter(|&b| stored.get(b).is_ok_and(|s| s.0 == e))
            .and_then(|b| Some((b, &reagents.get(b).ok()?.0)));
        let (active, flashing) = match (beaker, chem.as_ref()) {
            (Some((_, h)), Some(chem)) => active_reactions(&chem.0, h),
            _ => (Vec::new(), false),
        };
        let amount = |path: &str| {
            chem.as_ref()
                .and_then(|c| c.0.id(path))
                .map_or(0.0, |id| buffer.0.amount(id))
        };
        ChemHeaterData {
            target_temp: heater.target_temperature,
            is_active: heater.on,
            upgrade_level: heater.heater_coefficient * 10.0,
            has_beaker_in_hand: crate::chem_dispenser::beaker_in_hand(
                user,
                inventories,
                dm_reagents,
                reagents,
            ),
            beaker: beaker.map(|(b, h)| HeaterBeaker {
                max_volume: dm_reagents.get(b).map_or(h.maximum_volume, |d| d.volume),
                ph: round_to(h.ph, 0.01),
                current_volume: h.total_volume,
                contents: chem
                    .as_ref()
                    .map(|c| {
                        h.reagents
                            .iter()
                            .map(|r| {
                                (
                                    c.0.def(r.id).name.clone(),
                                    round_to(r.volume, ss13_reagents::CHEMICAL_VOLUME_ROUNDING),
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            }),
            current_temp: beaker.map_or(0.0, |(_, h)| h.chem_temp),
            active_reactions: active,
            is_flashing: flashing,
            acidic_buffer_vol: amount(ACIDIC_BUFFER),
            basic_buffer_vol: amount(BASIC_BUFFER),
            dispense_volume: heater.dispense_volume,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<HeaterAct, UiReject> {
        let number = |key: &str| params.number(key).map(|n| n as f32);
        Ok(match action {
            "power" => HeaterAct::Power,
            "temperature" => HeaterAct::Temperature(number("target")?),
            "eject" => HeaterAct::Eject,
            "insert" => HeaterAct::Insert,
            "acidBuffer" => HeaterAct::AcidBuffer(number("target")?),
            "basicBuffer" => HeaterAct::BasicBuffer(number("target")?),
            "disp_vol" => HeaterAct::DispenseVolume(number("target")?),
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

fn heater_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<HeaterWindow>>>) {
    let pending: Vec<(Entity, Entity, HeaterAct)> = acts
        .read(world.resource::<Messages<UiActed<HeaterWindow>>>())
        .map(|a| (a.actor, a.source, a.act.clone()))
        .collect();
    for (actor, source, act) in pending {
        apply_act(world, actor, source, act);
    }
}

/// tg `ui_act`, every check at the moment of effect.
pub fn apply_act(world: &mut World, actor: Entity, e: Entity, act: HeaterAct) {
    if world.get::<ChemHeater>(e).is_none() {
        return;
    }
    match act {
        HeaterAct::Power => {
            if let Some(mut h) = world.get_mut::<ChemHeater>(e) {
                h.on = !h.on;
            }
        }
        HeaterAct::Temperature(target) => {
            if target.is_finite()
                && let Some(mut h) = world.get_mut::<ChemHeater>(e)
            {
                h.target_temperature = target.clamp(0.0, MAX_TARGET);
            }
        }
        // Eject does not turn it off, so the next beaker can be preheated.
        HeaterAct::Eject => {
            replace_beaker(world, actor, e, None);
            update_icon(world, e);
        }
        HeaterAct::Insert => {
            crate::chem_dispenser::insert_held(world, actor, e);
            update_icon(world, e);
        }
        HeaterAct::AcidBuffer(target) => move_buffer(world, e, ACIDIC_BUFFER, target),
        HeaterAct::BasicBuffer(target) => move_buffer(world, e, BASIC_BUFFER, target),
        // tg takes any number; the window's input is 1-10.
        HeaterAct::DispenseVolume(target) => {
            if target.is_finite()
                && let Some(mut h) = world.get_mut::<ChemHeater>(e)
            {
                h.dispense_volume = target.clamp(1.0, 10.0);
            }
        }
    }
}

/// tg `move_buffer`: `volume` < 0 draws the beaker's buffer up to half the holder, else
/// `dispense_volume` of it goes into the beaker.
fn move_buffer(world: &mut World, e: Entity, buffer: &str, volume: f32) {
    if !volume.is_finite() || volume == 0.0 {
        return;
    }
    let Some(beaker) = beaker_of(world, e) else {
        crate::cargo::console::machine_say(world, e, "No beaker found!");
        return;
    };
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let Some(id) = chem.id(buffer) else {
        return;
    };
    let Some(dispense) = world.get::<ChemHeater>(e).map(|h| h.dispense_volume) else {
        return;
    };
    let (Some(mut own), Some(mut theirs)) = (
        world.get::<Reagents>(e).map(|r| r.0.clone()),
        world.get::<Reagents>(beaker).map(|r| r.0.clone()),
    ) else {
        return;
    };
    if volume < 0.0 {
        if theirs.has(id).is_none() {
            let name = chem.def(id).name.clone();
            crate::cargo::console::machine_say(
                world,
                e,
                &format!(
                    "Unable to find {name} in beaker to draw from! Please insert a beaker containing {name}."
                ),
            );
            return;
        }
        let want = own.maximum_volume / 2.0 - own.amount(id);
        theirs.trans_to(&chem, &mut own, want, 1.0, Some(id), false, false);
    } else {
        own.trans_to(&chem, &mut theirs, dispense, 1.0, Some(id), false, false);
    }
    world.entity_mut(e).insert(Reagents(own));
    world.entity_mut(beaker).insert(Reagents(theirs));
}

/// `examine`.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(Span, String)>) {
    let Some(h) = world.get::<ChemHeater>(target) else {
        return;
    };
    let speed = crate::chem::dm_num(h.heater_coefficient * 1000.0);
    lines.push((
        Span::Notice,
        format!("Дисплей состояния: нагрев реагентов на скорости {speed}%."),
    ));
    match beaker_of(world, target).and_then(|b| world.get::<Reagents>(b)) {
        Some(r) => {
            lines.push((
                Span::Notice,
                format!(
                    "Внутри ёмкость с {} ед. реагентов.",
                    crate::chem::dm_num(r.0.total_volume)
                ),
            ));
            if r.0.is_reacting {
                lines.push((Span::Notice, "Содержимое сейчас реагирует.".to_owned()));
            }
        }
        None => lines.push((Span::Warning, "Ёмкость не вставлена.".to_owned())),
    }
    let on = if h.on {
        "включён"
    } else {
        "выключен"
    };
    lines.push((Span::Notice, format!("Нагрев {on}.")));
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_reagents::{ChemData, ReagentDef};

    fn water() -> ReagentDef {
        ReagentDef {
            path: "/datum/reagent/water".into(),
            name: "Water".into(),
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
    fn heats_towards_the_target_by_the_coefficient() {
        let chem = Chemistry::new(ChemData {
            reagents: vec![water()],
            ..Default::default()
        });
        let mut h = Holder::new(50.0, flags::OPENCONTAINER, Owner::default());
        h.add_reagent(&chem, 0, 50.0, 300.0, None, None, true, false);
        let mut heater = ChemHeater {
            on: false,
            target_temperature: 400.0,
            heater_coefficient: 0.1,
            dispense_volume: 1.0,
        };
        assert_eq!(heat(&chem, &heater, &mut h, 2.0), None, "off");
        heater.on = true;
        // delta T = (400 - 300) * 0.1 * 2 s = 20 K, whatever the volume.
        let energy = heat(&chem, &heater, &mut h, 2.0).unwrap();
        assert_eq!(energy, 100.0 * 0.1 * 2.0 * 200.0 * 50.0);
        assert!((h.chem_temp - 320.0).abs() < 1e-3, "{}", h.chem_temp);
        // Cooling works the same way.
        heater.target_temperature = 0.0;
        heat(&chem, &heater, &mut h, 1.0);
        assert!((h.chem_temp - 288.0).abs() < 1e-3, "{}", h.chem_temp);
    }
}
