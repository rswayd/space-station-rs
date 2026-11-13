//! The exosuit fabricator (server), after tg `/obj/machinery/mecha_part_fabricator`
//! (`mech_fabricator.dm`) with tgui's `ExosuitFabricator`.
//!
//! - Materials: like the R&D lathes (`crate::science::lathe`), the fabricator links to the ore
//!   silo of its level at startup (`link_on_init`; the maintenance one keeps its own storage of
//!   `100 + 25 * matter bin tiers` sheets) and takes stacks clicked into it; the station silo
//!   needs an ID with a bank account to print or eject.
//! - Designs: the researched `MECHFAB` designs (tg `cached_designs`), costs times
//!   `component_coeff` (`1.15 - 0.15 * micro laser tiers`), times times `time_coeff`
//!   (`1 - (servo tiers - 1) / 5`); tier 1 parts make both 1.
//! - The queue: `build` adds designs (any researched `MECHFAB` design); with `now` it also starts
//!   processing it. While processing, each part takes its materials when it starts (`build_part`:
//!   "Not enough resources. Processing stopped." otherwise) and comes out south of the machine
//!   after its `construction_time`; a dense tile there keeps it inside until it clears ("Error!
//!   The part outlet is obstructed."). `del_queue_part`, `clear_queue`, `build_queue`,
//!   `stop_queue`, `remove_mat` as tg.
//!
//! Cost: one pass per tick over the fabricators that are processing.
// shortcut: no drop direction (`mouse_drop_dragged`), multitool techweb links, active power use,
// sounds, loading overlays, stock part upgrades or deconstruction.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::DmObj;
use ss13_mapload::mecha::{FAB_ROOT, FabDef};
use ss13_mapload::research::{SHEET_MATERIAL_AMOUNT, build_type};
use ss13_shared::{Dir, TilePos};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};
use ss13_world::{SpatialIndex, WorldSystems};

use super::*;
use crate::cargo::console::machine_say;
use crate::click::{ClickResolved, ResolveClicks};
use crate::inventory::Inventory;
use crate::obj_damage::is_under;
use crate::science::lathe::{MaterialStore, OreSilo, optimal_cost, parse_color, plain};
use crate::science::points::{ResearchIcons, ResearchLoaded};
use crate::science::techweb::{ResearchDb, Techweb};

/// A part being built: its design, when it started and when it is done.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Building {
    pub design: usize,
    pub start: f64,
    pub finish: f64,
}

/// The fabricator's state (tg vars).
#[derive(Component, Clone, Debug)]
pub struct MechFab {
    pub silo: Option<Entity>,
    /// `rmat.local_size`, units.
    pub local_size: u32,
    pub component_coeff: f64,
    pub time_coeff: f64,
    /// [`ResearchDb::designs`] indices and who queued them.
    pub queue: Vec<(usize, Entity)>,
    pub process_queue: bool,
    pub being_built: Option<Building>,
    /// A finished part waiting for the outlet (`stored_part`).
    pub stored_part: Option<usize>,
    pub top_job_id: u32,
    catalogue: Arc<ExoFabStatic>,
    design_ids: Arc<Vec<usize>>,
}

/// Queue entries at most: tg has no limit, but every entry is in each UI update and a client
/// can queue hundreds a minute.
pub const MAX_QUEUE: usize = 100;

/// On a fabricator that runs `process()` (tg `begin_processing`).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct FabProcessing;

pub struct FabPlugin;

impl Plugin for FabPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<ExosuitFabricatorUi>()
            .add_systems(
                ss13_world::MapInit,
                setup_fabs
                    .after(ResearchLoaded)
                    .after(super::core::MechaLoaded)
                    .after(crate::science::lathe::LatheSetup),
            )
            .add_systems(
                Update,
                (
                    open_fab
                        .after(WorldSystems::Movement)
                        .after(ResolveClicks)
                        .before(UiSystems::Open),
                    fab_acts.in_set(UiSystems::Handle),
                    process.run_if(any_with_component::<FabProcessing>),
                    // `print_sound`: `build_next_in_queue` .. `on_finish_printing`.
                    crate::looping_sound::sync::<MechFab>(
                        &crate::looping_sound::defs::LATHE_PRINT,
                        |f| f.being_built.is_some(),
                    ),
                ),
            );
    }
}

