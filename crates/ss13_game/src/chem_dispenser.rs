//! Chem dispensers (server): tg `/obj/machinery/chem_dispenser` (`reagents/chemistry/machinery/
//! chem_dispenser.dm`) and its `ChemDispenser` window; the bar's soda and booze dispensers are
//! its `drinks` subtypes.
//!
//! - A dispenser dispenses its [`DmChemDispenser`] reagents (sorted by name; a servo above tier 3
//!   adds the upgrade ones, an emag the emagged ones) into the inserted beaker, `amount` units at
//!   a time, paying `power_cost` joules per unit from its cell (`RefreshParts`: the cell part,
//!   `power_cost = max(100 - 250 * matter bin tiers, 100)`, `recharge_amount = 300 W *` each
//!   capacitor's tier). Powered, every machine tick a cell that is not full is charged
//!   `recharge_amount * 2 s` from the area after using `active_power_usage * 2 s`.
//! - A held open container clicked on it goes in (the old beaker to the user's hands) and opens
//!   the window; an empty-hand click opens the window, a right click takes the beaker out. The
//!   window's acts (`amount`, `dispense`, `remove`, `eject`, `insert`) are checked on the server
//!   at the moment of effect: power, the beaker, the amount among the beaker's transfer amounts,
//!   the reagent among the dispensable ones.
// shortcut: no custom recipes, reaction lookup, EMP spray, hallucinated names, beaker overlay or
// working animation; an emag only exists as the `fullupgrade` subtypes' starting state.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::{DmChemDispenser, DmPower, DmReagents};
use ss13_reagents::{ReagentId, round_to};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::chem_dispenser::{
    ChemDispenserData, ChemDispenserUi, DispensableReagent, DispenserBeaker,
};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use crate::chem::{Chem, Reagents};
use crate::inventory::{CarriedBy, Inventory};
use crate::mecha::core::ItemCell;
use crate::power::{MACHINE_TICK_SECS, MachineTicks, Unpowered};
use crate::stock_parts::{ComponentParts, PartKind};
use crate::storage::{put_inside, take_out};

pub const DISPENSER_ROOT: &str = ss13_mapload::dispenser::DISPENSER_ROOT;
/// `power_cost` floor and the per-matter-bin-tier cut (`0.1 KILO WATTS`, `0.25 KILO WATTS`).
const MIN_POWER_COST: f32 = 100.0;
const BIN_POWER_CUT: f32 = 250.0;
/// `CHEMICAL_VOLUME_ROUNDING`.
const VOLUME_ROUNDING: f32 = 0.01;

/// A dispenser's live state.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ChemDispenser {
    pub amount: f32,
    pub beaker: Option<Entity>,
    pub emagged: bool,
    /// The cell part (`/obj/item/stock_parts/power_store`), [`ItemCell`].
    pub cell: Option<Entity>,
    /// What it dispenses now, in the window's order.
    pub dispensable: Vec<ReagentId>,
    /// Joules per unit.
    pub power_cost: f32,
    /// Watts.
    pub recharge_amount: f32,
}

pub struct ChemDispenserPlugin;

impl Plugin for ChemDispenserPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<DispenserWindow>()
            .add_systems(Startup, setup_mapped.after(crate::power::init_power))
            .add_systems(
                Update,
                (
                    dispenser_acts.in_set(UiSystems::Handle),
                    process_dispensers.after(crate::power::PowerSystems),
                ),
            );
    }
}

fn setup_mapped(world: &mut World) {
    let mut query = world.query_filtered::<Entity, With<DmChemDispenser>>();
    let found: Vec<Entity> = query.iter(world).collect();
    for e in found {
        setup(world, e);
    }
}

