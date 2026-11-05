//! R&D production machines (server), after tg `/obj/machinery/rnd/production`
//! (`machinery/_production.dm`): the protolathe, the circuit imprinter, the techfab and their
//! departmental subtypes, with tgui's `Fabricator` window; and their material storage, tg
//! `/datum/remote_materials` with `/datum/material_container` ([`MaterialStore`]).
//!
//! - Storage: at startup every lathe links to the ore silo (`/obj/machinery/ore_silo`) on its z
//!   and shares its storage (tg links station machines to `GLOB.ore_silo_default` at round
//!   start); without one it keeps a local store of `matter bins * 37.5` sheets. Material units
//!   are tg's (`SHEET_MATERIAL_AMOUNT` = 100 per sheet). A click with a stack out of combat mode
//!   inserts it (its `mats_per_unit` of the silo materials, split when it does not fit), as
//!   `user_insert` does; `remove_mat` ejects sheets. The station silo requires an ID with a bank
//!   account (or quartermaster access) to print or eject (`ID_required`, `check_permitted`).
//! - Printing: `build` checks the design is researched, allowed by the machine's department
//!   flags and build types, the amount (1-50) and the materials; then items come out one per
//!   `(construction_time * lathe_time_factor * efficiency) ** 0.8` deciseconds, each using its
//!   materials (times the efficiency, except stacks and `fixed_cost_efficiency` designs) and
//!   energy; a stack design prints all its units at once. `RefreshParts` ([`refresh_parts`], after
//!   an RPED; mapped ones have their board's tier 1 parts): efficiency is `1.2 - 0.1 * servo
//!   tiers` (protolathe, techfab) or `0.5 ** (servo tiers - 1)` (imprinter), the local storage
//!   `37.5 sheets * matter bin tiers`.
//!
//! Cost: lathes are scanned once at startup; a print is one timer per lathe that is printing.
// shortcut: no silo holds, bans, logs or the silo's own window; no material insertion energy,
// animations, department stripes, drop direction (`mouse_drop_dragged`), wires,
// deconstruction (a screwdriver opens the panel for the RPED only), design materials transferred to the printed item, the
// payment component (departmental lathes are free), or designs whose materials are material
// classes (`/datum/material_requirement`), which are not offered.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::research::{
    ALL_FLAGS, LatheDef, MAX_STACK_SIZE, PRODUCTION_ROOT, SHEET_MATERIAL_AMOUNT, build_type,
};
use ss13_mapload::{DmName, DmObj, DmPower, DmStack};
use ss13_proto::Prototypes;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::UiInterface as _;
use ss13_tgui::UiStatus;
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};
use ss13_world::WorldSystems;

use super::points::{ResearchIcons, ResearchLoaded};
use super::techweb::{ResearchDb, Techweb};
use super::*;
use crate::cargo::console::machine_say;
use crate::click::{ClickResolved, ResolveClicks};
use crate::combat::CombatMode;
use crate::inventory::{IdCard, Inventory};
use crate::obj_damage::is_under;
use crate::power::{PowerMachine, Unpowered};
use crate::tools::{Stack, tell};

pub const SILO_ROOT: &str = "/obj/machinery/ore_silo";
/// tg `/obj/machinery/component_printer` (`wiremod/core/component_printer.dm`): a lathe of
/// `COMPONENT_PRINTER` designs that prints at once and draws no energy per print, its
/// efficiency `1 - 0.875 * (servo tiers - servos) / 6`.
// shortcut: the `Fabricator` window instead of tgui's `ComponentPrinter`; no recycling of
// components, no circuit linking for remote printing.
pub const COMPONENT_PRINTER: &str = "/obj/machinery/component_printer";

/// The component printer's lathe def: its board has 2 matter bins and 2 servos.
fn component_printer_def() -> LatheDef {
    LatheDef {
        path: COMPONENT_PRINTER.to_owned(),
        build_types: build_type::COMPONENT_PRINTER,
        department_flags: ALL_FLAGS,
        imprinter: false,
        servos: 2,
        matter_bins: 2,
        payment_department: String::new(),
    }
}

fn is_component_printer(def: &LatheDef) -> bool {
    def.path == COMPONENT_PRINTER
}
/// `ACCESS_QM`: the silo's ID requirement does not apply.
const ACCESS_QM: &str = "qm";
/// `37.5 * SHEET_MATERIAL_AMOUNT` per matter bin tier.
const UNITS_PER_BIN: f64 = 37.5 * SHEET_MATERIAL_AMOUNT;

/// tg `OPTIMAL_COST`: `max(1, floor(cost))`.
pub fn optimal_cost(cost: f64) -> u32 {
    cost.floor().max(1.0) as u32
}

/// tg `/datum/material_container`: units of each [`ResearchDb::materials`] entry.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct MaterialStore {
    pub amounts: Vec<u32>,
    /// `max_amount`; `None` is the silo's `INFINITY`.
    pub max: Option<u32>,
}