/// tg `RefreshParts` with tier 1 parts: (local size, component coefficient, time coefficient).
pub fn coefficients(def: &FabDef) -> (u32, f64, f64) {
    let local =
        100.0 * SHEET_MATERIAL_AMOUNT + f64::from(def.matter_bins) * 25.0 * SHEET_MATERIAL_AMOUNT;
    // Rounded: DM's single precision makes `1.15 - 0.15` exactly 1, f64 makes it 0.999...
    let component = ((1.15 - 0.15 * f64::from(def.micro_lasers)) * 1e6).round() / 1e6;
    let t = -1.0 + f64::from(def.servos);
    let time = ((1.0 - t / 5.0) * 100.0).round() / 100.0;
    (local as u32, component, time)
}

/// tg `get_construction_time_w_coeff`: `round(construction_time * time_coeff)`, deciseconds.
pub fn construction_time(construction_time_ds: f64, time_coeff: f64) -> f64 {
    (construction_time_ds * time_coeff).round()
}

/// The designs a fabricator can print once researched and its window's static data.
pub fn catalogue(
    db: &ResearchDb,
    icons: Option<&ResearchIcons>,
    name: &str,
    component_coeff: f64,
    time_coeff: f64,
) -> (ExoFabStatic, Vec<usize>) {
    let mut ids = Vec::new();
    let mut designs = Vec::new();
    for (i, d) in db.designs.iter().enumerate() {
        if d.build_type & build_type::MECHFAB == 0
            || d.build_path.is_none()
            || d.material_requirements
            || d.materials.is_empty()
        {
            continue;
        }
        let cost = d
            .materials
            .iter()
            .filter_map(|(m, n)| {
                let index = *db.material_index.get(m)?;
                Some((index as u8, optimal_cost(n * component_coeff)))
            })
            .collect();
        ids.push(i);
        designs.push(ExoDesign {
            path: d.id.clone(),
            name: d.name.clone(),
            desc: d.desc.clone(),
            cost,
            categories: d.category.clone(),
            icon: icons.and_then(|icons| icons.designs.get(i).copied().flatten()),
            construction_time: construction_time(d.construction_time, time_coeff) as f32,
        });
    }
    let materials = db
        .materials
        .iter()
        .enumerate()
        .map(|(i, m)| MaterialInfo {
            name: m.name.clone(),
            color: m.color.as_deref().and_then(parse_color),
            icon: icons.and_then(|icons| icons.materials.get(i).copied().flatten()),
        })
        .collect();
    (
        ExoFabStatic {
            fab_name: name.to_owned(),
            materials,
            designs,
            sheet_amount: SHEET_MATERIAL_AMOUNT,
        },
        ids,
    )
}

