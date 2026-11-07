//! The E.X.P.E.R.I-MENTOR (server), after tg `/obj/machinery/rnd/experimentor`
//! (`experimentor/experimentor.dm`, `experiments.dm`) with tgui's `Experimentor` window.
//!
//! A click with an item out of combat mode puts it in the chamber; a click with an empty hand
//! opens the window. Each item type gets one random reaction (`item_reactions`; a relic always
//! `Discover`). `experiment` with the matching scan type runs its handler (and unhides one node
//! the item's type unlocks); any other scan fails. A relic's `Discover` reveals it
//! ([`super::relics::reveal`]) and ejects it. Handlers announce themselves, may go critical on
//! critical items (RCDs, grenades, AI cards, slime extracts, TTVs) and otherwise roll their
//! malfunctions, scaled by `(100 - malfunction_probability_coeff) / 100`. After each experiment
//! the machine may have a global malfunction (`handle_global_reactions`). The cooldown is
//! `2 s / servo tier` (0.5..2 s), the coefficient `2 × scanner tiers + laser tiers` from the parts.
// Not tg: tg adds the part tiers to `malfunction_probability_coeff` on every `RefreshParts` without
// resetting it (a bug that drives malfunctions to zero after a few part swaps); it is computed from
// the current parts here.
// shortcut: no wires, panel, Ian/Runtime theft (a new corgi or cat spawns), suspicious coffee,
// space-time throws, thrown items (they eject), sounds, the loading flick or admin logs.

use std::collections::HashMap;

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::{DmItem, DmName, DmObj};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, TilePos};
use ss13_tgui::UiInterface as _;
use ss13_tgui::UiStatus;
use ss13_tgui::interfaces::vending::ProductIcon;
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};
use ss13_world::{SpatialIndex, WorldSystems};

use super::relics::{Relic, is_relic};
use super::techweb::{ResearchDb, Techweb};
use super::*;
use crate::click::{ClickResolved, ResolveClicks};
use crate::combat::{CombatMode, CombatRng};
use crate::effects::smoke::{SmokeKind, do_smoke};
use crate::health::{DamageType, Health, Stat};
use crate::inventory::{CarriedBy, Inventory};
use crate::obj_damage::is_under;
use crate::power::{PowerMachine, Unpowered};
use crate::stock_parts::PartKind;
use crate::storage::{put_inside, take_out};
use crate::tools::tell;
use crate::zone::BodyZone;

pub const EXPERIMENTOR_ROOT: &str = "/obj/machinery/rnd/experimentor";
/// `EXPERIMENT_COOLDOWN_BASE`, seconds.
const COOLDOWN_BASE: f64 = 2.0;
/// `EFFECT_PROBABILITY` and `EFFECT_PROB_*`.
const EFFECT_PROBABILITY: f64 = 20.0;
const PROB_VERYLOW: f64 = 20.0;
const PROB_LOW: f64 = 35.0;
const PROB_MEDIUM: f64 = 50.0;
/// `use_energy(750 JOULES)` per experiment; the global power drain's 500 kJ.
const EXPERIMENT_ENERGY: f64 = 750.0;
const DRAIN_ENERGY: f64 = 500_000.0;
const PROB_VERYHIGH: f64 = 95.0;
/// `do_chem_smoke`'s 50 units.
const CHEM_SMOKE_UNITS: f32 = 50.0;
/// The hot and cold air malfunctions' energy, joules.
const HOT_AIR_ENERGY: f64 = 100_000.0;
const COLD_AIR_ENERGY: f64 = -75_000.0;
/// `TCMB`.
const TCMB: f64 = 2.7;
const GREENGLOW: &str = "/obj/effect/decal/cleanable/greenglow/filled";
const GREENGLOW_ROOT: &str = "/obj/effect/decal/cleanable/greenglow";
const FROSTOIL: &str = "/datum/reagent/consumable/frostoil";
/// The gas malfunction's dangerous chemicals and the leak's.
const GAS_CHEMS: [&str; 8] = [
    "/datum/reagent/carbon",
    "/datum/reagent/uranium/radium",
    "/datum/reagent/toxin",
    "/datum/reagent/consumable/condensedcapsaicin",
    "/datum/reagent/drug/mushroomhallucinogen",
    "/datum/reagent/drug/space_drugs",
    "/datum/reagent/consumable/ethanol",
    "/datum/reagent/consumable/ethanol/beepsky_smash",
];
const LEAK_CHEMS: [&str; 3] = [
    "/datum/reagent/mutationtoxin/classic",
    "/datum/reagent/cyborg_mutation_nanomachines",
    "/datum/reagent/toxin/acid",
];
/// `valid_items`: stock parts and knives at 15, food at 1-4 (here 2); spawnable at runtime
/// (`ss13_mapload::runtime::GAME_RUNTIME_TYPES`).
// shortcut: a fixed list instead of every `/obj/item` subtype, no chem grenades (tg detonates
// them on the spot).
const VALID_ITEMS: [(&str, u32); 8] = [
    ("/obj/item/stock_parts/servo", 15),
    ("/obj/item/stock_parts/capacitor", 15),
    ("/obj/item/stock_parts/scanning_module", 15),
    ("/obj/item/stock_parts/micro_laser", 15),
    ("/obj/item/stock_parts/matter_bin", 15),
    ("/obj/item/knife/kitchen", 15),
    ("/obj/item/food/donut/plain", 2),
    ("/obj/item/food/burger/plain", 2),
];
/// The `boom`/`honk` timer, seconds.
const BLOW_DELAY: f64 = 5.0;