impl MaterialStore {
    pub fn new(materials: usize, max: Option<u32>) -> Self {
        Self {
            amounts: vec![0; materials],
            max,
        }
    }

    pub fn total(&self) -> u64 {
        self.amounts.iter().map(|&n| u64::from(n)).sum()
    }

    /// Units that still fit.
    pub fn space(&self) -> u64 {
        self.max
            .map_or(u64::MAX, |m| u64::from(m).saturating_sub(self.total()))
    }

    /// tg `has_materials(mats, coefficient, multiplier)`: `OPTIMAL_COST(units * coefficient) *
    /// multiplier` of each.
    pub fn has_materials(&self, cost: &[(usize, f64)], coefficient: f64, multiplier: u32) -> bool {
        !cost.is_empty()
            && cost.iter().all(|&(m, n)| {
                let wanted = u64::from(optimal_cost(n * coefficient)) * u64::from(multiplier);
                self.amounts
                    .get(m)
                    .is_some_and(|&have| u64::from(have) >= wanted)
            })
    }

    /// tg `use_materials`; call after [`Self::has_materials`].
    pub fn use_materials(&mut self, cost: &[(usize, f64)], coefficient: f64, multiplier: u32) {
        for &(m, n) in cost {
            let wanted = optimal_cost(n * coefficient).saturating_mul(multiplier);
            if let Some(have) = self.amounts.get_mut(m)
                && *have >= wanted
            {
                *have -= wanted;
            }
        }
    }

    /// tg `insert_item` of `sheets` units of a stack with `per_unit` materials: as many as fit.
    /// Returns the stack units taken.
    pub fn insert_stack(&mut self, per_unit: &[(usize, f64)], sheets: u32) -> u32 {
        let per_sheet: f64 = per_unit.iter().map(|(_, n)| n).sum();
        if per_sheet <= 0.0 || sheets == 0 {
            return 0;
        }
        let space = self.space() as f64;
        let fit = if space >= per_sheet * f64::from(sheets) {
            sheets
        } else {
            (space / per_sheet).floor() as u32
        };
        if fit == 0 {
            return 0;
        }
        for &(m, n) in per_unit {
            if let Some(have) = self.amounts.get_mut(m) {
                *have = have.saturating_add(optimal_cost(n * f64::from(fit)));
            }
        }
        fit
    }
}

/// The station's ore silo (its [`MaterialStore`] has no limit).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct OreSilo;

/// An R&D production machine.
#[derive(Component, Clone, Debug)]
pub struct Lathe {
    pub def: LatheDef,
    /// `efficiency_coeff`.
    pub efficiency: f64,
    /// `local_size` in units.
    pub local_size: u32,
    /// The linked silo; `None`: the lathe's own [`MaterialStore`].
    pub silo: Option<Entity>,
    pub busy: bool,
    catalogue: Arc<FabricatorStatic>,
    /// [`ResearchDb::designs`] index of each catalogue design.
    design_ids: Arc<Vec<usize>>,
}

impl Lathe {
    /// tg `build_efficiency(design)`: stacks and `fixed_cost_efficiency` designs use exact
    /// materials.
    pub fn build_efficiency(&self, db: &ResearchDb, design: usize) -> f64 {
        let d = &db.designs[design];
        if d.stack || d.fixed_cost_efficiency {
            1.0
        } else {
            self.efficiency
        }
    }
}

/// tg `compute_efficiency()` with tier 1 parts.
pub fn efficiency(def: &LatheDef) -> f64 {
    if is_component_printer(def) {
        return 1.0;
    }
    efficiency_for(def.imprinter, def.servos)
}

/// tg `compute_efficiency()` for a sum of servo tiers: `0.5 ** max(servos - 1, 0)` (imprinter),
/// `1.2 - 0.1 * servos` (the others).
pub fn efficiency_for(imprinter: bool, servos: u32) -> f64 {
    if imprinter {
        0.5_f64.powi(servos.saturating_sub(1) as i32)
    } else {
        1.2 - 0.1 * f64::from(servos)
    }
}

/// tg `production/RefreshParts` (from [`crate::stock_parts::refresh`]): the local storage is
/// `37.5 sheets * matter bin tiers`, the efficiency from the servos; the window's costs follow.
pub(crate) fn refresh_parts(world: &mut World, e: Entity) {
    let Some(lathe) = world.get::<Lathe>(e).cloned() else {
        return;
    };
    let r = crate::stock_parts::ratings(world, e);
    let bins = r.sum(crate::stock_parts::PartKind::MatterBin);
    let servos = r.sum(crate::stock_parts::PartKind::Servo);
    let efficiency = if is_component_printer(&lathe.def) {
        1.0 + (0.125 - 1.0) * (f64::from(servos.saturating_sub(lathe.def.servos)) / 6.0)
    } else {
        efficiency_for(lathe.def.imprinter, servos)
    };
    let local_size = (f64::from(bins) * UNITS_PER_BIN) as u32;
    let catalogue = world.get_resource::<ResearchDb>().map(|db| {
        let mut c = (*lathe.catalogue).clone();
        priced(db, &mut c, &lathe.design_ids, efficiency);
        Arc::new(c)
    });
    if let Some(mut l) = world.get_mut::<Lathe>(e) {
        l.efficiency = efficiency;
        l.local_size = local_size;
        if let Some(c) = catalogue {
            l.catalogue = c;
        }
    }
    if let Some(mut store) = world.get_mut::<MaterialStore>(e) {
        store.max = Some(local_size);
    }
}

