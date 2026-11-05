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
// shortcut: no custom recipes, reaction lookup, hallucinated names, beaker overlay or
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

/// The container a chem machine holds (tg `beaker` of dispensers, the ChemMaster and the
/// heater); [`beaker_of`] reads it.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct InsertedBeaker(pub Option<Entity>);

/// A dispenser's live state.
#[derive(Component, Clone, Debug, Default, PartialEq)]
#[require(InsertedBeaker)]
pub struct ChemDispenser {
    pub amount: f32,
    pub emagged: bool,
    /// The cell part (`/obj/item/stock_parts/power_store`), [`ItemCell`].
    pub cell: Option<Entity>,
    /// What it dispenses now, in the window's order.
    pub dispensable: Vec<ReagentId>,
    /// Joules per unit.
    pub power_cost: f32,
    /// Watts.
    pub recharge_amount: f32,
    /// `saved_recipes`: (name, (reagent name, units)), in saving order.
    pub saved_recipes: Vec<(String, Vec<(String, f32)>)>,
    /// `recording_recipe`: what dispensing adds up while recording.
    pub recording: Option<Vec<(String, f32)>>,
    /// `show_ph`: off for the bar's dispensers.
    pub show_ph: bool,
}

/// What a prompt opened by the window was for.
#[derive(Component, Clone, Debug)]
enum DispenserPrompt {
    /// `clear_recipes`: "Clear all recipes?".
    Clear(Entity),
    /// `save_recording`: the recipe's name.
    Name(Entity),
    /// `save_recording`: overwrite the recipe of this name?
    Overwrite(Entity, String),
}

/// `MAX_NAME_LEN`.
const MAX_NAME_LEN: u32 = 42;
const DRINKS: &str = "/obj/machinery/chem_dispenser/drinks";

pub struct ChemDispenserPlugin;