/// `critical_items_typecache`.
const CRITICAL_ITEMS: [&str; 5] = [
    "/obj/item/construction/rcd",
    "/obj/item/grenade",
    "/obj/item/aicard",
    "/obj/item/slime_extract",
    "/obj/item/transfer_valve",
];
/// `banned_typecache`: no reaction is generated for these.
const BANNED_ITEMS: [&str; 3] = [
    "/obj/item/stock_parts/power_store/cell/infinite",
    "/obj/item/stock_parts/power_store/cell/ethereal",
    "/obj/item/grenade/chem_grenade/tuberculosis",
];
/// What the handlers and malfunctions spawn (`ss13_mapload::runtime::GAME_RUNTIME_TYPES`).
const PLASMA: &str = "/obj/item/stack/sheet/mineral/plasma";
const PLASTEEL: &str = "/obj/item/stack/sheet/plasteel";
const BANANA_PEEL: &str = "/obj/item/grown/bananapeel";
const CORGI: &str = "/mob/living/basic/pet/dog/corgi";
const CAT: &str = "/mob/living/basic/pet/cat";

/// `SCANTYPE_*` and `FAIL`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Scan {
    Poke,
    Irradiate,
    Gas,
    Heat,
    Cold,
    Obliterate,
    Discover,
    Fail,
}

/// The scan handlers in `experimentor_result_handlers` order, without `Fail`.
pub const SCANS: [Scan; 7] = [
    Scan::Poke,
    Scan::Irradiate,
    Scan::Gas,
    Scan::Heat,
    Scan::Cold,
    Scan::Obliterate,
    Scan::Discover,
];

impl Scan {
    pub fn id(self) -> &'static str {
        match self {
            Self::Poke => "Poke",
            Self::Irradiate => "Irradiate",
            Self::Gas => "Gas",
            Self::Heat => "Heat",
            Self::Cold => "Freeze",
            Self::Obliterate => "Obliterate",
            Self::Discover => "Discover",
            Self::Fail => "Fail",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        SCANS.into_iter().find(|s| s.id() == id)
    }

    fn fa_icon(self) -> &'static str {
        match self {
            Self::Poke => "hand",
            Self::Irradiate => "radiation",
            Self::Gas => "cloud",
            Self::Heat => "fire",
            Self::Cold => "snowflake",
            Self::Obliterate => "trash",
            Self::Discover | Self::Fail => "question",
        }
    }

    /// `start_message_template` and `start_message_type`.
    fn start(self) -> (&'static str, Span) {
        match self {
            Self::Poke => ("prods at %ITEM% with mechanical arms.", Span::Notice),
            Self::Irradiate => ("reflects radioactive rays at %ITEM%!", Span::Danger),
            Self::Gas => (
                "fills its chamber with gas, %ITEM% included.",
                Span::Warning,
            ),
            Self::Heat => ("raises %ITEM%'s temperature.", Span::Notice),
            Self::Cold => ("lowers %ITEM%'s temperature.", Span::Notice),
            Self::Obliterate => (
                "activates the crushing mechanism, %ITEM% is destroyed!",
                Span::Warning,
            ),
            Self::Discover => ("scans the %ITEM%, revealing its true nature!", Span::Notice),
            Self::Fail => ("", Span::Warning),
        }
    }

    /// `critical_prob` and `critical_message_template`.
    fn critical(self) -> (f64, &'static str) {
        match self {
            Self::Poke => (
                PROB_LOW,
                "%ITEM% is gripped in just the right way, enhancing its focus.",
            ),
            Self::Irradiate => (PROB_VERYLOW, "%ITEM% has activated an unknown subroutine!"),
            Self::Gas => (PROB_LOW, "%ITEM% achieves the perfect mix!"),
            Self::Heat | Self::Cold => (
                PROB_LOW,
                "%ITEM%'s emergency coolant system gives off a small ding!",
            ),
            Self::Obliterate => (
                PROB_LOW,
                "%ITEM%'s crushing mechanism slowly and smoothly descends, flattening the %ITEM%!",
            ),
            Self::Discover | Self::Fail => (0.0, ""),
        }
    }
}

#[derive(Component, Clone, Debug, Default)]
pub struct Experimentor {
    pub loaded: Option<Entity>,
    /// Game time `run_experiment`'s cooldown ends.
    pub ready_at: f64,
    /// `item_reactions`: item type -> its reaction.
    pub item_reactions: HashMap<String, Scan>,
    pub critical_malfunction_counter: u32,
    /// A pending `boom` (false) or `honk` (true) and its game time.
    pub pending_blow: Option<(f64, bool)>,
}

pub struct ExperimentorPlugin;

impl Plugin for ExperimentorPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<ExperimentorUi>()
            .add_systems(ss13_world::MapInit, setup_experimentors)
            .add_systems(
                Update,
                (
                    open_experimentor
                        .after(WorldSystems::Movement)
                        .after(ResolveClicks)
                        .before(UiSystems::Open),
                    experimentor_acts.in_set(UiSystems::Handle),
                    unpowered_windows.in_set(UiSystems::StatusHook),
                    blow_timers,
                ),
            );
    }
}