/// A mapped dispenser, or one built from a frame: its parts as items (the cell holds the
/// charge), its state and `RefreshParts`.
pub fn setup(world: &mut World, e: Entity) {
    let Some(dm) = world.get::<DmChemDispenser>(e).cloned() else {
        return;
    };
    crate::stock_parts::ensure_parts(world, e);
    world.entity_mut(e).insert(ChemDispenser {
        amount: dm.amount,
        emagged: dm.emagged_at_start,
        ..default()
    });
    refresh_parts(world, e);
}

/// tg `chem_dispenser/RefreshParts`, and the dispensable list it settles.
pub fn refresh_parts(world: &mut World, e: Entity) {
    let (Some(dm), Some(state)) = (
        world.get::<DmChemDispenser>(e).cloned(),
        world.get::<ChemDispenser>(e).cloned(),
    ) else {
        return;
    };
    let r = crate::stock_parts::ratings(world, e);
    let cell = world.get::<ComponentParts>(e).and_then(|p| {
        p.parts()
            .iter()
            .copied()
            .filter(|&p| world.get::<ItemCell>(p).is_some())
            .last()
    });
    let power_cost = (dm.power_cost - r.sum(PartKind::MatterBin) as f32 * BIN_POWER_CUT)
        .max(MIN_POWER_COST);
    let recharge_amount = r
        .tiers(PartKind::Capacitor)
        .fold(dm.recharge_amount, |a, t| a * t as f32);
    let upgraded = r.last(PartKind::Servo).is_some_and(|t| t > 3);
    let dispensable = world
        .get_resource::<Chem>()
        .map(|chem| dispensable(&chem.0, &dm, upgraded, state.emagged))
        .unwrap_or_default();
    world.entity_mut(e).insert(ChemDispenser {
        cell,
        power_cost,
        recharge_amount,
        dispensable,
        ..state
    });
}

/// `Initialize`'s `sort_list(dispensable_reagents, cmp_reagents_asc)`, then the upgrade
/// reagents (`RefreshParts`) and the emagged ones (`emag_act`) added at the end (`|=`).
pub fn dispensable(
    chem: &ss13_reagents::Chemistry,
    dm: &DmChemDispenser,
    upgraded: bool,
    emagged: bool,
) -> Vec<ReagentId> {
    let sorted = |list: &[String]| {
        let mut ids: Vec<ReagentId> = list.iter().filter_map(|p| chem.id(p)).collect();
        ids.sort_by(|a, b| chem.def(*a).name.cmp(&chem.def(*b).name));
        ids
    };
    let mut out = sorted(&dm.dispensable);
    let mut extra = Vec::new();
    if upgraded {
        extra.extend(sorted(&dm.upgrade));
    }
    if emagged {
        extra.extend(sorted(&dm.emagged));
    }
    for id in extra {
        if !out.contains(&id) {
            out.push(id);
        }
    }
    out
}

fn operational(world: &World, e: Entity) -> bool {
    world.get::<Unpowered>(e).is_none()
}

/// A held open reagent container (`is_chem_container`).
fn chem_container(world: &World, item: Entity) -> bool {
    world.get::<DmReagents>(item).is_some()
        && world
            .get::<Reagents>(item)
            .is_some_and(|r| r.0.is_open_container())
}

/// `item_interaction`: a container goes in and the window opens. True when the click was
/// the dispenser's.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<ChemDispenser>(target).is_none() || !chem_container(world, held) {
        return false;
    }
    replace_beaker(world, mob, target, Some(held));
    open_window(world, client, mob, target);
    true
}

/// `attack_hand` (the window) and `attack_hand_secondary` (the beaker out).
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity, right: bool) -> bool {
    if world.get::<ChemDispenser>(target).is_none() {
        return false;
    }
    if right {
        replace_beaker(world, mob, target, None);
    } else {
        open_window(world, client, mob, target);
    }
    true
}

fn open_window(world: &mut World, client: Entity, mob: Entity, target: Entity) {
    let title = world
        .get::<ss13_mapload::DmName>(target)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default();
    world.write_message(OpenUi::new::<DispenserWindow>(client, mob, target, title));
}

