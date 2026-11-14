//! The autolathe (server), after tg `/obj/machinery/autolathe` (`code/game/machinery/autolathe.dm`),
//! `/datum/techweb/autounlocking/autolathe` and `/datum/wires/autolathe`, with tgui's `Autolathe`.
//!
//! - Materials: its own [`MaterialStore`] of `37.5` sheets per matter bin tier; a stack clicked
//!   into it (out of combat mode, not printing) goes in as far as it fits (`user_insert`), and
//!   `eject` gives sheets back. Taking it apart drops what it holds as sheets.
//! - Designs: every `AUTOLATHE` design in the `initial` category, and the `hacked` ones while
//!   hacked. A print (`make`) checks power, the disable wire, the design (hacked ones only while
//!   hacked), the amount (1-50) and the materials at `creation_efficiency` (stacks and
//!   `fixed_cost_efficiency` designs at 1); then items come out on its tile one per
//!   `(construction_time * lathe_time_factor) ** 0.8` deciseconds, each taking its materials and
//!   `ceil(units / (50 sheets) * coefficient * active_power_usage)` joules (a stack design prints
//!   all its units at once).
//! - `RefreshParts`: the storage is `37.5 sheets * matter bin tiers`, `creation_efficiency =
//!   max(1, round(1.8 - 0.2 * servo tiers, 0.1))` (1.6 with the board's tier 1 servo).
//! - Wires (`crate::wires`, panel opened with a screwdriver): hack (pulse: toggles for 6 s; cut:
//!   on), disable (refuses to print), shock (opening its window shocks with 50% chance), zap
//!   (cutting or mending shocks the user).
//!
//! Cost: a print is one timer per printing autolathe.
// shortcut: no design disks (`imported_designs`), no item recycling (only stacks with silo
// materials go in), no material choice for `/datum/material_requirement` designs (not
// offered), no drop direction, insertion energy or material animations; the window
// is the Fabricator layout (designs with categories, the material bar) with the storage total.

use std::sync::Arc;

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::research::{MAX_STACK_SIZE, SHEET_MATERIAL_AMOUNT, build_type};
use ss13_mapload::{DmObj, DmPower, DmStack};
use ss13_proto::Prototypes;
use ss13_shared::TilePos;
use ss13_tgui::UiStatus;
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};
use ss13_world::WorldSystems;

use super::lathe::{MaterialStore, design_cost, material_infos, optimal_cost};
use super::points::{ResearchIcons, ResearchLoaded};
use super::techweb::ResearchDb;
use super::*;
use crate::cargo::console::machine_say;
use crate::click::{ClickResolved, ResolveClicks};
use crate::combat::CombatMode;
use crate::construction::PanelOpen;
use crate::inventory::Inventory;
use crate::obj_damage::is_under;
use crate::power::{PowerMachine, Unpowered};
use crate::stock_parts::PartKind;
use crate::tools::Stack;

pub const AUTOLATHE_ROOT: &str = "/obj/machinery/autolathe";
const HACKED: &str = "/obj/machinery/autolathe/hacked";
/// `RND_CATEGORY_INITIAL`, `RND_CATEGORY_HACKED`.
const INITIAL: &str = "initial";
const HACKED_CATEGORY: &str = "hacked";
/// `37.5 * SHEET_MATERIAL_AMOUNT` per matter bin tier.
const UNITS_PER_BIN: f64 = 37.5 * SHEET_MATERIAL_AMOUNT;

/// The autolathe's state (tg vars).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Autolathe {
    pub hacked: bool,
    pub disabled: bool,
    pub shocked: bool,
    pub busy: bool,
    /// `creation_efficiency`.
    pub efficiency: f64,
    window: Arc<AutolatheStatic>,
}

/// A print in progress (`do_make_item` timers).
#[derive(Component, Clone, Debug)]
pub struct AutolathePrinting {
    pub design: usize,
    pub remaining: u32,
    pub per_item_secs: f64,
    pub coefficient: f64,
    /// Joules per item.
    pub charge: f64,
    pub next_at: f64,
}

/// The shared catalogue: the window's static data and each entry's [`ResearchDb`] design.
#[derive(Resource, Clone)]
pub struct AutolatheCatalogue {
    pub window: Arc<AutolatheStatic>,
    pub design_ids: Arc<Vec<usize>>,
}

impl AutolatheCatalogue {
    /// The catalogue entry of a design path, if the autolathe prints it.
    fn entry(&self, db: &ResearchDb, path: &str) -> Option<usize> {
        let design = *db.design_index.get(path)?;
        self.design_ids.iter().position(|&d| d == design)
    }
}