impl Plugin for ChemDispenserPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<DispenserWindow>()
            .add_systems(
                Startup,
                setup_mapped
                    .after(crate::power::init_power)
                    .after(crate::mecha::core::MechaLoaded),
            )
            .add_systems(
                Update,
                (
                    dispenser_acts.in_set(UiSystems::Handle),
                    prompt_answers
                        .in_set(UiSystems::Handle)
                        .after(ss13_tgui::prompt::PromptSystems::Answer),
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
    let show_ph = world
        .get::<ss13_mapload::DmObj>(e)
        .is_none_or(|o| !crate::obj_damage::is_under(&o.path, DRINKS));
    world.entity_mut(e).insert(ChemDispenser {
        amount: dm.amount,
        emagged: dm.emagged_at_start,
        show_ph,
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
    let power_cost =
        (dm.power_cost - r.sum(PartKind::MatterBin) as f32 * BIN_POWER_CUT).max(MIN_POWER_COST);
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
pub(crate) fn chem_container(world: &World, item: Entity) -> bool {
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
    if world.get::<ChemDispenser>(target).is_none() {
        return false;
    }
    if !chem_container(world, held) {
        return false;
    }
    replace_beaker(world, mob, target, Some(held));
    open_window(world, client, mob, target);
    true
}

/// tg `emag_act` (from `crate::emag`): the emagged reagents join the dispensable ones, once.
pub fn emag_act(world: &mut World, client: Entity, e: Entity) {
    let Some(d) = world.get::<ChemDispenser>(e) else {
        return;
    };
    if d.emagged {
        crate::tools::tell(world, client, Span::Warning, "уже взломан!");
        return;
    }
    if let Some(mut d) = world.get_mut::<ChemDispenser>(e) {
        d.emagged = true;
    }
    refresh_parts(world, e);
    crate::tools::tell(world, client, Span::Notice, "предохранители закорочены");
}

/// `attack_hand` (the window) and `attack_hand_secondary` (the beaker out).
pub fn interact(
    world: &mut World,
    client: Entity,
    mob: Entity,
    target: Entity,
    right: bool,
) -> bool {
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

/// tg `replace_beaker` of the chem machines: the old beaker to the user's hands, `new` (from
/// them) in.
pub(crate) fn replace_beaker(world: &mut World, mob: Entity, e: Entity, new: Option<Entity>) {
    let old = beaker_of(world, e);
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
    if let Some(mut b) = world.get_mut::<InsertedBeaker>(e) {
        b.0 = new;
    }
}

/// The beaker, if it is still inside (tg `Exited` clears it).
pub(crate) fn beaker_of(world: &World, e: Entity) -> Option<Entity> {
    let beaker = world.get::<InsertedBeaker>(e)?.0?;
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
            let active = world
                .get::<DmPower>(e)
                .map_or(0.0, |p| p.active_power_usage);
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
    DispenseRecipe(String),
    ClearRecipes,
    RecordRecipe,
    SaveRecording,
    CancelRecording,
}

impl ServerUi for DispenserWindow {
    type Source = (
        Entity,
        &'static ChemDispenser,
        &'static DmChemDispenser,
        &'static InsertedBeaker,
    );
    type UserParam = (
        Query<'static, 'static, &'static Reagents>,
        Query<'static, 'static, &'static DmReagents>,
        Query<'static, 'static, &'static ItemCell>,
        Query<'static, 'static, &'static Inventory>,
        Query<'static, 'static, &'static crate::storage::StoredIn>,
        Option<Res<'static, Chem>>,
    );
    type Act = DispenserAct;

    fn static_data(_: (Entity, &ChemDispenser, &DmChemDispenser, &InsertedBeaker)) {}

    fn data(
        (e, d, _, inserted): (Entity, &ChemDispenser, &DmChemDispenser, &InsertedBeaker),
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
        let has_beaker_in_hand = beaker_in_hand(user, inventories, dm_reagents, reagents);
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
                            ph_col: ss13_tgui::interfaces::chem_dispenser::ph_color(def.ph).into(),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        let beaker = inserted
            .0
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
            show_ph: d.show_ph && d.recording.is_none(),
            has_beaker_in_hand,
            chemicals,
            beaker,
            recipes: d.saved_recipes.iter().map(|(n, _)| n.clone()).collect(),
            recording_recipe: d.recording.clone(),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<DispenserAct, UiReject> {
        Ok(match action {
            "amount" => DispenserAct::Amount(params.number("target")? as f32),
            "dispense" => DispenserAct::Dispense(params.text("reagent")?),
            "remove" => DispenserAct::Remove(params.number("amount")? as f32),
            "eject" => DispenserAct::Eject,
            "insert" => DispenserAct::Insert,
            "dispense_recipe" => DispenserAct::DispenseRecipe(params.text("recipe")?),
            "clear_recipes" => DispenserAct::ClearRecipes,
            "record_recipe" => DispenserAct::RecordRecipe,
            "save_recording" => DispenserAct::SaveRecording,
            "cancel_recording" => DispenserAct::CancelRecording,
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

/// `hasBeakerInHand`: the user's active hand holds an open reagent container.
pub(crate) fn beaker_in_hand(
    user: Entity,
    inventories: &Query<&Inventory>,
    dm_reagents: &Query<&DmReagents>,
    reagents: &Query<&Reagents>,
) -> bool {
    let held = inventories.get(user).ok().and_then(Inventory::active_item);
    held.is_some_and(|h| {
        dm_reagents.contains(h) && reagents.get(h).is_ok_and(|r| r.0.is_open_container())
    })
}

/// tg `ui_act` `insert`: the held open container in (the old one out).
pub(crate) fn insert_held(world: &mut World, actor: Entity, e: Entity) {
    let held = world
        .get::<Inventory>(actor)
        .and_then(Inventory::active_item);
    if let Some(held) = held.filter(|&h| chem_container(world, h)) {
        replace_beaker(world, actor, e, Some(held));
    }
}

fn transfer_amounts(world: &World, beaker: Entity) -> Vec<f32> {
    world
        .get::<DmReagents>(beaker)
        .map(|dm| dm.possible_transfer_amounts.clone())
        .unwrap_or_default()
}

fn dispenser_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<DispenserWindow>>>) {
    let pending: Vec<(Entity, Entity, Entity, DispenserAct)> = acts
        .read(world.resource::<Messages<UiActed<DispenserWindow>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, act) in pending {
        if let Some(asked) = prompt_for(&act, source) {
            if operational(world, source) {
                ask(world, client, actor, asked);
            }
            continue;
        }
        apply_act(world, actor, source, act);
    }
}

/// The acts that ask first (`tgui_alert`, `tgui_input_text`).
fn prompt_for(act: &DispenserAct, e: Entity) -> Option<DispenserPrompt> {
    match act {
        DispenserAct::ClearRecipes => Some(DispenserPrompt::Clear(e)),
        DispenserAct::SaveRecording => Some(DispenserPrompt::Name(e)),
        _ => None,
    }
}

fn ask(world: &mut World, client: Entity, actor: Entity, asked: DispenserPrompt) {
    use ss13_tgui::prompt::{Ask, PromptKind};
    let yes_no = || PromptKind::Alert {
        buttons: vec!["Yes".into(), "No".into()],
        autofocus: true,
    };
    let question = match &asked {
        DispenserPrompt::Clear(_) => {
            Ask::new(client, actor, "Clear?", "Clear all recipes?", yes_no())
        }
        DispenserPrompt::Name(_) => Ask::new(
            client,
            actor,
            "Recipe Name",
            "What do you want to name this recipe?",
            PromptKind::Text {
                default: String::new(),
                max_length: MAX_NAME_LEN,
                multiline: false,
            },
        ),
        DispenserPrompt::Overwrite(_, name) => Ask::new(
            client,
            actor,
            "",
            format!("\"{name}\" already exists, do you want to overwrite it?"),
            yes_no(),
        ),
    };
    let prompt = ss13_tgui::prompt::ask_world(world, question);
    world.entity_mut(prompt).insert(asked);
}

/// The answers to the recipe prompts, checked again now: power, the user next to it.
fn prompt_answers(
    world: &mut World,
    mut cursor: Local<MessageCursor<ss13_tgui::prompt::PromptAnswered>>,
) {
    use ss13_tgui::prompt::{PromptAnswer, PromptAnswered};
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in answers {
        let Some(asked) = world.get::<DispenserPrompt>(prompt).cloned() else {
            continue;
        };
        let e = match &asked {
            DispenserPrompt::Clear(e)
            | DispenserPrompt::Name(e)
            | DispenserPrompt::Overwrite(e, _) => *e,
        };
        if world.get::<ChemDispenser>(e).is_none()
            || !operational(world, e)
            || !crate::storage::in_reach(world, actor, e)
        {
            continue;
        }
        match (asked, answer) {
            (DispenserPrompt::Clear(_), Some(PromptAnswer::Button(0))) => {
                if let Some(mut d) = world.get_mut::<ChemDispenser>(e) {
                    d.saved_recipes.clear();
                }
            }
            (DispenserPrompt::Name(_), Some(PromptAnswer::Text(name))) => {
                let name = name.trim().to_owned();
                let exists = world
                    .get::<ChemDispenser>(e)
                    .is_some_and(|d| d.saved_recipes.iter().any(|(n, _)| *n == name));
                if exists {
                    ask(world, client, actor, DispenserPrompt::Overwrite(e, name));
                } else {
                    save_recording(world, client, e, name);
                }
            }
            // `tgui_alert(...) == "No"` returns; closing it (null) saves, as in tg.
            (DispenserPrompt::Overwrite(_, name), answer)
                if answer != Some(PromptAnswer::Button(1)) =>
            {
                save_recording(world, client, e, name);
            }
            _ => {}
        }
    }
}

/// The end of `save_recording`: every recorded reagent must be dispensable here.
fn save_recording(world: &mut World, client: Entity, e: Entity, name: String) {
    let Some(d) = world.get::<ChemDispenser>(e).cloned() else {
        return;
    };
    let Some(recording) = d.recording.clone() else {
        return;
    };
    if name.is_empty() {
        return;
    }
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    for (reagent, _) in &recording {
        if !d
            .dispensable
            .iter()
            .any(|&id| chem.def(id).name == *reagent)
        {
            crate::tools::tell(
                world,
                client,
                Span::Warning,
                format!(
                    "{} не может найти {reagent}!",
                    crate::chem::name_of(world, e)
                ),
            );
            if let Some(&pos) = world.get::<TilePos>(e) {
                crate::sound::playsound(
                    world,
                    crate::sound::PlaySound::new(
                        pos,
                        "sound/machines/buzz/buzz-two.ogg",
                        50.0,
                        true,
                    ),
                );
            }
            return;
        }
    }
    if let Some(mut d) = world.get_mut::<ChemDispenser>(e) {
        match d.saved_recipes.iter_mut().find(|(n, _)| *n == name) {
            Some(slot) => slot.1 = recording,
            None => d.saved_recipes.push((name, recording)),
        }
        d.recording = None;
    }
}

/// `recording_recipe[name] += amount` (DM assoc order: first addition).
fn record(recording: &mut Vec<(String, f32)>, name: &str, amount: f32) {
    match recording.iter_mut().find(|(n, _)| n == name) {
        Some(slot) => slot.1 += amount,
        None => recording.push((name.to_owned(), amount)),
    }
}

/// Why [`dispense`] stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DispenseFail {
    Full,
    NoEnergy,
    Other,
}

/// One reagent into the beaker: `min(amount, free space)` units, paid from the cell.
fn dispense(world: &mut World, e: Entity, id: ReagentId, amount: f32) -> Result<(), DispenseFail> {
    let (Some(d), Some(beaker), Some(chem)) = (
        world.get::<ChemDispenser>(e).cloned(),
        beaker_of(world, e),
        world.get_resource::<Chem>().map(|c| c.0.clone()),
    ) else {
        return Err(DispenseFail::Other);
    };
    let (Some(cell), Some(dm)) = (d.cell, world.get::<DmChemDispenser>(e).cloned()) else {
        return Err(DispenseFail::Other);
    };
    let Some(mut holder) = world.get::<Reagents>(beaker).map(|r| r.0.clone()) else {
        return Err(DispenseFail::Other);
    };
    let to_dispense = amount
        .min(holder.maximum_volume - holder.total_volume)
        .max(0.0);
    if to_dispense <= 0.0 {
        return Err(DispenseFail::Full);
    }
    let Some(mut state) = world.get::<ItemCell>(cell).map(|c| c.0) else {
        return Err(DispenseFail::Other);
    };
    if state.take(f64::from(to_dispense * d.power_cost), false) <= 0.0 {
        return Err(DispenseFail::NoEnergy);
    }
    world.entity_mut(cell).insert(ItemCell(state));
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
    Ok(())
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
            if !operational(world, e) || d.cell.is_none() {
                return;
            }
            let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
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
            if d.recording.is_some() {
                if let Some(mut live) = world.get_mut::<ChemDispenser>(e)
                    && let Some(r) = live.recording.as_mut()
                {
                    record(r, &name, d.amount);
                }
                return;
            }
            if beaker.is_none() {
                return;
            }
            match dispense(world, e, id, d.amount) {
                Err(DispenseFail::Full) => {
                    crate::cargo::console::machine_say(world, e, "The container is full!");
                }
                Err(DispenseFail::NoEnergy) => crate::cargo::console::machine_say(
                    world,
                    e,
                    "Not enough energy to complete operation!",
                ),
                _ => {}
            }
        }
        DispenserAct::DispenseRecipe(recipe) => {
            if !operational(world, e) || d.cell.is_none() {
                return;
            }
            let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
                return;
            };
            let Some((_, list)) = d.saved_recipes.iter().find(|(n, _)| *n == recipe).cloned()
            else {
                return;
            };
            for (name, amount) in list {
                let Some(id) = d
                    .dispensable
                    .iter()
                    .copied()
                    .find(|&id| chem.def(id).name == name)
                else {
                    return;
                };
                if d.recording.is_some() {
                    if let Some(mut live) = world.get_mut::<ChemDispenser>(e)
                        && let Some(r) = live.recording.as_mut()
                    {
                        record(r, &name, amount);
                    }
                    continue;
                }
                if beaker.is_none() {
                    return;
                }
                if dispense(world, e, id, amount) == Err(DispenseFail::NoEnergy) {
                    crate::cargo::console::machine_say(
                        world,
                        e,
                        "Not enough energy to complete operation!",
                    );
                    return;
                }
            }
        }
        DispenserAct::RecordRecipe => {
            if operational(world, e)
                && let Some(mut d) = world.get_mut::<ChemDispenser>(e)
            {
                d.recording = Some(Vec::new());
            }
        }
        DispenserAct::CancelRecording => {
            if operational(world, e)
                && let Some(mut d) = world.get_mut::<ChemDispenser>(e)
            {
                d.recording = None;
            }
        }
        // Asked first (`dispenser_acts`).
        DispenserAct::ClearRecipes | DispenserAct::SaveRecording => {}
        DispenserAct::Remove(amount) => {
            if operational(world, e)
                && d.recording.is_none()
                && let Some(b) = beaker
                && transfer_amounts(world, b).contains(&amount)
                && let Some(mut r) = world.get_mut::<Reagents>(b)
            {
                r.0.remove_all(amount);
            }
        }
        DispenserAct::Eject => replace_beaker(world, actor, e, None),
        DispenserAct::Insert => insert_held(world, actor, e),
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
            default_container: String::new(),
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
        assert_eq!(
            names(dispensable(&chem, &dm, false, false)),
            ["Cola", "Water"]
        );
        assert_eq!(
            names(dispensable(&chem, &dm, true, true)),
            ["Cola", "Water", "Ice", "Toxin"]
        );
    }
}