/// tg `replace_beaker`: the old beaker to the user's hands, `new` (from them) in.
fn replace_beaker(world: &mut World, mob: Entity, e: Entity, new: Option<Entity>) {
    let old = world.get::<ChemDispenser>(e).and_then(|d| d.beaker);
    if let Some(new) = new {
        if let Some(mut inventory) = world.get_mut::<Inventory>(mob)
            && let Some(slot) = inventory.hands.iter_mut().find(|h| **h == Some(new))
        {
            *slot = None;
        }
        world.entity_mut(new).remove::<CarriedBy>();
        put_inside(world, new, e);
    }
    if let Some(old) = old {
        if let Some(&pos) = world.get::<TilePos>(e) {
            take_out(world, old, pos);
        }
        if let Some(inventory) = world.get::<Inventory>(mob).cloned()
            && let Some(hand) = [inventory.active, 1 - inventory.active]
                .into_iter()
                .find(|&h| inventory.hands[h].is_none())
        {
            crate::inventory::pick_up(world, mob, old, hand);
        }
    }
    if let Some(mut d) = world.get_mut::<ChemDispenser>(e) {
        d.beaker = new;
    }
}

/// The beaker, if it is still inside.
fn beaker_of(world: &World, e: Entity) -> Option<Entity> {
    let beaker = world.get::<ChemDispenser>(e)?.beaker?;
    world
        .get::<crate::storage::StoredIn>(beaker)
        .filter(|s| s.0 == e)
        .map(|_| beaker)
}

/// `process`: a cell that is not full charges.
fn process_dispensers(world: &mut World, mut last: Local<Option<u64>>) {
    let ticks = world.resource::<MachineTicks>().0;
    let from = last.unwrap_or(ticks);
    *last = Some(ticks);
    for _ in from..ticks {
        let mut q = world.query::<(Entity, &ChemDispenser)>();
        let found: Vec<(Entity, Option<Entity>, f32)> = q
            .iter(world)
            .map(|(e, d)| (e, d.cell, d.recharge_amount))
            .collect();
        for (e, cell, recharge) in found {
            if !operational(world, e) {
                continue;
            }
            let Some(cell) = cell else {
                continue;
            };
            let Some(mut state) = world.get::<ItemCell>(cell).map(|c| c.0) else {
                continue;
            };
            if state.charge >= state.maxcharge {
                continue;
            }
            let active = world.get::<DmPower>(e).map_or(0.0, |p| p.active_power_usage);
            crate::kitchen::machine_use_energy(world, e, active * MACHINE_TICK_SECS);
            crate::power::charge_cell(
                world,
                e,
                f64::from(recharge) * MACHINE_TICK_SECS,
                &mut state,
                false,
            );
            world.entity_mut(cell).insert(ItemCell(state));
        }
    }
}

pub struct DispenserWindow;

impl UiInterface for DispenserWindow {
    const NAME: &'static str = ChemDispenserUi::NAME;
    type Data = ChemDispenserData;
    type StaticData = ();
}

#[derive(Clone, Debug, PartialEq)]
pub enum DispenserAct {
    Amount(f32),
    Dispense(String),
    Remove(f32),
    Eject,
    Insert,
}

impl ServerUi for DispenserWindow {
    type Source = (Entity, &'static ChemDispenser, &'static DmChemDispenser);
    type UserParam = (
        Query<'static, 'static, &'static Reagents>,
        Query<'static, 'static, &'static DmReagents>,
        Query<'static, 'static, &'static ItemCell>,
        Query<'static, 'static, &'static Inventory>,
        Query<'static, 'static, &'static crate::storage::StoredIn>,
        Option<Res<'static, Chem>>,
    );
    type Act = DispenserAct;

    fn static_data(_: (Entity, &ChemDispenser, &DmChemDispenser)) {}