pub struct AutolathePlugin;

impl Plugin for AutolathePlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<AutolatheUi>()
            .add_systems(ss13_world::MapInit, setup_autolathes.after(ResearchLoaded))
            .add_systems(
                Update,
                (
                    open_autolathe
                        .after(WorldSystems::Movement)
                        .after(ResolveClicks)
                        .before(UiSystems::Open),
                    autolathe_acts.in_set(UiSystems::Handle),
                    unpowered_windows.in_set(UiSystems::StatusHook),
                    print_tick.run_if(any_with_component::<AutolathePrinting>),
                    autolathe_looks,
                    // `print_sound` while `busy`.
                    crate::looping_sound::sync::<Autolathe>(
                        &crate::looping_sound::defs::LATHE_PRINT,
                        |a| a.busy,
                    ),
                ),
            );
    }
}

/// `/datum/techweb/autounlocking/autolathe`: the `initial` and `hacked` `AUTOLATHE` designs.
pub fn catalogue(db: &ResearchDb, icons: Option<&ResearchIcons>) -> AutolatheCatalogue {
    let mut ids = Vec::new();
    let mut window = AutolatheStatic {
        fab: FabricatorStatic {
            fab_name: "Автолат".to_owned(),
            materials: material_infos(db, icons),
            designs: Vec::new(),
            sheet_amount: SHEET_MATERIAL_AMOUNT,
        },
        ..default()
    };
    for (i, d) in db.designs.iter().enumerate() {
        let initial = d.category.iter().any(|c| c == INITIAL);
        let hacked = d.category.iter().any(|c| c == HACKED_CATEGORY);
        if d.build_type & build_type::AUTOLATHE == 0
            || !(initial || hacked)
            || d.build_path.is_none()
            || d.material_requirements
            || d.materials.is_empty()
        {
            continue;
        }
        let entry = ids.len() as u16;
        if d.stack || d.fixed_cost_efficiency {
            window.exact.push(entry);
        }
        if !initial {
            window.hacked_only.push(entry);
        }
        ids.push(i);
        window.fab.designs.push(FabDesign {
            path: d.id.clone(),
            name: d.name.clone(),
            desc: d.desc.clone(),
            cost: d
                .materials
                .iter()
                .filter_map(|(m, n)| Some((*db.material_index.get(m)? as u8, optimal_cost(*n))))
                .collect(),
            categories: d.category.clone(),
            icon: icons.and_then(|icons| icons.designs.get(i).copied().flatten()),
        });
    }
    AutolatheCatalogue {
        window: Arc::new(window),
        design_ids: Arc::new(ids),
    }
}

/// `RefreshParts`: (`materials.max_amount`, `creation_efficiency`).
pub fn part_numbers(parts: &crate::stock_parts::PartRatings) -> (u32, f64) {
    let capacity = f64::from(parts.sum(PartKind::MatterBin)) * UNITS_PER_BIN;
    let efficiency = 1.8 - 0.2 * f64::from(parts.sum(PartKind::Servo));
    // DM `round(x, 0.1)`.
    (
        capacity as u32,
        ((efficiency * 10.0).round() / 10.0).max(1.0),
    )
}

fn setup_autolathes(world: &mut World) {
    let Some(db) = world.get_resource::<ResearchDb>() else {
        return;
    };
    let cat = catalogue(db, world.get_resource::<ResearchIcons>());
    info!("autolathe: {} designs", cat.design_ids.len());
    world.insert_resource(cat);
    let mut q = world.query_filtered::<(Entity, &DmObj), With<ss13_world::Mapped>>();
    let found: Vec<Entity> = q
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, AUTOLATHE_ROOT))
        .map(|(e, _)| e)
        .collect();
    for e in found {
        setup(world, e);
    }
}

/// The live state of an autolathe (map load and `crate::machine_frame`).
pub(crate) fn setup(world: &mut World, e: Entity) {
    let Some(path) = world.get::<DmObj>(e).map(|o| o.path.clone()) else {
        return;
    };
    let materials = world
        .get_resource::<ResearchDb>()
        .map_or(0, |db| db.materials.len());
    let window = world
        .get_resource::<AutolatheCatalogue>()
        .map(|c| c.window.clone())
        .unwrap_or_default();
    world.entity_mut(e).insert((
        Autolathe {
            hacked: is_under(&path, HACKED),
            disabled: false,
            shocked: false,
            busy: false,
            efficiency: 1.6,
            window,
        },
        MaterialStore::new(materials, Some(0)),
    ));
    crate::wires::add_autolathe_wires(world, e);
    refresh_parts(world, e);
}