fn setup_experimentors(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &DmObj), With<ss13_world::Mapped>>();
    let found: Vec<Entity> = query
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, EXPERIMENTOR_ROOT))
        .map(|(e, _)| e)
        .collect();
    for e in found {
        setup(world, e);
    }
}

/// An experimentor's state (map load and `crate::machine_frame`).
pub(crate) fn setup(world: &mut World, e: Entity) {
    world.entity_mut(e).insert(Experimentor::default());
}

use crate::tools::now;

fn rng(world: &mut World) -> CombatRng {
    CombatRng(world.get_resource::<CombatRng>().map_or(0, |r| r.0))
}

fn put_rng(world: &mut World, rng: CombatRng) {
    if let Some(mut r) = world.get_resource_mut::<CombatRng>() {
        r.0 = rng.0;
    }
}

fn prob(world: &mut World, percent: f64) -> bool {
    let mut r = rng(world);
    let hit = r.chance(percent as f32);
    put_rng(world, r);
    hit
}

fn rand_range(world: &mut World, low: u32, high: u32) -> u32 {
    let mut r = rng(world);
    let n = r.range(low, high);
    put_rng(world, r);
    n
}

fn open_experimentor(
    mut clicks: MessageReader<ClickResolved>,
    machines: Query<(), With<Experimentor>>,
    inventories: Query<&Inventory>,
    mut open: MessageWriter<OpenUi>,
) {
    for click in clicks.read() {
        if machines.contains(click.target)
            && inventories
                .get(click.mob)
                .is_ok_and(|i| i.active_item().is_none())
        {
            open.write(OpenUi::new::<ExperimentorUi>(
                click.client,
                click.mob,
                click.target,
                "E.X.P.E.R.I-MENTOR",
            ));
        }
    }
}

fn unpowered_windows(
    mut windows: Query<&mut UiWindow>,
    unpowered: Query<(), (With<Experimentor>, With<Unpowered>)>,
) {
    for mut window in &mut windows {
        if window.interface == ExperimentorUi::NAME && unpowered.contains(window.source) {
            window.cap_status(UiStatus::Update);
        }
    }
}

type ExperimentorParams = (
    Option<Res<'static, ResearchDb>>,
    Option<Res<'static, Techweb>>,
    Res<'static, Time>,
    Query<
        'static,
        'static,
        (
            &'static DmItem,
            Option<&'static DmName>,
            Option<&'static Appearance>,
            Option<&'static Relic>,
        ),
    >,
);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExperimentorAct {
    Eject,
    Experiment(String),
}

impl ServerUi for ExperimentorUi {
    type Source = &'static Experimentor;
    type UserParam = ExperimentorParams;
    type Act = ExperimentorAct;

    fn static_data(_: &Experimentor) {}