    fn data(
        (e, d, _): (Entity, &ChemDispenser, &DmChemDispenser),
        user: Entity,
        (reagents, dm_reagents, cells, inventories, stored, chem): &SystemParamItem<
            '_,
            '_,
            Self::UserParam,
        >,
    ) -> ChemDispenserData {
        let cell = d.cell.and_then(|c| cells.get(c).ok()).map(|c| c.0);
        let (charge, max) = cell.map_or((0.0, 0.0), |c| (c.charge as f32, c.maxcharge as f32));
        let cost = d.power_cost.max(1.0);
        let held = inventories.get(user).ok().and_then(Inventory::active_item);
        let has_beaker_in_hand = held.is_some_and(|h| {
            dm_reagents.contains(h) && reagents.get(h).is_ok_and(|r| r.0.is_open_container())
        });
        let chemicals = chem
            .as_ref()
            .map(|chem| {
                d.dispensable
                    .iter()
                    .map(|&id| {
                        let def = chem.0.def(id);
                        DispensableReagent {
                            title: def.name.clone(),
                            id: def.name.clone(),
                            ph: def.ph,
                            color: def.color.clone(),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        let beaker = d
            .beaker
            .filter(|&b| stored.get(b).is_ok_and(|s| s.0 == e))
            .and_then(|b| {
                let holder = &reagents.get(b).ok()?.0;
                let contents = chem
                    .as_ref()
                    .map(|chem| {
                        holder
                            .reagents
                            .iter()
                            .map(|r| {
                                (
                                    chem.0.def(r.id).name.clone(),
                                    round_to(r.volume, VOLUME_ROUNDING),
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                Some(DispenserBeaker {
                    max_volume: holder.maximum_volume,
                    transfer_amounts: dm_reagents
                        .get(b)
                        .map(|dm| dm.possible_transfer_amounts.clone())
                        .unwrap_or_default(),
                    ph: round_to(holder.ph, 0.01),
                    current_volume: round_to(holder.total_volume, VOLUME_ROUNDING),
                    contents,
                })
            });
        ChemDispenserData {
            amount: d.amount,
            energy: charge,
            max_energy: max,
            displayed_units: charge / cost,
            displayed_max_units: max / cost,
            show_ph: false,
            has_beaker_in_hand,
            chemicals,
            beaker,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<DispenserAct, UiReject> {
        Ok(match action {
            "amount" => DispenserAct::Amount(params.number("target")? as f32),
            "dispense" => DispenserAct::Dispense(params.text("reagent")?),
            "remove" => DispenserAct::Remove(params.number("amount")? as f32),
            "eject" => DispenserAct::Eject,
            "insert" => DispenserAct::Insert,
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

fn transfer_amounts(world: &World, beaker: Entity) -> Vec<f32> {
    world
        .get::<DmReagents>(beaker)
        .map(|dm| dm.possible_transfer_amounts.clone())
        .unwrap_or_default()
}

fn dispenser_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<DispenserWindow>>>) {
    let pending: Vec<(Entity, Entity, DispenserAct)> = acts
        .read(world.resource::<Messages<UiActed<DispenserWindow>>>())
        .map(|a| (a.actor, a.source, a.act.clone()))
        .collect();
    for (actor, source, act) in pending {
        apply_act(world, actor, source, act);
    }
}

/// tg `ui_act`, with every check at the moment of effect.
pub fn apply_act(world: &mut World, actor: Entity, e: Entity, act: DispenserAct) {
    let Some(d) = world.get::<ChemDispenser>(e).cloned() else {
        return;
    };
    let beaker = beaker_of(world, e);
    match act {
        DispenserAct::Amount(target) => {
            if operational(world, e)
                && let Some(b) = beaker
                && transfer_amounts(world, b).contains(&target)
                && let Some(mut d) = world.get_mut::<ChemDispenser>(e)
            {
                d.amount = target;
            }
        }
        DispenserAct::Dispense(name) => {
            if !operational(world, e) {
                return;
            }
            let (Some(cell), Some(beaker), Some(chem)) = (
                d.cell,
                beaker,
                world.get_resource::<Chem>().map(|c| c.0.clone()),
            ) else {
                return;
            };
            let Some(id) = d
                .dispensable
                .iter()
                .copied()
                .find(|&id| chem.def(id).name == name)
            else {
                return;
            };
            let Some(mut holder) = world.get::<Reagents>(beaker).map(|r| r.0.clone()) else {
                return;
            };
            let to_dispense = d
                .amount
                .min(holder.maximum_volume - holder.total_volume)
                .max(0.0);
            if to_dispense <= 0.0 {
                crate::cargo::console::machine_say(world, e, "The container is full!");
                return;
            }
            let Some(mut state) = world.get::<ItemCell>(cell).map(|c| c.0) else {
                return;
            };
            if state.take(f64::from(to_dispense * d.power_cost), false) <= 0.0 {
                crate::cargo::console::machine_say(
                    world,
                    e,
                    "Not enough energy to complete operation!",
                );
                return;
            }
            world.entity_mut(cell).insert(ItemCell(state));
            let Some(dm) = world.get::<DmChemDispenser>(e).cloned() else {
                return;
            };
            holder.add_reagent(
                &chem,
                id,
                to_dispense,
                dm.dispensed_temperature,
                Some(dm.base_reagent_purity),
                None,
                false,
                false,
            );
            world.entity_mut(beaker).insert(Reagents(holder));
        }
        DispenserAct::Remove(amount) => {
            if operational(world, e)
                && let Some(b) = beaker
                && transfer_amounts(world, b).contains(&amount)
                && let Some(mut r) = world.get_mut::<Reagents>(b)
            {
                r.0.remove_all(amount);
            }
        }
        DispenserAct::Eject => replace_beaker(world, actor, e, None),
        DispenserAct::Insert => {
            let held = world.get::<Inventory>(actor).and_then(Inventory::active_item);
            if let Some(held) = held.filter(|&h| chem_container(world, h)) {
                replace_beaker(world, actor, e, Some(held));
            }
        }
    }
}

/// `examine`: the recharge rate and energy cost.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(Span, String)>) {
    let Some(d) = world.get::<ChemDispenser>(target) else {
        return;
    };
    lines.push((
        Span::Notice,
        format!(
            "Скорость перезарядки: {} Вт. Расход энергии: {} Дж/ед.",
            crate::chem::dm_num(d.recharge_amount),
            crate::chem::dm_num(d.power_cost)
        ),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_reagents::{ChemData, Chemistry, ReagentDef};

    fn reagent(path: &str, name: &str) -> ReagentDef {
        ReagentDef {
            path: path.into(),
            name: name.into(),
            description: String::new(),
            color: "#FFFFFF".into(),
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
        }
    }

    #[test]
    fn dispensable_is_sorted_by_name_then_upgrades_and_emag_appended() {
        let chem = Chemistry::new(ChemData {
            reagents: vec![
                reagent("/w", "Water"),
                reagent("/c", "Cola"),
                reagent("/i", "Ice"),
                reagent("/t", "Toxin"),
            ],
            ..default()
        });
        let dm = DmChemDispenser {
            dispensable: vec!["/w".into(), "/c".into()],
            upgrade: vec!["/i".into()],
            emagged: vec!["/t".into(), "/c".into()],
            ..default()
        };
        let names = |ids: Vec<ReagentId>| -> Vec<String> {
            ids.into_iter().map(|i| chem.def(i).name.clone()).collect()
        };
        assert_eq!(names(dispensable(&chem, &dm, false, false)), ["Cola", "Water"]);
        assert_eq!(
            names(dispensable(&chem, &dm, true, true)),
            ["Cola", "Water", "Ice", "Toxin"]
        );
    }
}