/// tg `autolathe/RefreshParts` (from [`crate::stock_parts::refresh`]).
pub(crate) fn refresh_parts(world: &mut World, e: Entity) {
    if world.get::<Autolathe>(e).is_none() {
        return;
    }
    let (capacity, efficiency) = part_numbers(&crate::stock_parts::ratings(world, e));
    if let Some(mut a) = world.get_mut::<Autolathe>(e) {
        a.efficiency = efficiency;
    }
    if let Some(mut store) = world.get_mut::<MaterialStore>(e) {
        store.max = Some(capacity);
    }
}

/// tg `adjust_hacked` (the wires).
pub(crate) fn set_hacked(world: &mut World, e: Entity, hacked: bool) {
    if let Some(mut a) = world.get_mut::<Autolathe>(e) {
        a.hacked = hacked;
    }
}

/// tg `ui_interact`: needs power; a shocked one shocks (50%) instead.
fn open_autolathe(world: &mut World, mut clicks: Local<MessageCursor<ClickResolved>>) {
    let pending: Vec<ClickResolved> = clicks
        .read(world.resource::<Messages<ClickResolved>>())
        .copied()
        .collect();
    for click in pending {
        let Some(state) = world.get::<Autolathe>(click.target).cloned() else {
            continue;
        };
        let empty = world
            .get::<Inventory>(click.mob)
            .is_some_and(|i| i.active_item().is_none());
        // An open panel takes the hand for the wires (`INTERACT_MACHINE_WIRES_IF_OPEN`).
        let panel = world.get::<PanelOpen>(click.target).is_some_and(|p| p.0);
        if !empty || panel || world.get::<Unpowered>(click.target).is_some() {
            continue;
        }
        if state.shocked && crate::wires::shock(world, click.target, click.mob, 50.0) {
            continue;
        }
        world.write_message(OpenUi::new::<AutolatheUi>(
            click.client,
            click.mob,
            click.target,
            "Автолат".to_owned(),
        ));
    }
}