    fn data(
        machine: &Experimentor,
        _: Entity,
        (db, web, time, items): &SystemParamItem<'_, '_, ExperimentorParams>,
    ) -> ExperimentorData {
        let mut data = ExperimentorData {
            has_item: machine.loaded.is_some(),
            is_on_cooldown: time.elapsed_secs_f64() < machine.ready_at,
            is_server_connected: web.is_some(),
            ..default()
        };
        let Some((item, name, appearance, relic)) = machine.loaded.and_then(|e| items.get(e).ok())
        else {
            return data;
        };
        let is_relic = relic.is_some() || is_relic(&item.path);
        data.available_experiments = SCANS
            .iter()
            .map(|&scan| {
                let is_discover = scan == Scan::Discover;
                ExperimentorOption {
                    id: scan.id().to_owned(),
                    name: scan.id().to_owned(),
                    fa_icon: scan.fa_icon().to_owned(),
                    is_available: is_relic == is_discover,
                    is_discover,
                }
            })
            .collect();
        let associated_nodes = match (db.as_deref(), web.as_deref()) {
            (Some(db), Some(web)) => db
                .unlock_items
                .get(&item.path)
                .into_iter()
                .flatten()
                .map(|&n| (db.nodes[n].name.clone(), !web.hidden.contains(&n)))
                .collect(),
            _ => Vec::new(),
        };
        data.loaded_item = Some(ExperimentorItem {
            name: name.map_or_else(|| item.path.clone(), |n| n.name.clone()),
            icon: appearance.map(|a| ProductIcon {
                icon_state: a.icon_state,
                color: a.color,
            }),
            is_relic,
            associated_nodes,
        });
        data
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<ExperimentorAct, UiReject> {
        match action {
            EXPERIMENTOR_EJECT => Ok(ExperimentorAct::Eject),
            EXPERIMENTOR_EXPERIMENT => params
                .get(EXPERIMENTOR_ID)
                .map(|id| ExperimentorAct::Experiment(id.to_owned()))
                .ok_or_else(|| UiReject::BadParams("id: missing".into())),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

fn experimentor_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<ExperimentorUi>>>) {
    let pending: Vec<(Entity, ExperimentorAct)> = acts
        .read(world.resource::<Messages<UiActed<ExperimentorUi>>>())
        .map(|a| (a.source, a.act.clone()))
        .collect();
    for (machine, act) in pending {
        if world.get::<Unpowered>(machine).is_some() || world.get::<Experimentor>(machine).is_none()
        {
            continue;
        }
        match act {
            ExperimentorAct::Eject => item_eject(world, machine, false),
            ExperimentorAct::Experiment(id) => {
                try_perform_experiment(world, machine, &id);
            }
        }
    }
}

fn item_name(world: &World, item: Entity) -> String {
    world
        .get::<DmName>(item)
        .map(|n| n.name.clone())
        .unwrap_or_default()
}

fn path_of(world: &World, item: Entity) -> Option<String> {
    world.get::<DmObj>(item).map(|o| o.path.clone())
}

/// `visible_message` of the machine.
fn say(world: &mut World, machine: Entity, span: Span, text: &str) {
    crate::medical::visible(world, machine, "", text, span);
}

/// `"[src] [message]"`.
fn say_prefixed(world: &mut World, machine: Entity, span: Span, text: &str) {
    let name = item_name(world, machine);
    let name = name.trim_start_matches("\\improper ");
    say(world, machine, span, &format!("{name} {text}"));
}

/// tg `item_eject(should_clone)`: onto the tile east of the machine (its own tile at the map
/// edge); a clone leaves the item inside.
pub fn item_eject(world: &mut World, machine: Entity, should_clone: bool) {
    let Some(item) = world.get::<Experimentor>(machine).and_then(|m| m.loaded) else {
        return;
    };
    let Some(&pos) = world.get::<TilePos>(machine) else {
        return;
    };
    let east = TilePos::new(pos.x + 1, pos.y, pos.z);
    let drop = if world.resource::<SpatialIndex>().in_bounds(east) {
        east
    } else {
        pos
    };
    if world.get_entity(item).is_err() {
        if let Some(mut m) = world.get_mut::<Experimentor>(machine) {
            m.loaded = None;
        }
        return;
    }
    if should_clone {
        let name = item_name(world, item);
        say(
            world,
            machine,
            Span::Notice,
            &format!("A duplicate of the {name} pops out!"),
        );
        if let Some(path) = path_of(world, item) {
            crate::construction::spawn_obj(world, &path, drop);
        }
        return;
    }
    take_out(world, item, drop);
    crate::construction::tile_changed(world, drop);
    if let Some(mut m) = world.get_mut::<Experimentor>(machine) {
        m.loaded = None;
    }
}

/// `QDEL_NULL(machine.loaded_item)`.
fn destroy_loaded(world: &mut World, machine: Entity) {
    let Some(item) = world
        .get_mut::<Experimentor>(machine)
        .and_then(|mut m| m.loaded.take())
    else {
        return;
    };
    if world.get_entity(item).is_ok() {
        world.despawn(item);
    }
}

/// `try_generate_reaction_for_item`.
fn generate_reaction(world: &mut World, machine: Entity, item: Entity) {
    let Some(path) = path_of(world, item) else {
        return;
    };
    let known = world
        .get::<Experimentor>(machine)
        .is_some_and(|m| m.item_reactions.contains_key(&path));
    if known || BANNED_ITEMS.iter().any(|b| is_under(&path, b)) {
        return;
    }
    let reaction = if world.get::<Relic>(item).is_some() || is_relic(&path) {
        Scan::Discover
    } else {
        // `get_available_reactions`: every handler but the special ones.
        SCANS[rand_range(world, 0, SCANS.len() as u32 - 2) as usize]
    };
    if let Some(mut m) = world.get_mut::<Experimentor>(machine) {
        m.item_reactions.insert(path, reaction);
    }
}

/// The current parts: (`cooldown`, `malfunction_probability_coeff`).
fn part_values(world: &World, machine: Entity) -> (f64, f64) {
    let ratings = crate::stock_parts::ratings(world, machine);
    let cooldown = ratings.last(PartKind::Servo).map_or(COOLDOWN_BASE, |t| {
        (COOLDOWN_BASE / f64::from(t.max(1))).clamp(0.5, 2.0)
    });
    let coeff = f64::from(ratings.sum(PartKind::ScanningModule) * 2)
        + f64::from(ratings.sum(PartKind::MicroLaser));
    (cooldown, coeff)
}

/// `get_malfunction_chance`.
fn malfunction_chance(world: &World, machine: Entity) -> f64 {
    (100.0 - part_values(world, machine).1) * 0.01
}

/// tg `try_perform_experiment(reaction)`. True when an experiment ran.
pub fn try_perform_experiment(world: &mut World, machine: Entity, id: &str) -> bool {
    let Some(state) = world.get::<Experimentor>(machine) else {
        return false;
    };
    let Some(item) = state.loaded.filter(|&i| world.get_entity(i).is_ok()) else {
        return false;
    };
    if !world.contains_resource::<Techweb>() || now(world) < state.ready_at {
        return false;
    }
    let path = path_of(world, item).unwrap_or_default();
    let reaction = if world.get::<Relic>(item).is_some() || is_relic(&path) {
        Scan::Discover
    } else {
        // `match_reaction`.
        match (Scan::from_id(id), state.item_reactions.get(&path)) {
            (Some(asked), Some(&known)) if asked != Scan::Discover && asked == known => known,
            _ => Scan::Fail,
        }
    };
    if reaction != Scan::Fail {
        let nodes = world
            .get_resource::<ResearchDb>()
            .and_then(|db| db.unlock_items.get(&path).cloned())
            .unwrap_or_default();
        if !nodes.is_empty() {
            let node = nodes[rand_range(world, 0, nodes.len() as u32 - 1) as usize];
            world.resource_scope(|world, mut web: Mut<Techweb>| {
                web.unhide_node(world.resource::<ResearchDb>(), node);
            });
        }
    }
    run_experiment(world, machine, reaction);
    if world.get::<PowerMachine>(machine).is_some() {
        crate::kitchen::machine_use_energy(world, machine, EXPERIMENT_ENERGY);
    }
    true
}

/// tg `run_experiment(experiment_type)`.
fn run_experiment(world: &mut World, machine: Entity, scan: Scan) {
    let Some(item) = world.get::<Experimentor>(machine).and_then(|m| m.loaded) else {
        return;
    };
    execute(world, machine, item, scan);
    handle_global_reactions(world, machine);
    let ready = now(world) + part_values(world, machine).0;
    if let Some(mut m) = world.get_mut::<Experimentor>(machine) {
        m.ready_at = ready;
    }
}

/// `/datum/experimentor_result_handler/proc/execute`.
fn execute(world: &mut World, machine: Entity, item: Entity, scan: Scan) {
    let name = item_name(world, item);
    match scan {
        Scan::Fail => {
            let a = ["rumbles", "shakes", "vibrates", "shudders", "honks"];
            let b = ["crushes", "spins", "viscerates", "smashes", "insults"];
            let a = a[rand_range(world, 0, 4) as usize];
            let b = b[rand_range(world, 0, 4) as usize];
            say(
                world,
                machine,
                Span::Warning,
                &format!("{name} {a}, and {b}, the experiment was a failure."),
            );
            return;
        }
        Scan::Discover => {
            let (text, span) = scan.start();
            say_prefixed(world, machine, span, &text.replace("%ITEM%", &name));
            super::relics::reveal(world, item);
            item_eject(world, machine, false);
            return;
        }
        _ => {}
    }
    let (text, span) = scan.start();
    say_prefixed(world, machine, span, &text.replace("%ITEM%", &name));
    let path = path_of(world, item).unwrap_or_default();
    let critical = CRITICAL_ITEMS.iter().any(|c| is_under(&path, c));
    let (critical_prob, critical_text) = scan.critical();
    if critical && prob(world, critical_prob) {
        say(
            world,
            machine,
            Span::Notice,
            &critical_text.replace("%ITEM%", &name),
        );
        handle_critical(world, machine, scan);
        return;
    }
    handle_malfunctions(world, machine, scan, &name);
}

/// A tile in `oview(1, machine)`.
fn adjacent_tile(world: &mut World, machine: Entity) -> Option<TilePos> {
    let pos = *world.get::<TilePos>(machine)?;
    let tiles: Vec<TilePos> = (-1..=1)
        .flat_map(|dy| (-1..=1).map(move |dx| (dx, dy)))
        .filter(|&d| d != (0, 0))
        .map(|(dx, dy)| TilePos::new(pos.x + dx, pos.y + dy, pos.z))
        .filter(|&p| world.resource::<SpatialIndex>().in_bounds(p))
        .collect();
    if tiles.is_empty() {
        return None;
    }
    Some(tiles[rand_range(world, 0, tiles.len() as u32 - 1) as usize])
}

/// The handlers' `handle_critical`.
fn handle_critical(world: &mut World, machine: Entity, scan: Scan) {
    match scan {
        Scan::Poke => {
            if let Some(mut m) = world.get_mut::<Experimentor>(machine) {
                m.critical_malfunction_counter += 1;
            }
        }
        Scan::Irradiate => item_eject(world, machine, true),
        Scan::Gas | Scan::Obliterate => {
            let path = if scan == Scan::Gas { PLASMA } else { PLASTEEL };
            if let Some(pos) = adjacent_tile(world, machine) {
                crate::construction::spawn_obj(world, path, pos);
            }
            if scan == Scan::Obliterate {
                destroy_loaded(world, machine);
            }
        }
        // shortcut: the cup of suspicious coffee is not made.
        _ => {}
    }
}

/// Living mobs within `range` tiles of `pos` (`oview`/`view` by distance).
fn mobs_near(world: &World, pos: TilePos, range: i32, skip_center: bool) -> Vec<Entity> {
    let index = world.resource::<SpatialIndex>();
    let mut out = Vec::new();
    for dy in -range..=range {
        for dx in -range..=range {
            if skip_center && dx == 0 && dy == 0 {
                continue;
            }
            let p = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
            out.extend(
                index.on_tile(p).iter().copied().filter(|&e| {
                    world.get::<Health>(e).is_some() && world.get::<Stat>(e).is_some()
                }),
            );
        }
    }
    out
}

fn damage(world: &mut World, mob: Entity, zone: BodyZone, kind: DamageType, amount: f32) {
    let (Some(mut health), Some(&stat)) =
        (world.get::<Health>(mob).cloned(), world.get::<Stat>(mob))
    else {
        return;
    };
    let mut new = stat;
    crate::health::apply_damage_zone(&mut health, &mut new, zone, kind, amount);
    world.entity_mut(mob).insert(health);
    if new != stat {
        world.entity_mut(mob).insert(new);
    }
}

fn head_or_chest(world: &mut World) -> BodyZone {
    if rand_range(world, 0, 1) == 0 {
        BodyZone::Head
    } else {
        BodyZone::Chest
    }
}

/// The handlers' `handle_malfunctions`.
fn handle_malfunctions(world: &mut World, machine: Entity, scan: Scan, name: &str) {
    let chance = malfunction_chance(world, machine);
    let machine_name = item_name(world, machine);
    let machine_name = machine_name.trim_start_matches("\\improper ").to_owned();
    let Some(&pos) = world.get::<TilePos>(machine) else {
        return;
    };
    match scan {
        Scan::Poke => {
            if prob(world, PROB_VERYLOW * chance) {
                say(
                    world,
                    machine,
                    Span::Danger,
                    &format!(
                        "{machine_name} malfunctions and destroys {name}, lashing its arms out at \
                         nearby people!"
                    ),
                );
                for mob in mobs_near(world, pos, 1, true) {
                    let zone = head_or_chest(world);
                    damage(world, mob, zone, DamageType::Brute, 15.0);
                }
                destroy_loaded(world, machine);
            } else if prob(world, PROB_LOW * chance) {
                say(
                    world,
                    machine,
                    Span::Warning,
                    &format!("{machine_name} malfunctions!"),
                );
                run_experiment(world, machine, Scan::Obliterate);
            } else if prob(world, PROB_MEDIUM * chance) {
                say(
                    world,
                    machine,
                    Span::Danger,
                    &format!("{machine_name} malfunctions, throwing the {name}!"),
                );
                if !mobs_near(world, pos, 7, true).is_empty() {
                    item_eject(world, machine, false);
                }
            }
        }
        Scan::Irradiate => {
            if prob(world, PROB_VERYLOW * chance) {
                say(
                    world,
                    machine,
                    Span::Danger,
                    &format!("{machine_name} malfunctions, melting {name} and leaking radiation!"),
                );
                crate::radiation::radiation_pulse(
                    world,
                    machine,
                    6.0,
                    0.3,
                    crate::radiation::DEFAULT_RADIATION_CHANCE,
                    0.0,
                );
                destroy_loaded(world, machine);
            } else if prob(world, PROB_LOW * chance) {
                say(
                    world,
                    machine,
                    Span::Warning,
                    &format!("{machine_name} malfunctions, spewing toxic waste!"),
                );
                for tile in ring(world, pos) {
                    if !turf_dense(world, tile)
                        && prob(world, PROB_VERYHIGH)
                        && !has_on_tile(world, tile, GREENGLOW_ROOT)
                    {
                        crate::construction::spawn_obj(world, GREENGLOW, tile);
                    }
                }
                destroy_loaded(world, machine);
            } else if prob(world, PROB_MEDIUM * chance) {
                transform(world, machine, &machine_name, name, pos);
            }
        }
        Scan::Gas => {
            if prob(world, PROB_VERYLOW * chance) {
                say(
                    world,
                    machine,
                    Span::Danger,
                    &format!("{machine_name} destroys {name}, leaking dangerous gas!"),
                );
                let chem = GAS_CHEMS[rand_range(world, 0, GAS_CHEMS.len() as u32 - 1) as usize];
                chem_smoke(world, pos, chem);
                destroy_loaded(world, machine);
            } else if prob(world, PROB_VERYLOW * chance) {
                say(
                    world,
                    machine,
                    Span::Danger,
                    &format!("{machine_name}'s chemical chamber has sprung a leak!"),
                );
                let chem = LEAK_CHEMS[rand_range(world, 0, LEAK_CHEMS.len() as u32 - 1) as usize];
                chem_smoke(world, pos, chem);
                destroy_loaded(world, machine);
                warn!("experimentor: {chem} smoke at {pos:?}");
            } else if prob(world, PROB_LOW * chance) {
                say(
                    world,
                    machine,
                    Span::Warning,
                    &format!("{machine_name} malfunctions, spewing harmless gas."),
                );
                do_smoke(world, 1, pos, None, SmokeKind::Basic);
            } else if prob(world, PROB_MEDIUM * chance) {
                say(
                    world,
                    machine,
                    Span::Warning,
                    &format!("{machine_name} melts {name}, ionizing the air around it!"),
                );
                crate::emp::empulse(
                    world,
                    crate::emp::Empulse {
                        epicenter: pos,
                        heavy: 4,
                        light: 6,
                    },
                );
                destroy_loaded(world, machine);
            }
        }
        Scan::Heat => {
            if prob(world, PROB_VERYLOW * chance) {
                // `locate(/mob/living) in view(machine, 3)`.
                if let Some(target) = mobs_near(world, pos, 3, false).first().copied()
                    && let Some(&at) = world.get::<TilePos>(target)
                {
                    say(
                        world,
                        machine,
                        Span::Danger,
                        &format!(
                            "{machine_name} dangerously overheats, launching a flaming fuel orb!"
                        ),
                    );
                    crate::spells::effects::fire_bolt(
                        world,
                        machine,
                        pos,
                        Some(target),
                        at,
                        crate::spells::effects::BoltKind::Fireball,
                        1,
                    );
                }
            } else if prob(world, PROB_LOW * chance) {
                say(
                    world,
                    machine,
                    Span::Danger,
                    &format!(
                        "{machine_name} malfunctions, melting {name} and releasing a burst of \
                         flame!"
                    ),
                );
                destroy_loaded(world, machine);
                crate::explosions::explosion(
                    world,
                    pos,
                    crate::explosions::Explosion::new(-1, 0, 0).flame(2),
                );
            } else if prob(world, PROB_MEDIUM * chance) {
                say(
                    world,
                    machine,
                    Span::Warning,
                    &format!("{machine_name} malfunctions, melting {name} and leaking hot air!"),
                );
                change_air(world, pos, HOT_AIR_ENERGY);
                destroy_loaded(world, machine);
            } else if prob(world, PROB_MEDIUM * chance) {
                say(
                    world,
                    machine,
                    Span::Warning,
                    &format!(
                        "{machine_name} malfunctions, activating its emergency coolant systems!"
                    ),
                );
                do_smoke(world, 1, pos, None, SmokeKind::Basic);
                for mob in mobs_near(world, pos, 1, true) {
                    let zone = head_or_chest(world);
                    damage(world, mob, zone, DamageType::Burn, 5.0);
                }
                item_eject(world, machine, false);
            }
        }
        Scan::Cold => {
            if prob(world, PROB_VERYLOW * chance) {
                say(
                    world,
                    machine,
                    Span::Danger,
                    &format!(
                        "{machine_name} malfunctions, shattering {name} and releasing a \
                         dangerous cloud of coolant!"
                    ),
                );
                chem_smoke(world, pos, FROSTOIL);
                destroy_loaded(world, machine);
            } else if prob(world, PROB_LOW * chance) {
                say(
                    world,
                    machine,
                    Span::Warning,
                    &format!(
                        "{machine_name} malfunctions, shattering {name} and leaking cold air!"
                    ),
                );
                change_air(world, pos, COLD_AIR_ENERGY);
                destroy_loaded(world, machine);
            } else if prob(world, PROB_MEDIUM * chance) {
                say(
                    world,
                    machine,
                    Span::Warning,
                    &format!(
                        "{machine_name} malfunctions, releasing a flurry of chilly air as {name} \
                         pops out!"
                    ),
                );
                do_smoke(world, 1, pos, None, SmokeKind::Basic);
                item_eject(world, machine, false);
            }
        }
        Scan::Obliterate => destroy_loaded(world, machine),
        Scan::Discover | Scan::Fail => {}
    }
}

/// `do_chem_smoke(0, machine, machine.loc, chem, 50)`.
fn chem_smoke(world: &mut World, pos: TilePos, reagent: &str) {
    let Some(chem) = world
        .get_resource::<crate::chem::Chem>()
        .map(|c| c.0.clone())
    else {
        return;
    };
    let Some(id) = chem.id(reagent) else {
        do_smoke(world, 0, pos, None, SmokeKind::Bad);
        return;
    };
    let mut carry =
        ss13_reagents::Holder::new(CHEM_SMOKE_UNITS, 0, ss13_reagents::Owner::default());
    carry.add_reagent_list(&chem, &[(id, CHEM_SMOKE_UNITS)], None, true);
    crate::effects::smoke::do_chem_smoke(world, 0, pos, None, &carry, SmokeKind::Chem);
}

/// The hot (`+100 kJ`, at most 1000 K) and cold (`-75 kJ`, at least `TCMB`) air of the machine's
/// turf.
fn change_air(world: &mut World, pos: TilePos, energy: f64) {
    let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() else {
        return;
    };
    atmos.modify_air(pos, |a| {
        let capacity = a.heat_capacity().max(1.0);
        let t = (a.temperature * capacity + energy) / capacity;
        a.temperature = if energy > 0.0 {
            t.min(1000.0)
        } else {
            t.max(TCMB)
        };
    });
}

/// The tiles of `oview(1, machine)` in bounds.
fn ring(world: &World, pos: TilePos) -> Vec<TilePos> {
    let index = world.resource::<SpatialIndex>();
    (-1..=1)
        .flat_map(|dy| (-1..=1).map(move |dx| (dx, dy)))
        .filter(|&d| d != (0, 0))
        .map(|(dx, dy)| TilePos::new(pos.x + dx, pos.y + dy, pos.z))
        .filter(|&p| index.in_bounds(p))
        .collect()
}

/// `T.density` of the turf.
fn turf_dense(world: &World, pos: TilePos) -> bool {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| {
            world
                .get::<DmObj>(e)
                .is_some_and(|o| is_under(&o.path, "/turf/closed"))
        })
}

fn has_on_tile(world: &World, pos: TilePos, root: &str) -> bool {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| {
            world
                .get::<DmObj>(e)
                .is_some_and(|o| is_under(&o.path, root))
        })
}

/// The irradiate malfunction's transformation: the item becomes a random `valid_items` one,
/// which pops out.
fn transform(world: &mut World, machine: Entity, machine_name: &str, name: &str, pos: TilePos) {
    let total: u32 = VALID_ITEMS.iter().map(|(_, w)| w).sum();
    let mut roll = rand_range(world, 1, total);
    let mut path = VALID_ITEMS[0].0;
    for (p, w) in VALID_ITEMS {
        if roll <= w {
            path = p;
            break;
        }
        roll -= w;
    }
    destroy_loaded(world, machine);
    let Some(new) = crate::construction::spawn_obj(world, path, pos) else {
        return;
    };
    put_inside(world, new, machine);
    if let Some(mut m) = world.get_mut::<Experimentor>(machine) {
        m.loaded = Some(new);
    }
    let new_name = item_name(world, new);
    say(
        world,
        machine,
        Span::Warning,
        &format!("{machine_name} malfunctions, transforming {name} into {new_name}!"),
    );
    item_eject(world, machine, false);
}

/// tg `handle_global_reactions`.
fn handle_global_reactions(world: &mut World, machine: Entity) {
    let chance = malfunction_chance(world, machine);
    let Some(item) = world.get::<Experimentor>(machine).and_then(|m| m.loaded) else {
        return;
    };
    if world.get_entity(item).is_err() || !prob(world, EFFECT_PROBABILITY * chance) {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(machine) else {
        return;
    };
    let machine_name = item_name(world, machine);
    let machine_name = machine_name.trim_start_matches("\\improper ").to_owned();
    let name = item_name(world, item);
    match rand_range(world, 1, 100) {
        1..=15 => {
            say(
                world,
                machine,
                Span::Warning,
                &format!("{machine_name}'s onboard detection system has malfunctioned!"),
            );
            if let Some(path) = path_of(world, item) {
                let reaction = SCANS[rand_range(world, 0, SCANS.len() as u32 - 2) as usize];
                if let Some(mut m) = world.get_mut::<Experimentor>(machine) {
                    m.item_reactions.insert(path, reaction);
                }
            }
            item_eject(world, machine, false);
        }
        16..=35 => {
            say(
                world,
                machine,
                Span::Warning,
                &format!("{machine_name} melts {name}, ian-izing the air around it!"),
            );
            do_smoke(world, 1, pos, None, SmokeKind::Basic);
            crate::mobs::spawn::spawn_mob(world, CORGI, pos);
        }
        36..=50 => {
            say(
                world,
                machine,
                Span::Warning,
                "Experimentor draws the life essence of those nearby!",
            );
            for mob in mobs_near(world, pos, 4, false) {
                if let Some(client) = world.get::<crate::server::PlayerMob>(mob).map(|p| p.client) {
                    tell(
                        world,
                        client,
                        Span::Danger,
                        format!(
                            "You feel your flesh being torn from you, mists of blood drifting to \
                             {machine_name}!"
                        ),
                    );
                }
                damage(world, mob, BodyZone::Chest, DamageType::Brute, 50.0);
            }
        }
        51..=75 => {
            say(
                world,
                machine,
                Span::Warning,
                &format!("{machine_name} encounters a run-time error!"),
            );
            do_smoke(world, 1, pos, None, SmokeKind::Basic);
            crate::mobs::spawn::spawn_mob(world, CAT, pos);
        }
        76..=98 => {
            say(
                world,
                machine,
                Span::Warning,
                &format!("{machine_name} emits a low hum."),
            );
            crate::effects::particles::do_sparks(world, 3, false, pos);
            if world.get::<PowerMachine>(machine).is_some() {
                crate::kitchen::machine_use_energy(world, machine, DRAIN_ENERGY);
            }
        }
        n => {
            say(
                world,
                machine,
                Span::Warning,
                &format!("{machine_name} begins to glow and vibrate. It's going to blow!"),
            );
            let at = now(world) + BLOW_DELAY;
            if let Some(mut m) = world.get_mut::<Experimentor>(machine) {
                m.pending_blow = Some((at, n == 100));
            }
        }
    }
}

/// The `boom` and `honk` timers.
fn blow_timers(world: &mut World) {
    let t = now(world);
    let mut query = world.query::<(Entity, &Experimentor)>();
    let due: Vec<(Entity, bool)> = query
        .iter(world)
        .filter_map(|(e, m)| {
            m.pending_blow
                .filter(|(at, _)| *at <= t)
                .map(|(_, h)| (e, h))
        })
        .collect();
    for (machine, honk) in due {
        if let Some(mut m) = world.get_mut::<Experimentor>(machine) {
            m.pending_blow = None;
        }
        let Some(&pos) = world.get::<TilePos>(machine) else {
            continue;
        };
        if honk {
            crate::construction::spawn_obj(world, BANANA_PEEL, pos);
        } else {
            crate::explosions::explosion(
                world,
                pos,
                crate::explosions::Explosion::new(1, 5, 10).flash(5),
            );
        }
    }
}

/// tg `item_interaction`: the held item into the chamber. True when the click was the machine's.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(state) = world.get::<Experimentor>(target) else {
        return false;
    };
    let loaded = state.loaded.is_some_and(|l| world.get_entity(l).is_ok());
    if world.get::<CombatMode>(mob).is_some_and(|c| c.0) {
        return false;
    }
    // `is_insertion_ready`.
    let refusal = if world.get::<Unpowered>(target).is_some() {
        Some("no power!")
    } else if loaded {
        Some("item already loaded!")
    } else {
        None
    };
    if let Some(text) = refusal {
        tell(world, client, Span::Warning, text);
        return true;
    }
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        for hand in inventory.hands.iter_mut() {
            if *hand == Some(held) {
                *hand = None;
            }
        }
    }
    super::relics::ensure_relic(world, held);
    world.entity_mut(held).remove::<CarriedBy>();
    put_inside(world, held, target);
    if let Some(mut m) = world.get_mut::<Experimentor>(target) {
        m.loaded = Some(held);
    }
    let name = item_name(world, held);
    tell(
        world,
        client,
        Span::Notice,
        format!("You put {name} in the chamber."),
    );
    generate_reaction(world, target, held);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_ids_round_trip() {
        for scan in SCANS {
            assert_eq!(Scan::from_id(scan.id()), Some(scan));
        }
        assert_eq!(Scan::from_id("Fail"), None, "not a selectable scan");
        assert_eq!(Scan::Cold.id(), "Freeze");
    }
}