/// The catalogue's costs at `efficiency` (stacks and fixed designs at 1).
fn priced(db: &ResearchDb, st: &mut FabricatorStatic, ids: &[usize], efficiency: f64) {
    for (entry, &i) in st.designs.iter_mut().zip(ids) {
        let d = &db.designs[i];
        let coefficient = if d.stack || d.fixed_cost_efficiency {
            1.0
        } else {
            efficiency
        };
        entry.cost = d
            .materials
            .iter()
            .filter_map(|(m, n)| {
                let index = *db.material_index.get(m)?;
                Some((index as u8, optimal_cost(n * coefficient)))
            })
            .collect();
    }
}

/// `(construction_time * lathe_time_factor * efficiency_coeff) ** 0.8`, in seconds.
pub fn build_secs(construction_time_ds: f64, lathe_time_factor: f64, efficiency: f64) -> f64 {
    (construction_time_ds * lathe_time_factor * efficiency).powf(0.8) / 10.0
}

/// A print in progress (tg's `do_make_item` timer chain).
#[derive(Component, Clone, Debug)]
pub struct Printing {
    pub design: usize,
    pub remaining: u32,
    pub per_item_secs: f64,
    pub coefficient: f64,
    /// Joules per item.
    pub charge: f64,
    pub target: TilePos,
    pub user: Entity,
    pub next_at: f64,
}

/// After the lathes and the ore silos are set up at startup.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LatheSetup;

pub struct LathePlugin;

impl Plugin for LathePlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<FabricatorUi>()
            .add_systems(
                Startup,
                setup_lathes.after(ResearchLoaded).in_set(LatheSetup),
            )
            .add_systems(
                Update,
                (
                    open_lathe
                        .after(WorldSystems::Movement)
                        .after(ResolveClicks)
                        .before(UiSystems::Open),
                    lathe_acts.in_set(UiSystems::Handle),
                    unpowered_windows.in_set(UiSystems::StatusHook),
                    print_tick.run_if(any_with_component::<Printing>),
                ),
            );
    }
}

/// The designs `def` could print and its window's static data.
pub fn catalogue(
    db: &ResearchDb,
    icons: Option<&ResearchIcons>,
    def: &LatheDef,
    name: &str,
) -> (FabricatorStatic, Vec<usize>) {
    let coefficient = efficiency(def);
    let mut ids = Vec::new();
    let mut designs = Vec::new();
    for (i, d) in db.designs.iter().enumerate() {
        if d.build_path.is_none()
            || d.material_requirements
            || d.materials.is_empty()
            || d.departmental_flags & def.department_flags == 0
            || d.build_type & def.build_types == 0
        {
            continue;
        }
        let coefficient = if d.stack || d.fixed_cost_efficiency {
            1.0
        } else {
            coefficient
        };
        let cost = d
            .materials
            .iter()
            .filter_map(|(m, n)| {
                let index = *db.material_index.get(m)?;
                Some((index as u8, optimal_cost(n * coefficient)))
            })
            .collect();
        ids.push(i);
        designs.push(FabDesign {
            path: d.id.clone(),
            name: d.name.clone(),
            desc: d.desc.clone(),
            cost,
            categories: d.category.clone(),
            icon: icons.and_then(|icons| icons.designs.get(i).copied().flatten()),
        });
    }
    (
        FabricatorStatic {
            fab_name: name.to_owned(),
            materials: material_infos(db, icons),
            designs,
            sheet_amount: SHEET_MATERIAL_AMOUNT,
        },
        ids,
    )
}

/// `mat_container.ui_static_data`: every silo material with its color and sheet icon.
pub(crate) fn material_infos(db: &ResearchDb, icons: Option<&ResearchIcons>) -> Vec<MaterialInfo> {
    db.materials
        .iter()
        .enumerate()
        .map(|(i, m)| MaterialInfo {
            name: m.name.clone(),
            color: m.color.as_deref().and_then(parse_color),
            icon: icons.and_then(|icons| icons.materials.get(i).copied().flatten()),
        })
        .collect()
}

pub(crate) fn parse_color(hex: &str) -> Option<[u8; 4]> {
    let hex = hex.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?, 255])
}