fn unpowered_windows(
    mut windows: Query<&mut UiWindow>,
    unpowered: Query<(), (With<Autolathe>, With<Unpowered>)>,
) {
    for mut window in &mut windows {
        if window.interface == AutolatheUi::NAME && unpowered.contains(window.source) {
            window.cap_status(UiStatus::Close);
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AutolatheAct {
    Make(String, f64),
    Eject(usize, f64),
}

impl ServerUi for AutolatheUi {
    type Source = (&'static Autolathe, &'static MaterialStore);
    type UserParam = ();
    type Act = AutolatheAct;

    fn static_data((lathe, _): (&Autolathe, &MaterialStore)) -> AutolatheStatic {
        (*lathe.window).clone()
    }

    fn data((lathe, store): (&Autolathe, &MaterialStore), _: Entity, _: &()) -> AutolatheData {
        AutolatheData {
            materials: store.amounts.clone(),
            material_total: store.total() as u32,
            materials_max: store.max.unwrap_or(0),
            active: lathe.busy,
            efficiency: lathe.efficiency,
            hacked: lathe.hacked,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<AutolatheAct, UiReject> {
        match action {
            MAKE => Ok(AutolatheAct::Make(
                params
                    .get(DESIGN_PATH)
                    .map(str::to_owned)
                    .ok_or_else(|| UiReject::BadParams("design_path: missing".into()))?,
                params.number(MULTIPLIER)?,
            )),
            EJECT => {
                let index = params.number(MATERIAL_REF)?;
                if !(0.0..256.0).contains(&index) {
                    return Err(UiReject::BadParams("ref: not a material".into()));
                }
                Ok(AutolatheAct::Eject(index as usize, params.number(AMOUNT)?))
            }
            _ => Err(UiReject::UnknownAction),
        }
    }
}

fn autolathe_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<AutolatheUi>>>) {
    let pending: Vec<(Entity, Entity, AutolatheAct)> = acts
        .read(world.resource::<Messages<UiActed<AutolatheUi>>>())
        .map(|a| (a.actor, a.source, a.act.clone()))
        .collect();
    for (actor, lathe, act) in pending {
        if world.get::<Unpowered>(lathe).is_some() || !world.contains_resource::<ResearchDb>() {
            continue;
        }
        match act {
            AutolatheAct::Make(path, amount) => {
                start_build(world, lathe, &path, amount);
            }
            AutolatheAct::Eject(material, amount) => {
                // Its own store: no silo permission (`linked = false`).
                super::lathe::eject_sheets(
                    world,
                    lathe,
                    actor,
                    material,
                    amount,
                    Some(lathe),
                    false,
                );
            }
        }
    }
}

/// tg `ui_act("make")`.
pub fn start_build(world: &mut World, lathe: Entity, path: &str, amount: f64) -> bool {
    let Some(state) = world.get::<Autolathe>(lathe).cloned() else {
        return false;
    };
    if state.disabled {
        machine_say(
            world,
            lathe,
            "Unable to print, voltage mismatch in internal wiring.",
        );
        return false;
    }
    if state.busy {
        machine_say(world, lathe, "Currently printing.");
        return false;
    }
    let Some(cat) = world.get_resource::<AutolatheCatalogue>().cloned() else {
        return false;
    };
    let db = world.resource::<ResearchDb>();
    // Not tg: tg accepts a hacked design from a forged act on an unhacked autolathe.
    let Some(entry) = cat
        .entry(db, path)
        .filter(|&i| state.hacked || !cat.window.hacked_only.contains(&(i as u16)))
    else {
        return false;
    };
    let design = cat.design_ids[entry];
    let d = &db.designs[design];
    if !amount.is_finite() {
        return false;
    }
    let quantity = (amount as i64).clamp(1, i64::from(MAX_PRINT)) as u32;
    let coefficient = if d.stack || d.fixed_cost_efficiency {
        1.0
    } else {
        state.efficiency
    };
    let cost = design_cost(db, design);
    let total_units: f64 = d.materials.iter().map(|(_, n)| n).sum();
    // `(construction_time * lathe_time_factor) ** 0.8` deciseconds.
    let per_item_secs = (d.construction_time * d.lathe_time_factor).powf(0.8) / 10.0;
    let active = world
        .get::<DmPower>(lathe)
        .map_or(0.0, |p| p.active_power_usage);
    let charge =
        (total_units / (f64::from(MAX_STACK_SIZE) * SHEET_MATERIAL_AMOUNT) * coefficient * active)
            .ceil();
    let enough = world
        .get::<MaterialStore>(lathe)
        .is_some_and(|s| s.has_materials(&cost, coefficient, quantity));
    if !enough {
        machine_say(world, lathe, "Not enough materials to begin production.");
        return false;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(mut a) = world.get_mut::<Autolathe>(lathe) {
        a.busy = true;
    }
    world.entity_mut(lathe).insert(AutolathePrinting {
        design,
        remaining: quantity,
        per_item_secs,
        coefficient,
        charge,
        next_at: now + per_item_secs,
    });
    true
}

fn print_tick(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut query = world.query::<(Entity, &AutolathePrinting)>();
    let due: Vec<Entity> = query
        .iter(world)
        .filter(|(_, p)| p.next_at <= now)
        .map(|(e, _)| e)
        .collect();
    for lathe in due {
        make_item(world, lathe, now);
    }
}

/// tg `finalize_build`.
fn finalize(world: &mut World, lathe: Entity) {
    world.entity_mut(lathe).remove::<AutolathePrinting>();
    if let Some(mut a) = world.get_mut::<Autolathe>(lathe) {
        a.busy = false;
    }
}

/// tg `do_make_item`.
fn make_item(world: &mut World, lathe: Entity, now: f64) {
    let Some(mut job) = world.get::<AutolathePrinting>(lathe).cloned() else {
        return;
    };
    if job.remaining == 0 {
        finalize(world, lathe);
        return;
    }
    if world.get::<Unpowered>(lathe).is_some() {
        machine_say(
            world,
            lathe,
            "Unable to continue production, power failure.",
        );
        finalize(world, lathe);
        return;
    }
    let powered = world.get::<PowerMachine>(lathe).is_none()
        || job.charge <= 0.0
        || crate::kitchen::machine_use_energy(world, lathe, job.charge) >= job.charge;
    if !powered {
        machine_say(
            world,
            lathe,
            "Unable to continue production, power grid overload.",
        );
        finalize(world, lathe);
        return;
    }
    let db = world.resource::<ResearchDb>();
    let design = &db.designs[job.design];
    let stack = design.stack;
    let build_path = design.build_path.clone().unwrap_or_default();
    let cost = design_cost(db, job.design);
    let multiplier = if stack { job.remaining } else { 1 };
    let Some(&target) = world.get::<TilePos>(lathe) else {
        finalize(world, lathe);
        return;
    };
    let Some(mut materials) = world.get_mut::<MaterialStore>(lathe) else {
        finalize(world, lathe);
        return;
    };
    if !materials.has_materials(&cost, job.coefficient, multiplier) {
        machine_say(
            world,
            lathe,
            "Unable to continue production, missing materials.",
        );
        finalize(world, lathe);
        return;
    }
    materials.use_materials(&cost, job.coefficient, multiplier);
    if stack {
        let per_item = world
            .get_resource::<Prototypes>()
            .and_then(|p| p.get(&build_path))
            .and_then(|p| p.get::<DmStack>())
            .map_or(1, |s| s.amount.max(1));
        crate::construction::spawn_stack(world, &build_path, target, per_item * job.remaining);
        job.remaining = 0;
    } else {
        crate::construction::spawn_obj(world, &build_path, target);
        job.remaining -= 1;
    }
    if job.remaining == 0 {
        finalize(world, lathe);
        return;
    }
    job.next_at = now + job.per_item_secs;
    world.entity_mut(lathe).insert(job);
}

/// tg `base_item_interaction`: busy blocks, then a stack goes into the material container.
/// True when the click was the autolathe's.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(state) = world.get::<Autolathe>(target).cloned() else {
        return false;
    };
    if world.get::<CombatMode>(mob).is_some_and(|c| c.0) {
        return false;
    }
    if state.busy {
        crate::tools::tell(
            world,
            client,
            ss13_shared::chat::Span::Warning,
            "Автолат занят!",
        );
        return true;
    }
    let Some(path) = world
        .get::<ss13_mapload::DmItem>(held)
        .map(|i| i.path.clone())
    else {
        return false;
    };
    if world.get::<Stack>(held).is_none() || world.get::<Unpowered>(target).is_some() {
        return false;
    }
    super::lathe::insert_stack(world, client, target, Some(target), held, &path);
    true
}

/// `material_container.retrieve_all()` on deconstruction: what it holds as sheets.
pub(crate) fn drop_sheets(world: &mut World, machine: Entity, pos: TilePos) {
    let Some(store) = world.get::<MaterialStore>(machine).cloned() else {
        return;
    };
    if world.get::<super::lathe::OreSilo>(machine).is_some() {
        return;
    }
    let Some(db) = world.get_resource::<ResearchDb>() else {
        return;
    };
    let sheets: Vec<(String, u32)> = store
        .amounts
        .iter()
        .enumerate()
        .filter_map(|(i, &units)| {
            let n = units / SHEET_MATERIAL_AMOUNT as u32;
            Some((db.materials.get(i)?.sheet_type.clone()?, n)).filter(|(_, n)| *n > 0)
        })
        .collect();
    for (sheet, n) in sheets {
        crate::construction::spawn_stack(world, &sheet, pos, n);
    }
}

/// `update_icon_state`: `_n` while printing, `_t` with the panel open.
fn autolathe_looks(
    changed: Query<
        (Entity, &Autolathe, Option<&PanelOpen>, &DmObj),
        Or<(Changed<Autolathe>, Changed<PanelOpen>)>,
    >,
    mut commands: Commands,
) {
    for (e, state, panel, obj) in &changed {
        if !is_under(&obj.path, AUTOLATHE_ROOT) {
            continue;
        }
        let base = "autolathe";
        let icon = if state.busy {
            format!("{base}_n")
        } else if panel.is_some_and(|p| p.0) {
            format!("{base}_t")
        } else {
            base.to_owned()
        };
        commands.queue(move |world: &mut World| {
            crate::obj_damage::set_icon_state(world, e, &icon);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stock_parts::PartRatings;

    #[test]
    fn refresh_parts_numbers() {
        let parts = |bins: &[u32], servo: u32| PartRatings {
            parts: bins
                .iter()
                .map(|&t| (PartKind::MatterBin, t))
                .chain([(PartKind::Servo, servo)])
                .collect(),
            cells: vec![],
        };
        assert_eq!(part_numbers(&parts(&[1, 1, 1], 1)), (11_250, 1.6));
        assert_eq!(part_numbers(&parts(&[1, 1, 1], 2)), (11_250, 1.4));
        assert_eq!(part_numbers(&parts(&[4, 4, 4], 4)), (45_000, 1.0));
        assert_eq!(part_numbers(&parts(&[2, 1, 1], 3)).1, 1.2);
    }
}