fn setup_fabs(world: &mut World) {
    let mut query = world.query_filtered::<(
        Entity,
        &DmObj,
        Option<&TilePos>,
        Option<&ss13_mapload::DmName>,
    ), With<ss13_world::Mapped>>();
    let found: Vec<(Entity, String, Option<u16>, String)> = query
        .iter(world)
        .filter(|(_, o, _, _)| is_under(&o.path, FAB_ROOT))
        .map(|(e, o, p, n)| {
            let name = n.map_or_else(|| o.path.clone(), |n| plain(&n.name).to_owned());
            (e, o.path.clone(), p.map(|p| p.z), name)
        })
        .collect();
    if found.is_empty() {
        return;
    }
    let mut silos = world.query_filtered::<(Entity, &TilePos), With<OreSilo>>();
    let silos: HashMap<u16, Entity> = silos.iter(world).map(|(e, p)| (p.z, e)).collect();
    let Some(db) = world.get_resource::<ResearchDb>() else {
        return;
    };
    let materials = db.materials.len();
    let icons = world.get_resource::<ResearchIcons>();
    let fab_defs = &world.resource::<MechaDb>().data.fabs;
    let mut inserts = Vec::new();
    for (e, path, z, name) in found {
        let def = fab_defs
            .iter()
            .find(|f| f.path == path)
            .cloned()
            .unwrap_or(FabDef {
                path,
                servos: 1,
                micro_lasers: 1,
                matter_bins: 2,
                link_on_init: true,
            });
        let (local_size, component_coeff, time_coeff) = coefficients(&def);
        let (catalogue, ids) = catalogue(db, icons, &name, component_coeff, time_coeff);
        let silo = def
            .link_on_init
            .then(|| z.and_then(|z| silos.get(&z).copied()))
            .flatten();
        inserts.push((
            e,
            MechFab {
                silo,
                local_size,
                component_coeff,
                time_coeff,
                queue: Vec::new(),
                process_queue: false,
                being_built: None,
                stored_part: None,
                top_job_id: 0,
                catalogue: Arc::new(catalogue),
                design_ids: Arc::new(ids),
            },
        ));
    }
    info!(
        "mecha: {} exosuit fabricators ({} linked to a silo)",
        inserts.len(),
        inserts.iter().filter(|(_, f)| f.silo.is_some()).count()
    );
    for (e, fab) in inserts {
        let local = fab
            .silo
            .is_none()
            .then(|| MaterialStore::new(materials, Some(fab.local_size)));
        let mut entity = world.entity_mut(e);
        entity.insert(fab);
        if let Some(store) = local {
            entity.insert(store);
        }
    }
}

/// The store a fabricator uses and whether it is the station silo.
pub fn store_of(world: &World, fab: Entity) -> (Option<Entity>, bool) {
    let Some(state) = world.get::<MechFab>(fab) else {
        return (None, false);
    };
    match state.silo {
        Some(silo) => (world.get::<MaterialStore>(silo).map(|_| silo), true),
        None => (world.get::<MaterialStore>(fab).map(|_| fab), false),
    }
}

/// tg `ui_interact` on `attack_hand`.
fn open_fab(
    mut clicks: MessageReader<ClickResolved>,
    fabs: Query<&MechFab>,
    inventories: Query<&Inventory>,
    mut open: MessageWriter<OpenUi>,
) {
    for click in clicks.read() {
        let Ok(fab) = fabs.get(click.target) else {
            continue;
        };
        if inventories
            .get(click.mob)
            .is_ok_and(|i| i.active_item().is_none())
        {
            open.write(OpenUi::new::<ExosuitFabricatorUi>(
                click.client,
                click.mob,
                click.target,
                fab.catalogue.fab_name.clone(),
            ));
        }
    }
}

/// tg `remote_materials` insertion of a held stack. True when the click was the fabricator's.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<MechFab>(target).is_none() || world.get::<crate::tools::Stack>(held).is_none() {
        return false;
    }
    if world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0)
    {
        return false;
    }
    let Some(path) = world
        .get::<ss13_mapload::DmItem>(held)
        .map(|i| i.path.clone())
    else {
        return false;
    };
    let (store, _) = store_of(world, target);
    crate::science::lathe::insert_stack(world, client, target, store, held, &path);
    true
}

type FabParams = (
    Option<Res<'static, Techweb>>,
    Query<'static, 'static, &'static MaterialStore, With<OreSilo>>,
    Res<'static, Time>,
);

#[derive(Clone, Debug, PartialEq)]
pub enum FabAct {
    Build(Vec<String>, bool),
    DelQueuePart(f64),
    ClearQueue,
    BuildQueue,
    StopQueue,
    RemoveMat(usize, f64),
}