pub(crate) fn plain(name: &str) -> &str {
    name.trim_start_matches("\\improper ")
        .trim_start_matches("\\proper ")
}

fn setup_lathes(world: &mut World) {
    let mut query = world.query::<(Entity, &DmObj, Option<&TilePos>, Option<&DmName>)>();
    let found: Vec<(Entity, DmObj, Option<TilePos>, Option<DmName>)> = query
        .iter(world)
        .filter(|(_, o, _, _)| {
            is_under(&o.path, SILO_ROOT)
                || is_under(&o.path, PRODUCTION_ROOT)
                || o.path == COMPONENT_PRINTER
        })
        .map(|(e, o, p, n)| (e, o.clone(), p.copied(), n.cloned()))
        .collect();
    let Some(db) = world.get_resource::<ResearchDb>() else {
        return;
    };
    let materials = db.materials.len();
    let mut silos: HashMap<u16, Entity> = HashMap::new();
    let mut lathes = Vec::new();
    for (e, obj, pos, name) in found {
        if is_under(&obj.path, SILO_ROOT) {
            if let Some(pos) = pos {
                silos.entry(pos.z).or_insert(e);
            }
        } else if let Some(def) = db
            .lathes
            .get(&obj.path)
            .cloned()
            .or_else(|| (obj.path == COMPONENT_PRINTER).then(component_printer_def))
        {
            let name = name.map_or_else(|| obj.path.clone(), |n| plain(&n.name).to_owned());
            lathes.push((e, def, pos.map(|p| p.z), name));
        }
    }
    let icons = world.get_resource::<ResearchIcons>();
    let mut catalogues: HashMap<String, (Arc<FabricatorStatic>, Arc<Vec<usize>>)> = HashMap::new();
    let mut inserts = Vec::new();
    for (e, def, z, name) in lathes {
        let (catalogue, design_ids) = catalogues
            .entry(def.path.clone())
            .or_insert_with(|| {
                let (c, ids) = catalogue(db, icons, &def, &name);
                (Arc::new(c), Arc::new(ids))
            })
            .clone();
        let silo = z.and_then(|z| silos.get(&z).copied());
        let lathe = Lathe {
            efficiency: efficiency(&def),
            local_size: (f64::from(def.matter_bins) * UNITS_PER_BIN) as u32,
            silo,
            busy: false,
            catalogue,
            design_ids,
            def,
        };
        inserts.push((e, lathe));
    }
    let silo_count = silos.len();
    info!(
        "research: {} lathes ({} linked to a silo), {silo_count} ore silos",
        inserts.len(),
        inserts.iter().filter(|(_, l)| l.silo.is_some()).count()
    );
    for silo in silos.into_values() {
        world
            .entity_mut(silo)
            .insert((OreSilo, MaterialStore::new(materials, None)));
    }
    for (e, lathe) in inserts {
        let local = lathe
            .silo
            .is_none()
            .then(|| MaterialStore::new(materials, Some(lathe.local_size)));
        let mut entity = world.entity_mut(e);
        entity.insert(lathe);
        if let Some(store) = local {
            entity.insert(store);
        }
    }
}

/// The store a lathe uses: its silo's or its own.
pub fn store_of(world: &World, lathe: Entity) -> Option<Entity> {
    match world.get::<Lathe>(lathe)?.silo {
        Some(silo) if world.get::<MaterialStore>(silo).is_some() => Some(silo),
        Some(_) => None,
        None => world.get::<MaterialStore>(lathe).map(|_| lathe),
    }
}

/// tg `ui_interact` on `attack_hand`.
fn open_lathe(
    mut clicks: MessageReader<ClickResolved>,
    lathes: Query<&Lathe>,
    inventories: Query<&Inventory>,
    mut open: MessageWriter<OpenUi>,
) {
    for click in clicks.read() {
        let Ok(lathe) = lathes.get(click.target) else {
            continue;
        };
        if inventories
            .get(click.mob)
            .is_ok_and(|i| i.active_item().is_none())
        {
            let title = lathe.catalogue.fab_name.clone();
            open.write(OpenUi::new::<FabricatorUi>(
                click.client,
                click.mob,
                click.target,
                title,
            ));
        }
    }
}

fn unpowered_windows(
    mut windows: Query<&mut UiWindow>,
    unpowered: Query<(), (With<Lathe>, With<Unpowered>)>,
) {
    for mut window in &mut windows {
        if window.interface == FabricatorUi::NAME && unpowered.contains(window.source) {
            window.cap_status(UiStatus::Update);
        }
    }
}

type LatheParams = (
    Option<Res<'static, Techweb>>,
    Query<'static, 'static, &'static MaterialStore, With<OreSilo>>,
);

#[derive(Clone, Debug, PartialEq)]
pub enum LatheAct {
    Build(String, f64),
    RemoveMat(usize, f64),
}