/// tg `ui_data`.
pub fn fab_data(
    fab: &MechFab,
    own: Option<&MaterialStore>,
    silos: &Query<&MaterialStore, With<OreSilo>>,
    web: Option<&Techweb>,
    now: f64,
) -> ExoFabData {
    let store = fab.silo.and_then(|s| silos.get(s).ok()).or(own);
    let index_of = |design: usize| {
        fab.design_ids
            .iter()
            .position(|&d| d == design)
            .unwrap_or(0) as u16
    };
    let mut queue = Vec::new();
    if let Some(b) = fab.being_built {
        queue.push(ExoQueueEntry {
            job_id: fab.top_job_id,
            design: index_of(b.design),
            processing: true,
            time_left: ((b.finish - now) * 10.0).max(0.0) as f32,
        });
    }
    for (offset, &(design, _)) in fab.queue.iter().enumerate() {
        let i = index_of(design);
        queue.push(ExoQueueEntry {
            job_id: fab.top_job_id + offset as u32 + 1,
            design: i,
            processing: false,
            time_left: fab
                .catalogue
                .designs
                .get(usize::from(i))
                .map_or(0.0, |d| d.construction_time / 10.0),
        });
    }
    ExoFabData {
        materials: store.map(|s| s.amounts.clone()).unwrap_or_default(),
        queue,
        processing: fab.process_queue,
        researched: web
            .map(|web| {
                fab.design_ids
                    .iter()
                    .enumerate()
                    .filter(|(_, d)| web.researched_designs.contains(d))
                    .map(|(i, _)| i as u16)
                    .collect()
            })
            .unwrap_or_default(),
    }
}

impl ServerUi for ExosuitFabricatorUi {
    type Source = (&'static MechFab, Option<&'static MaterialStore>);
    type UserParam = FabParams;
    type Act = FabAct;

    fn static_data((fab, _): (&MechFab, Option<&MaterialStore>)) -> ExoFabStatic {
        (*fab.catalogue).clone()
    }