/// tg `ui_data`.
pub fn lathe_data(
    lathe: &Lathe,
    own: Option<&MaterialStore>,
    silos: &Query<&MaterialStore, With<OreSilo>>,
    web: Option<&Techweb>,
) -> FabricatorData {
    let store = lathe.silo.and_then(|s| silos.get(s).ok()).or(own);
    FabricatorData {
        materials: store.map(|s| s.amounts.clone()).unwrap_or_default(),
        on_hold: false,
        busy: lathe.busy,
        material_maximum: lathe.local_size,
        researched: web
            .map(|web| {
                lathe
                    .design_ids
                    .iter()
                    .enumerate()
                    .filter(|(_, d)| web.researched_designs.contains(d))
                    .map(|(i, _)| i as u16)
                    .collect()
            })
            .unwrap_or_default(),
    }
}

impl ServerUi for FabricatorUi {
    type Source = (&'static Lathe, Option<&'static MaterialStore>);
    type UserParam = LatheParams;
    type Act = LatheAct;

    fn static_data((lathe, _): (&Lathe, Option<&MaterialStore>)) -> FabricatorStatic {
        (*lathe.catalogue).clone()
    }

    fn data(
        (lathe, own): (&Lathe, Option<&MaterialStore>),
        _: Entity,
        (web, silos): &SystemParamItem<'_, '_, LatheParams>,
    ) -> FabricatorData {
        lathe_data(lathe, own, silos, web.as_deref())
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<LatheAct, UiReject> {
        match action {
            BUILD => Ok(LatheAct::Build(
                params
                    .get(DESIGN_PATH)
                    .map(str::to_owned)
                    .ok_or_else(|| UiReject::BadParams("design_path: missing".into()))?,
                params.number(AMOUNT)?,
            )),
            REMOVE_MAT => {
                let index = params.number(MATERIAL_REF)?;
                if !(0.0..256.0).contains(&index) {
                    return Err(UiReject::BadParams("ref: not a material".into()));
                }
                Ok(LatheAct::RemoveMat(index as usize, params.number(AMOUNT)?))
            }
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// The ID card `get_idcard(hand_first = TRUE)` finds.
fn actor_card(world: &World, mob: Entity) -> Option<Entity> {
    let inventory = world.get::<Inventory>(mob)?;
    [
        inventory.hands[inventory.active],
        inventory.hands[1 - inventory.active],
        inventory.id,
    ]
    .into_iter()
    .flatten()
    .find(|&e| {
        world.get::<IdCard>(e).is_some()
            || world
                .get::<crate::modpc::pda::ModularComputer>(e)
                .is_some_and(|c| c.stored_id.is_some())
    })
}

/// tg `can_use_resource(user_data = ID_DATA(user))`: a silo-linked lathe needs the user's ID to
/// carry a bank account or quartermaster access (`check_permitted` of the station silo).
fn can_use_resource(world: &mut World, lathe: Entity, user: Entity) -> bool {
    let Some(state) = world.get::<Lathe>(lathe) else {
        return false;
    };
    let linked = state.silo.is_some();
    let store = store_of(world, lathe);
    silo_permitted(world, lathe, user, store, linked)
}

/// `remote_materials.can_use_resource` of any machine with a material store: `store` is the one
/// it uses (`None`: its silo is gone), `linked` whether that is the station silo.
pub(crate) fn silo_permitted(
    world: &mut World,
    machine: Entity,
    user: Entity,
    store: Option<Entity>,
    linked: bool,
) -> bool {
    if store.is_none() {
        machine_say(
            world,
            machine,
            "No access to material storage, please contact the quartermaster.",
        );
        return false;
    }
    if !linked {
        return true;
    }
    let card = actor_card(world, user);
    let card_entity = card.and_then(|c| {
        world
            .get::<crate::modpc::pda::ModularComputer>(c)
            .and_then(|m| m.stored_id)
            .or(Some(c))
    });
    let qm = card_entity
        .and_then(|c| world.get::<IdCard>(c))
        .is_some_and(|c| c.access.iter().any(|a| a == ACCESS_QM));
    if qm {
        return true;
    }
    let Some(card) = card else {
        machine_say(
            world,
            machine,
            "SILO ERR: ID interface failure. Please contact the Head of Personnel.",
        );
        return false;
    };
    if crate::cargo::economy::account_of(world, card).is_none() {
        machine_say(
            world,
            machine,
            "SILO ERR: No account ID found. Please contact Head of Personnel.",
        );
        return false;
    }
    true
}

fn lathe_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<FabricatorUi>>>) {
    let pending: Vec<(Entity, Entity, LatheAct)> = acts
        .read(world.resource::<Messages<UiActed<FabricatorUi>>>())
        .map(|a| (a.actor, a.source, a.act.clone()))
        .collect();
    for (actor, lathe, act) in pending {
        if world.get::<Unpowered>(lathe).is_some() || !world.contains_resource::<ResearchDb>() {
            continue;
        }
        match act {
            LatheAct::Build(path, amount) => {
                start_build(world, lathe, actor, &path, amount);
            }
            LatheAct::RemoveMat(material, amount) => {
                eject(world, lathe, actor, material, amount);
            }
        }
    }
}

/// tg `ui_act("build")`.
fn start_build(world: &mut World, lathe: Entity, user: Entity, path: &str, amount: f64) -> bool {
    let Some(state) = world.get::<Lathe>(lathe).cloned() else {
        return false;
    };
    if state.busy {
        machine_say(world, lathe, "Warning: fabricator is busy!");
        return false;
    }
    let db = world.resource::<ResearchDb>();
    let Some(&design) = db.design_index.get(path) else {
        return false;
    };
    let researched = world
        .get_resource::<Techweb>()
        .is_some_and(|w| w.researched_designs.contains(&design));
    if !researched || !state.design_ids.contains(&design) {
        return false;
    }
    let d = &db.designs[design];
    if d.departmental_flags & state.def.department_flags == 0 {
        machine_say(
            world,
            lathe,
            "This fabricator does not have the necessary keys to decrypt this design.",
        );
        return false;
    }
    if d.build_type & state.def.build_types == 0 {
        machine_say(
            world,
            lathe,
            "This fabricator does not have the necessary manipulation systems for this design.",
        );
        return false;
    }
    if !amount.is_finite() {
        return false;
    }
    let quantity = (amount as i64).clamp(1, i64::from(MAX_PRINT)) as u32;
    let coefficient = state.build_efficiency(db, design);
    let cost = design_cost(db, design);
    let total_units: f64 = d.materials.iter().map(|(_, n)| n).sum();
    let printer = is_component_printer(&state.def);
    let per_item_secs = if printer {
        0.0
    } else {
        build_secs(d.construction_time, d.lathe_time_factor, state.efficiency)
    };
    let active = if printer {
        0.0
    } else {
        world
            .get::<DmPower>(lathe)
            .map_or(0.0, |p| p.active_power_usage)
    };
    let charge =
        (total_units / (f64::from(MAX_STACK_SIZE) * SHEET_MATERIAL_AMOUNT) * coefficient * active)
            .ceil();
    if !can_use_resource(world, lathe, user) {
        return false;
    }
    let enough = store_of(world, lathe)
        .and_then(|s| world.get::<MaterialStore>(s))
        .is_some_and(|s| s.has_materials(&cost, coefficient, quantity));
    if !enough {
        let text = format!(
            "Not enough materials to complete prototype{}.",
            if quantity > 1 { "s" } else { "" }
        );
        machine_say(world, lathe, &text);
        return false;
    }
    let Some(&target) = world.get::<TilePos>(lathe) else {
        return false;
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(mut l) = world.get_mut::<Lathe>(lathe) {
        l.busy = true;
    }
    world.entity_mut(lathe).insert(Printing {
        design,
        remaining: quantity,
        per_item_secs,
        coefficient,
        charge,
        target,
        user,
        next_at: now + per_item_secs,
    });
    true
}

/// A design's materials as [`ResearchDb::materials`] indices.
pub fn design_cost(db: &ResearchDb, design: usize) -> Vec<(usize, f64)> {
    db.designs[design]
        .materials
        .iter()
        .filter_map(|(m, n)| Some((*db.material_index.get(m)?, *n)))
        .collect()
}

fn print_tick(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut query = world.query::<(Entity, &Printing)>();
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
    world.entity_mut(lathe).remove::<Printing>();
    if let Some(mut l) = world.get_mut::<Lathe>(lathe) {
        l.busy = false;
    }
}

/// tg `do_make_item`: one item (or the whole stack) of the print.
fn make_item(world: &mut World, lathe: Entity, now: f64) {
    let Some(mut job) = world.get::<Printing>(lathe).cloned() else {
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
    if !can_use_resource(world, lathe, job.user) {
        finalize(world, lathe);
        return;
    }
    let db = world.resource::<ResearchDb>();
    let design = &db.designs[job.design];
    let stack = design.stack;
    let build_path = design.build_path.clone().unwrap_or_default();
    let cost = design_cost(db, job.design);
    let multiplier = if stack { job.remaining } else { 1 };
    let Some(store) = store_of(world, lathe) else {
        finalize(world, lathe);
        return;
    };
    let mut materials = world
        .get_mut::<MaterialStore>(store)
        .expect("store_of checked it");
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
        crate::construction::spawn_stack(world, &build_path, job.target, per_item * job.remaining);
        job.remaining = 0;
    } else {
        crate::construction::spawn_obj(world, &build_path, job.target);
        job.remaining -= 1;
    }
    if job.remaining == 0 {
        finalize(world, lathe);
        return;
    }
    job.next_at = now + job.per_item_secs;
    world.entity_mut(lathe).insert(job);
}

/// tg `ui_act("remove_mat")` -> `eject_sheets` -> `retrieve_stack`.
fn eject(world: &mut World, lathe: Entity, user: Entity, material: usize, amount: f64) {
    let linked = world.get::<Lathe>(lathe).is_some_and(|l| l.silo.is_some());
    let store = store_of(world, lathe);
    eject_sheets(world, lathe, user, material, amount, store, linked);
}

/// `remote_materials.eject_sheets` of any machine with a material store (see [`silo_permitted`]).
pub(crate) fn eject_sheets(
    world: &mut World,
    machine: Entity,
    user: Entity,
    material: usize,
    amount: f64,
    store: Option<Entity>,
    linked: bool,
) {
    if !amount.is_finite() || amount < 1.0 {
        return;
    }
    let sheet_type = world
        .resource::<ResearchDb>()
        .materials
        .get(material)
        .and_then(|m| m.sheet_type.clone());
    let (Some(sheet_type), Some(&pos)) = (sheet_type, world.get::<TilePos>(machine)) else {
        return;
    };
    // Permission and the sheets actually there come before any energy is drawn, and the
    // energy is for those sheets: tg charges for the requested amount first, which lets
    // anyone drain the powernet with a huge `amount`.
    if !silo_permitted(world, machine, user, store, linked) {
        return;
    }
    let Some(store) = store else {
        return;
    };
    let have = world
        .get::<MaterialStore>(store)
        .and_then(|m| m.amounts.get(material).copied())
        .unwrap_or(0);
    let sheets = amount.min(f64::from(have / SHEET_MATERIAL_AMOUNT as u32)) as u32;
    if sheets == 0 {
        return;
    }
    let active = world
        .get::<DmPower>(machine)
        .map_or(0.0, |p| p.active_power_usage);
    let charge = (f64::from(sheets) / f64::from(MAX_STACK_SIZE) * 0.4 * active).ceil();
    let powered = world.get::<PowerMachine>(machine).is_none()
        || charge <= 0.0
        || crate::kitchen::machine_use_energy(world, machine, charge) >= charge;
    if !powered {
        machine_say(world, machine, "No power to dispense sheets");
        return;
    }
    let Some(mut materials) = world.get_mut::<MaterialStore>(store) else {
        return;
    };
    let Some(have) = materials.amounts.get_mut(material) else {
        return;
    };
    *have -= sheets * SHEET_MATERIAL_AMOUNT as u32;
    crate::construction::spawn_stack(world, &sheet_type, pos, sheets);
}

/// tg `remote_materials.on_item_insert` / `attempt_insert` of a held stack. True when the click
/// was the lathe's.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<Lathe>(target).is_none() {
        return false;
    }
    let Some(path) = world
        .get::<ss13_mapload::DmItem>(held)
        .map(|i| i.path.clone())
    else {
        return false;
    };
    if world.get::<Stack>(held).is_none() {
        return false;
    }
    if world.get::<CombatMode>(mob).is_some_and(|c| c.0) {
        return false;
    }
    let store = store_of(world, target);
    insert_stack(world, client, target, store, held, &path);
    true
}

/// `remote_materials` `user_insert` of a held stack into `store`, for any machine.
pub(crate) fn insert_stack(
    world: &mut World,
    client: Entity,
    lathe: Entity,
    store: Option<Entity>,
    held: Entity,
    path: &str,
) {
    let lathe_name = world
        .get::<DmName>(lathe)
        .map(|n| plain(&n.name).to_owned())
        .unwrap_or_default();
    let singular = world
        .get::<DmStack>(held)
        .map(|s| s.singular_name.clone())
        .filter(|s| !s.is_empty())
        .or_else(|| world.get::<DmName>(held).map(|n| n.name.clone()))
        .unwrap_or_default();
    let per_unit = world
        .resource::<ResearchDb>()
        .stacks
        .get(path)
        .cloned()
        .unwrap_or_default();
    if per_unit.is_empty() {
        tell(
            world,
            client,
            Span::Warning,
            format!("The {singular} has no materials that can be accepted by {lathe_name}!"),
        );
        return;
    }
    let Some(store) = store else {
        return;
    };
    let amount = crate::tools::stack_amount(world, held);
    let taken = world
        .get_mut::<MaterialStore>(store)
        .map_or(0, |mut s| s.insert_stack(&per_unit, amount));
    if taken == 0 {
        tell(
            world,
            client,
            Span::Warning,
            format!("{lathe_name} has no space to accept the {singular}!"),
        );
        return;
    }
    crate::tools::use_stack(world, held, taken);
    let text = if taken > 1 {
        format!("{taken} {singular}s were added to {lathe_name}.")
    } else {
        format!("The {singular} was added to {lathe_name}.")
    };
    tell(world, client, Span::Notice, text);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(imprinter: bool, servos: u32) -> LatheDef {
        LatheDef {
            imprinter,
            servos,
            matter_bins: 2,
            ..default()
        }
    }

    /// A huge `remove_mat` amount draws no energy without permission or sheets to eject.
    #[test]
    fn eject_draws_no_energy_for_sheets_not_there() {
        use crate::power::{Apc, AreaPower, Cell, Channel, Grid};
        let mut world = World::new();
        world.init_resource::<Grid>();
        world.init_resource::<Messages<ss13_net::chat::server::SendChat>>();
        world.insert_resource(ResearchDb {
            materials: vec![ss13_mapload::research::MaterialDef {
                id: "/datum/material/iron".into(),
                name: "iron".into(),
                sheet_type: Some("/obj/item/stack/sheet/iron".into()),
                color: None,
            }],
            ..default()
        });
        let area = world.spawn_empty().id();
        let cell = Cell {
            charge: 1e6,
            maxcharge: 1e6,
            chargerate: 0.0,
        };
        let apc = world.spawn(Apc::new(area, Some(cell))).id();
        world.entity_mut(area).insert(AreaPower {
            requires_power: true,
            apc: Some(apc),
            ..default()
        });
        let machine = world
            .spawn((
                TilePos::new(1, 1, 1),
                DmPower {
                    active_power_usage: 1000.0,
                    ..default()
                },
                PowerMachine {
                    channel: Channel::Equip,
                    area,
                    watts: 0.0,
                },
            ))
            .id();
        let store = world.spawn(MaterialStore::new(1, None)).id();
        let user = world.spawn_empty().id();
        let charge = |world: &World| world.get::<Apc>(apc).unwrap().cell.unwrap().charge;
        // An empty store, then a linked silo refusing a user without an ID.
        eject_sheets(&mut world, machine, user, 0, 1e12, Some(store), false);
        assert_eq!(charge(&world), 1e6);
        world.get_mut::<MaterialStore>(store).unwrap().amounts[0] =
            10 * SHEET_MATERIAL_AMOUNT as u32;
        eject_sheets(&mut world, machine, user, 0, 1e12, Some(store), true);
        assert_eq!(charge(&world), 1e6);
        assert_eq!(
            world.get::<MaterialStore>(store).unwrap().amounts[0],
            10 * SHEET_MATERIAL_AMOUNT as u32
        );
    }

    #[test]
    fn tier_one_efficiency_and_build_time() {
        assert!(
            (efficiency(&def(false, 2)) - 1.0).abs() < 1e-9,
            "protolathe"
        );
        assert_eq!(efficiency(&def(true, 1)), 1.0, "imprinter");
        assert_eq!(efficiency(&def(true, 3)), 0.25);
        // Two tier 4 servos (RefreshParts).
        assert!((efficiency_for(false, 8) - 0.4).abs() < 1e-9);
        assert_eq!(efficiency_for(true, 8), 0.5_f64.powi(7));
        // 3.2 s designs: 32 ** 0.8 = 16 ds.
        assert!((build_secs(32.0, 1.0, 1.0) - 1.6).abs() < 1e-9);
    }

    #[test]
    fn material_costs_follow_optimal_cost() {
        let mut store = MaterialStore::new(2, Some(7500));
        assert_eq!(store.insert_stack(&[(0, 100.0)], 10), 10);
        assert_eq!(store.amounts, vec![1000, 0]);
        // 0.4 units at 1.0 still costs 1 (`OPTIMAL_COST`), times 3 prints.
        let cost = [(0, 0.4)];
        assert!(store.has_materials(&cost, 1.0, 3));
        store.use_materials(&cost, 1.0, 3);
        assert_eq!(store.amounts[0], 997);
        assert!(
            !store.has_materials(&[(0, 500.0), (1, 1.0)], 1.0, 1),
            "no glass"
        );
        assert!(
            !store.has_materials(&[], 1.0, 1),
            "an empty cost is not printable"
        );
        // 50 * 0.5 efficiency = 25 each.
        assert!(store.has_materials(&[(0, 50.0)], 0.5, 39));
        assert!(!store.has_materials(&[(0, 50.0)], 0.5, 40));
    }

    #[test]
    fn stacks_split_when_storage_is_full() {
        let mut store = MaterialStore::new(1, Some(7500));
        assert_eq!(store.insert_stack(&[(0, 100.0)], 50), 50);
        assert_eq!(store.insert_stack(&[(0, 100.0)], 50), 25, "75 sheets fit");
        assert_eq!(store.insert_stack(&[(0, 100.0)], 5), 0);
        let mut silo = MaterialStore::new(1, None);
        assert_eq!(
            silo.insert_stack(&[(0, 100.0)], 500),
            500,
            "the silo has no limit"
        );
    }

    #[test]
    fn colors_parse() {
        assert_eq!(parse_color("#B6BEC2"), Some([0xB6, 0xBE, 0xC2, 255]));
        assert_eq!(parse_color("red"), None);
    }
}