    fn data(
        (fab, own): (&MechFab, Option<&MaterialStore>),
        _: Entity,
        (web, silos, time): &SystemParamItem<'_, '_, FabParams>,
    ) -> ExoFabData {
        fab_data(fab, own, silos, web.as_deref(), time.elapsed_secs_f64())
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<FabAct, UiReject> {
        match action {
            BUILD => {
                let designs = params
                    .get(DESIGNS)
                    .ok_or_else(|| UiReject::BadParams("designs: missing".into()))?
                    .split(',')
                    .map(str::trim)
                    .filter(|p| !p.is_empty())
                    .map(str::to_owned)
                    .collect();
                let now = params.get(NOW).is_some_and(|v| !v.is_empty() && v != "0");
                Ok(FabAct::Build(designs, now))
            }
            DEL_QUEUE_PART => Ok(FabAct::DelQueuePart(params.number(INDEX)?)),
            CLEAR_QUEUE => Ok(FabAct::ClearQueue),
            BUILD_QUEUE => Ok(FabAct::BuildQueue),
            STOP_QUEUE => Ok(FabAct::StopQueue),
            REMOVE_MAT => {
                let index = params.number(MATERIAL_REF)?;
                if !(0.0..256.0).contains(&index) {
                    return Err(UiReject::BadParams("ref: not a material".into()));
                }
                Ok(FabAct::RemoveMat(index as usize, params.number(AMOUNT)?))
            }
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// tg `/obj/machinery/mecha_part_fabricator/ui_act`.
fn fab_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<ExosuitFabricatorUi>>>) {
    let pending: Vec<(Entity, Entity, FabAct)> = acts
        .read(world.resource::<Messages<UiActed<ExosuitFabricatorUi>>>())
        .map(|a| (a.actor, a.source, a.act.clone()))
        .collect();
    for (actor, fab, act) in pending {
        // tg `can_interact`: no use of an unpowered machine.
        if world.get::<MechFab>(fab).is_none()
            || world.get::<crate::power::Unpowered>(fab).is_some()
            || !world.contains_resource::<ResearchDb>()
        {
            continue;
        }
        match act {
            FabAct::Build(designs, now) => build(world, fab, actor, &designs, now),
            FabAct::DelQueuePart(index) => {
                // `remove_from_queue`: an integer index into the queue, from 1.
                let mut state = world.get_mut::<MechFab>(fab).expect("checked above");
                if index.fract() == 0.0 && index >= 1.0 && (index as usize) <= state.queue.len() {
                    state.queue.remove(index as usize - 1);
                }
            }
            FabAct::ClearQueue => world
                .get_mut::<MechFab>(fab)
                .expect("checked")
                .queue
                .clear(),
            FabAct::BuildQueue => start_processing(world, fab),
            FabAct::StopQueue => {
                world
                    .get_mut::<MechFab>(fab)
                    .expect("checked")
                    .process_queue = false;
            }
            FabAct::RemoveMat(material, amount) => {
                let (store, linked) = store_of(world, fab);
                crate::science::lathe::eject_sheets(
                    world, fab, actor, material, amount, store, linked,
                );
            }
        }
    }
}

/// `ui_act("build")`: the researched `MECHFAB` designs into the queue; `now` starts processing.
fn build(world: &mut World, fab: Entity, user: Entity, designs: &[String], now: bool) {
    let (store, linked) = store_of(world, fab);
    if !crate::science::lathe::silo_permitted(world, fab, user, store, linked) {
        return;
    }
    let db = world.resource::<ResearchDb>();
    let web = world.get_resource::<Techweb>();
    let queued: Vec<usize> = designs
        .iter()
        .filter_map(|p| db.design_index.get(p).copied())
        .filter(|&d| web.is_some_and(|w| w.researched_designs.contains(&d)))
        .filter(|&d| db.designs[d].build_type & build_type::MECHFAB != 0)
        .collect();
    let mut state = world.get_mut::<MechFab>(fab).expect("checked by fab_acts");
    let room = MAX_QUEUE.saturating_sub(state.queue.len());
    state
        .queue
        .extend(queued.into_iter().take(room).map(|d| (d, user)));
    if now && !state.process_queue {
        start_processing(world, fab);
    }
}

/// `process_queue = TRUE` and `begin_processing` unless a part is being built.
fn start_processing(world: &mut World, fab: Entity) {
    let Some(mut state) = world.get_mut::<MechFab>(fab) else {
        return;
    };
    if state.process_queue {
        return;
    }
    state.process_queue = true;
    if state.being_built.is_none() {
        world.entity_mut(fab).insert(FabProcessing);
    }
}

/// tg `process()` of each processing fabricator.
fn process(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut query = world.query_filtered::<Entity, With<FabProcessing>>();
    let fabs: Vec<Entity> = query.iter(world).collect();
    for fab in fabs {
        process_one(world, fab, now);
    }
}

fn process_one(world: &mut World, fab: Entity, now: f64) {
    let Some(state) = world.get::<MechFab>(fab).cloned() else {
        world.entity_mut(fab).remove::<FabProcessing>();
        return;
    };
    // shortcut: tg keeps processing without power; here it pauses like the lathes (a part due
    // meanwhile comes out when power returns).
    if world.get::<crate::power::Unpowered>(fab).is_some() {
        return;
    }
    if let Some(design) = state.stored_part {
        let Some(exit) = exit_tile(world, fab) else {
            return;
        };
        if blocked(world, exit) {
            return;
        }
        world.get_mut::<MechFab>(fab).expect("checked").stored_part = None;
        if let Some(item) = create_result(world, design, exit) {
            let thing = super::core::name_of(world, item);
            machine_say(
                world,
                fab,
                &format!("Obstruction cleared. The fabrication of {thing} is now complete."),
            );
        }
    }
    if state.being_built.is_none() {
        if !state.process_queue || !build_next_in_queue(world, fab, now, true) {
            // `on_finish_printing` + `end_processing`.
            world
                .get_mut::<MechFab>(fab)
                .expect("checked")
                .process_queue = false;
            world.entity_mut(fab).remove::<FabProcessing>();
        }
        return;
    }
    if let Some(b) = state.being_built
        && b.finish < now
    {
        dispense(world, fab, b.design);
        if world.get::<MechFab>(fab).is_some_and(|f| f.process_queue) {
            build_next_in_queue(world, fab, now, false);
        }
    }
}

/// `build_next_in_queue`: the first queued design starts, or nothing.
fn build_next_in_queue(world: &mut World, fab: Entity, now: f64, verbose: bool) -> bool {
    let Some(&(design, user)) = world.get::<MechFab>(fab).and_then(|f| f.queue.first()) else {
        return false;
    };
    if build_part(world, fab, design, user, now, verbose) {
        world
            .get_mut::<MechFab>(fab)
            .expect("checked")
            .queue
            .remove(0);
        return true;
    }
    false
}

/// tg `build_part`: the materials are taken when the part starts.
fn build_part(
    world: &mut World,
    fab: Entity,
    design: usize,
    user: Entity,
    now: f64,
    verbose: bool,
) -> bool {
    let (store, linked) = store_of(world, fab);
    if store.is_none() {
        if verbose {
            machine_say(
                world,
                fab,
                "No access to material storage, please contact the quartermaster.",
            );
        }
        return false;
    }
    if !crate::science::lathe::silo_permitted(world, fab, user, store, linked) {
        return false;
    }
    let state = world
        .get::<MechFab>(fab)
        .expect("checked by the caller")
        .clone();
    let db = world.resource::<ResearchDb>();
    let cost = crate::science::lathe::design_cost(db, design);
    let time = construction_time(db.designs[design].construction_time, state.time_coeff) / 10.0;
    let store = store.expect("checked above");
    let enough = world
        .get::<MaterialStore>(store)
        .is_some_and(|s| s.has_materials(&cost, state.component_coeff, 1));
    if !enough {
        if verbose {
            machine_say(world, fab, "Not enough resources. Processing stopped.");
        }
        return false;
    }
    if let Some(mut s) = world.get_mut::<MaterialStore>(store) {
        s.use_materials(&cost, state.component_coeff, 1);
    }
    world.get_mut::<MechFab>(fab).expect("checked").being_built = Some(Building {
        design,
        start: now,
        finish: now + time,
    });
    true
}

/// `get_step(src, drop_direction)` with the default `SOUTH`.
fn exit_tile(world: &World, fab: Entity) -> Option<TilePos> {
    world.get::<TilePos>(fab).map(|p| p.step(Dir::South))
}

/// `exit.density`: the turf or anything dense on it.
fn blocked(world: &World, pos: TilePos) -> bool {
    !world
        .resource::<SpatialIndex>()
        .dense_on_tile(pos)
        .is_empty()
}

/// `design.create_result(src)`.
fn create_result(world: &mut World, design: usize, pos: TilePos) -> Option<Entity> {
    let db = world.resource::<ResearchDb>();
    let path = db.designs[design].build_path.clone()?;
    if db.designs[design].stack {
        crate::construction::spawn_stack(world, &path, pos, 1)
    } else {
        crate::construction::spawn_obj(world, &path, pos)
    }
}

/// tg `dispense_built_part`.
fn dispense(world: &mut World, fab: Entity, design: usize) {
    world.get_mut::<MechFab>(fab).expect("checked").being_built = None;
    let Some(exit) = exit_tile(world, fab) else {
        return;
    };
    if blocked(world, exit) {
        machine_say(world, fab, "Error! The part outlet is obstructed.");
        world.get_mut::<MechFab>(fab).expect("checked").stored_part = Some(design);
        return;
    }
    if let Some(item) = create_result(world, design, exit) {
        let thing = super::core::name_of(world, item);
        machine_say(
            world,
            fab,
            &format!("The fabrication of {thing} is now complete."),
        );
    }
    world.get_mut::<MechFab>(fab).expect("checked").top_job_id += 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_one_parts_change_nothing() {
        let def = FabDef {
            servos: 1,
            micro_lasers: 1,
            matter_bins: 2,
            ..default()
        };
        let (local, component, time) = coefficients(&def);
        assert_eq!(local, 15_000, "100 + 2 * 25 sheets");
        assert_eq!(component, 1.0, "OPTIMAL_COST(1000 * coeff) must stay 1000");
        assert!((time - 1.0).abs() < 1e-9);
        // Two servos: 0.8 (tg `round(1 - 1/5, 0.01)`).
        let (_, _, time) = coefficients(&FabDef { servos: 2, ..def });
        assert!((time - 0.8).abs() < 1e-9);
        assert_eq!(construction_time(100.0, 0.8), 80.0);
    }
}
