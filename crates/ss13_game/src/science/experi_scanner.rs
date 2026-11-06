//! Experiment handlers (server), after tg `/datum/component/experiment_handler`
//! (`experiment_handler.dm`) with tgui's `ExperimentConfigure` window, the experi-scanner
//! (`/obj/item/experi_scanner`, `handheld_scanner.dm`) and the destructive scanner
//! (`/obj/machinery/destructive_scanner`, `destructive_scanner.dm`).
//!
//! - A handler is linked to the station techweb (tg auto-connects on creation) and has one
//!   selected experiment. Its window lists the techweb while an R&D server exists and the
//!   available experiments the handler can perform (`is_compatible_experiment`: no disallowed
//!   traits, the parent among `allowed_experimentors`, an allowed experiment type).
//! - The experi-scanner (its window on use in hand) performs scanning experiments but not
//!   destructive ones: a click on an atom the selected experiment can use (`pre_attack`) starts
//!   a 1 s `do_after`, then scans it; on anything else it says the atom is not related and the
//!   click goes on. Living mobs are taken before melee.
//! - The destructive scanner (its window on a click with an empty hand) performs any scanning
//!   experiment: "Выполнить эксперимент" closes the tube over the items, decals and mobs on its
//!   tile (refused while a human is there), scans for 6 s, opens and runs the selected
//!   experiment over them; scanned items are destroyed by destructive experiments, mobs are
//!   gibbed.
//! - A completed experiment is announced by every linked handler that could perform it, and by
//!   every experi-scanner (`EXPERIMENT_CONFIG_ALWAYS_ANNOUNCE`).
// shortcut: no sounds, icon states (`tube_on`), power use or emag (humans are always refused) on
// the destructive scanner, whose mobs stay on its tile during the scan (gibbed if still there);
// no meat-toilet suicide; one techweb, so `select_server` takes only its id.

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::research::ExperimentKind;
use ss13_mapload::{DmCleanable, DmItem, DmMob, DmName, DmObj, DmTurfBuild};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::UiInterface as _;
use ss13_tgui::UiStatus;
use ss13_tgui::server::{
    AlwaysState, AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};
use ss13_world::{SpatialIndex, WorldSystems};

use super::experiments::{TRAIT_DESTRUCTIVE, performable};
use super::points::ResearchServer;
use super::techweb::{ResearchDb, Techweb};
use super::*;
use crate::click::{ClickResolved, ResolveClicks};
use crate::construction::Job;
use crate::health::{Health, Stat};
use crate::inventory::{CarriedBy, Inventory};
use crate::obj_damage::is_under;
use crate::power::Unpowered;
use crate::storage::{StoredIn, put_inside, take_out};
use crate::tools::{Actor, DoAfter, tell};

pub const SCANNER: &str = "/obj/item/experi_scanner";
pub const DESTRUCTIVE_SCANNER: &str = "/obj/machinery/destructive_scanner";
const SCANNING: &str = "/datum/experiment/scanning";
const PHYSICAL: &str = "/datum/experiment/physical";
/// The handheld scan's `do_after`, deciseconds.
const SCAN_DELAY_DS: f32 = 10.0;
/// `start_closing` -> `start_scanning` -> `finish_scanning` -> `open`, seconds.
const CLOSE_SECS: f64 = 1.2;
const SCAN_SECS: f64 = 6.0;
const OPEN_SECS: f64 = 1.2;
const SAY_RANGE: u32 = 7;

/// Which configuration of `experiment_handler` its parent uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandlerKind {
    /// The experi-scanner: scanning and physical experiments, no destructive ones, always
    /// announces.
    Handheld,
    /// The destructive scanner: scanning experiments, a start button.
    Destructive,
}

/// tg `/datum/component/experiment_handler`.
#[derive(Component, Clone, Debug)]
pub struct ExperimentHandler {
    pub kind: HandlerKind,
    /// The parent's type, for `allowed_experimentors`.
    pub parent_path: String,
    /// `linked_web`: the station techweb.
    pub linked: bool,
    /// `selected_experiment`'s type path.
    pub selected: Option<String>,
}

/// The destructive scanner's cycle (`scanning` and its timers).
#[derive(Component, Clone, Debug, Default)]
pub struct DestructiveScanner {
    pub cycle: Option<ScanCycle>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanStage {
    Closing,
    Scanning,
    Opening,
}

#[derive(Clone, Debug)]
pub struct ScanCycle {
    pub stage: ScanStage,
    /// Game time the stage ends.
    pub until: f64,
    /// What the tube closed over.
    pub inside: Vec<Entity>,
}

pub struct ExperiScannerPlugin;

impl Plugin for ExperiScannerPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // The experi-scanner opens its experiment configuration.
        app.add_click_handler(ClickPhase::AttackSelf, 2400, |w, c| {
            crate::science::experi_scanner::attack_self(w, c.client, c.mob, c.target)
        });
        // tg `pre_attack` of the experi-scanner: a scan instead of the attack (living mobs are
        // taken before melee).
        app.add_click_handler(ClickPhase::HeldOnTarget, 4400, |w, c| {
            w.get::<crate::health::Health>(c.target).is_none()
                && c.held.is_some_and(|h| {
                    crate::science::experi_scanner::pre_attack(w, c.client, c.mob, h, c.target)
                })
        });
        app.register_ui::<ExperimentConfigureUi>()
            .add_systems(ss13_world::MapInit, setup_destructive_scanners)
            .add_systems(
                Update,
                (
                    open_destructive_scanner
                        .after(WorldSystems::Movement)
                        .after(ResolveClicks)
                        .before(UiSystems::Open),
                    scanner_mob_clicks
                        .after(crate::security::security_clicks)
                        .before(crate::combat::MeleeSystems),
                    configure_acts.in_set(UiSystems::Handle),
                    unpowered_windows.in_set(UiSystems::StatusHook),
                    run_destructive_scanners.run_if(any_with_component::<DestructiveScanner>),
                ),
            );
    }
}

fn setup_destructive_scanners(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &DmObj), With<ss13_world::Mapped>>();
    let found: Vec<(Entity, String)> = query
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, DESTRUCTIVE_SCANNER))
        .map(|(e, o)| (e, o.path.clone()))
        .collect();
    for (e, path) in found {
        setup(world, e, path);
    }
}

/// A destructive scanner's handler and cycle (map load and `crate::machine_frame`).
pub(crate) fn setup(world: &mut World, e: Entity, path: String) {
    world.entity_mut(e).insert((
        ExperimentHandler {
            kind: HandlerKind::Destructive,
            parent_path: path,
            linked: true,
            selected: None,
        },
        DestructiveScanner::default(),
    ));
}

/// The experi-scanner's handler, made the first time it is used (tg adds it in
/// `LateInitialize`). False when `item` is not an experi-scanner.
pub fn ensure_handheld(world: &mut World, item: Entity) -> bool {
    if world.get::<ExperimentHandler>(item).is_some() {
        return true;
    }
    let Some(path) = world.get::<DmItem>(item).map(|i| i.path.clone()) else {
        return false;
    };
    if !is_under(&path, SCANNER) {
        return false;
    }
    // Its window follows the scanner, not a tile: open while its user holds it.
    world.entity_mut(item).insert((
        ExperimentHandler {
            kind: HandlerKind::Handheld,
            parent_path: path,
            linked: true,
            selected: None,
        },
        AlwaysState,
    ));
    true
}

/// tg `is_compatible_experiment`.
pub fn is_compatible(
    handler: &ExperimentHandler,
    def: &ss13_mapload::research::ExperimentDef,
) -> bool {
    let (allowed, disallowed): (&[&str], u32) = match handler.kind {
        HandlerKind::Handheld => (&[SCANNING, PHYSICAL], TRAIT_DESTRUCTIVE),
        HandlerKind::Destructive => (&[SCANNING], 0),
    };
    def.traits & disallowed == 0
        && (def.allowed_experimentors.is_empty()
            || def
                .allowed_experimentors
                .iter()
                .any(|t| is_under(&handler.parent_path, t)))
        && allowed.iter().any(|root| is_under(&def.id, root))
}

/// The type path of an atom, for `istype` against required types.
fn atom_path(world: &World, e: Entity) -> Option<String> {
    let entity = world.get_entity(e).ok()?;
    entity
        .get::<DmObj>()
        .map(|o| o.path.clone())
        .or_else(|| entity.get::<DmItem>().map(|i| i.path.clone()))
        .or_else(|| entity.get::<DmCleanable>().map(|c| c.path.clone()))
        .or_else(|| entity.get::<DmMob>().map(|m| m.path.clone()))
        .or_else(|| entity.get::<DmTurfBuild>().map(|t| t.path.clone()))
}

fn name_of(world: &World, e: Entity) -> String {
    world
        .get::<DmName>(e)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default()
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Where an atom is: its tile, or its holder's or container's.
fn atom_pos(world: &World, e: Entity) -> Option<TilePos> {
    let mut current = e;
    for _ in 0..8 {
        if let Some(&pos) = world.get::<TilePos>(current) {
            return Some(pos);
        }
        current = world
            .get::<CarriedBy>(current)
            .map(|c| c.0)
            .or_else(|| world.get::<StoredIn>(current).map(|s| s.0))?;
    }
    None
}

/// tg `say()` of the handler's parent, held or not.
fn handler_say(world: &mut World, handler: Entity, text: &str) {
    let Some(pos) = atom_pos(world, handler) else {
        return;
    };
    let name = name_of(world, handler);
    let mut query = world.query_filtered::<Entity, With<ss13_net::server::ClientToken>>();
    let clients: Vec<Entity> = query
        .iter(world)
        .filter(|&c| {
            let mob = world.resource::<crate::server::Players>().mob(c);
            mob.and_then(|m| world.get::<TilePos>(m))
                .and_then(|p| p.chebyshev(pos))
                .is_some_and(|d| d <= SAY_RANGE)
        })
        .collect();
    if !clients.is_empty() {
        world.write_message(ss13_net::chat::server::SendChat {
            clients,
            message: ss13_chat::server::say_message(&name, text),
        });
    }
}

/// `actionable()` of the selected experiment with its `final_contributing_index_checks`: the
/// required type `target` counts towards.
fn actionable(
    world: &mut World,
    handler: Entity,
    state: &ExperimentHandler,
    id: &str,
    target: Entity,
) -> Option<usize> {
    let path = atom_path(world, target)?;
    let (kind, index) = {
        let db = world.get_resource::<ResearchDb>()?;
        let web = world.get_resource::<Techweb>()?;
        let experiment = web.experiment(id)?;
        let def = &db.experiments[experiment.def];
        if !is_compatible(state, def) {
            return None;
        }
        (
            def.kind.clone(),
            experiment.requirement(def, target, &path)?,
        )
    };
    final_checks(world, handler, &kind, target).then_some(index)
}

/// `final_contributing_index_checks` of the experiment kinds.
fn final_checks(world: &mut World, handler: Entity, kind: &ExperimentKind, target: Entity) -> bool {
    match kind {
        ExperimentKind::Scanning | ExperimentKind::Points => true,
        ExperimentKind::MachineryTiered { tier } => {
            let parts = crate::stock_parts::ratings(world, target);
            if parts.parts.iter().any(|&(_, t)| t >= *tier) {
                return true;
            }
            handler_say(
                world,
                handler,
                &format!(
                    "В отсканированной машине отсутствуют достаточно качественные детали. \
                     Требуется уровень {tier} или выше."
                ),
            );
            false
        }
        ExperimentKind::MachineryPinpoint { part, part_name } => {
            crate::stock_parts::ensure_parts(world, target);
            let found = world
                .get::<crate::stock_parts::ComponentParts>(target)
                .is_some_and(|parts| {
                    parts.parts().iter().any(|&p| {
                        world
                            .get::<DmItem>(p)
                            .is_some_and(|i| is_under(&i.path, part))
                    })
                });
            if !found {
                handler_say(
                    world,
                    handler,
                    &format!(
                        "В отсканированной машине отсутствуют достаточно качественные детали. \
                         Требуется части уровня {part_name}."
                    ),
                );
            }
            found
        }
        ExperimentKind::Reagent {
            reagent,
            min_purity,
            ..
        } => {
            // `is_valid_scan_target`.
            let failure = {
                let Some(holder) = world.get::<crate::chem::Reagents>(target).map(|r| &r.0) else {
                    return false;
                };
                let wanted = world
                    .get_resource::<crate::chem::Chem>()
                    .and_then(|c| c.0.id(reagent));
                match holder.master_reagent() {
                    _ if holder.total_volume <= 0.0 => Some("Контейнер пуст!"),
                    None => Some("Контейнер пуст!"),
                    Some(r) if Some(r.id) != wanted => Some("Реагент не обнаружен!"),
                    Some(r) if f64::from(r.purity) < *min_purity => Some("Слишком низкая чистота!"),
                    Some(_) => None,
                }
            };
            if let Some(text) = failure {
                handler_say(world, handler, text);
                return false;
            }
            true
        }
        ExperimentKind::Unsupported => false,
    }
}

/// tg `action_experiment`: performs the handler's selected experiment on `target`. True when
/// it counted.
pub fn action_experiment(world: &mut World, handler: Entity, target: Entity) -> bool {
    let Some(state) = world.get::<ExperimentHandler>(handler).cloned() else {
        return false;
    };
    let Some(id) = state.selected.clone().filter(|_| state.linked) else {
        return false;
    };
    let Some(index) = actionable(world, handler, &state, &id, target) else {
        return false;
    };
    let path = atom_path(world, target).unwrap_or_default();
    let outcome = world.resource_scope(|world, mut web: Mut<Techweb>| {
        let db = world.resource::<ResearchDb>();
        let experiment = web.available_experiments.iter_mut().find(|e| e.id == id)?;
        let def = &db.experiments[experiment.def];
        experiment.record(def, index, target, &path);
        Some((
            def.traits & TRAIT_DESTRUCTIVE != 0,
            experiment.is_complete(def),
        ))
    });
    let Some((destructive, complete)) = outcome else {
        return false;
    };
    // Only things are destroyed here; living things are gibbed by the destructive scanner.
    if destructive && world.get::<Health>(target).is_none() && world.get_entity(target).is_ok() {
        let pos = world.get::<TilePos>(target).copied();
        world.despawn(target);
        if let Some(pos) = pos {
            crate::construction::tile_changed(world, pos);
        }
    }
    if complete {
        finish_experiment(world, handler, &id);
    }
    true
}

/// tg `finish_experiment` and `announce_message_to_all`.
fn finish_experiment(world: &mut World, handler: Entity, id: &str) {
    if let Some(mut h) = world.get_mut::<ExperimentHandler>(handler) {
        h.selected = None;
    }
    let text = world.resource_scope(|world, mut web: Mut<Techweb>| {
        web.complete_experiment(world.resource::<ResearchDb>(), id)
    });
    let Some(&def) = world.resource::<ResearchDb>().experiment_index.get(id) else {
        return;
    };
    let mut query = world.query::<(Entity, &ExperimentHandler)>();
    let speakers: Vec<Entity> = {
        let db = world.resource::<ResearchDb>();
        query
            .iter(world)
            .filter(|(_, h)| {
                h.linked
                    && (h.kind == HandlerKind::Handheld || is_compatible(h, &db.experiments[def]))
            })
            .map(|(e, _)| e)
            .collect()
    };
    for speaker in speakers {
        handler_say(world, speaker, &text);
    }
}

fn not_related(world: &mut World, client: Entity, target: Entity) {
    let name = capitalize(&name_of(world, target));
    tell(
        world,
        client,
        Span::Notice,
        format!("{name} не имеет отношения к выбранному эксперименту."),
    );
}

/// tg `try_run_handheld_experiment` (`COMSIG_ITEM_PRE_ATTACK`) and
/// `ignored_handheld_experiment_attempt`: the experi-scanner `held` on `target`. True when the
/// click was the scanner's (the scan started).
pub fn pre_attack(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if !ensure_handheld(world, held) {
        return false;
    }
    let state = world
        .get::<ExperimentHandler>(held)
        .cloned()
        .expect("ensured above");
    let Some(id) = state.selected.clone() else {
        return false;
    };
    // `should_run_handheld_experiment`.
    if state.linked && actionable(world, held, &state, &id, target).is_some() {
        let actor = Actor {
            client,
            mob,
            item: Some(held),
        };
        crate::tools::do_after(world, actor, target, SCAN_DELAY_DS, 0, Job::Experiment);
        return true;
    }
    not_related(world, client, target);
    false
}

/// [`Job::Experiment`] done: `try_run_handheld_experiment_async` after its `do_after`.
pub fn complete(world: &mut World, _mob: Entity, action: &DoAfter) {
    let Some(item) = action.item else {
        return;
    };
    let target = action.target;
    let name = name_of(world, target);
    if action_experiment(world, item, target) {
        tell(
            world,
            action.client,
            Span::Notice,
            format!("Вы сканируете {name}."),
        );
    } else {
        not_related(world, action.client, target);
    }
}

/// Living mobs clicked with an experi-scanner, before melee.
fn scanner_mob_clicks(world: &mut World, mut clicks: Local<MessageCursor<ClickResolved>>) {
    let clicks: Vec<ClickResolved> = clicks
        .read(world.resource::<Messages<ClickResolved>>())
        .copied()
        .collect();
    for click in clicks {
        let (mob, target) = (click.mob, click.target);
        if world
            .resource::<crate::security::MeleeHandled>()
            .0
            .contains(&mob)
            || world.get::<Health>(target).is_none()
            || world.get::<Stat>(mob) != Some(&Stat::Conscious)
        {
            continue;
        }
        let Some(held) = world.get::<Inventory>(mob).and_then(Inventory::active_item) else {
            continue;
        };
        if pre_attack(world, click.client, mob, held, target) {
            world
                .resource_mut::<crate::security::MeleeHandled>()
                .0
                .insert(mob);
        }
    }
}

/// tg `configure_experiment` on `COMSIG_ITEM_ATTACK_SELF`: the experi-scanner's window.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if !ensure_handheld(world, item) {
        return false;
    }
    let title = format!(
        "{} | Конфигурация Экспериментов",
        capitalize(&name_of(world, item))
    );
    world.write_message(OpenUi::new::<ExperimentConfigureUi>(
        client, mob, item, title,
    ));
    true
}

/// `EXPERIMENT_CONFIG_CLICK`: the destructive scanner's window on a click with an empty hand.
fn open_destructive_scanner(
    mut clicks: MessageReader<ClickResolved>,
    scanners: Query<Option<&DmName>, With<DestructiveScanner>>,
    inventories: Query<&Inventory>,
    mut open: MessageWriter<OpenUi>,
) {
    for click in clicks.read() {
        let Ok(name) = scanners.get(click.target) else {
            continue;
        };
        if inventories
            .get(click.mob)
            .is_ok_and(|i| i.active_item().is_none())
        {
            let name = name.map_or("", |n| crate::vending::plain_name(&n.name));
            let title = format!("{} | Конфигурация Экспериментов", capitalize(name));
            open.write(OpenUi::new::<ExperimentConfigureUi>(
                click.client,
                click.mob,
                click.target,
                title,
            ));
        }
    }
}

/// An unpowered destructive scanner's window is update-only; an experi-scanner's closes when it
/// leaves its user's hands (`default_state`).
fn unpowered_windows(
    mut windows: Query<&mut UiWindow>,
    unpowered: Query<(), (With<DestructiveScanner>, With<Unpowered>)>,
    handlers: Query<(&ExperimentHandler, Option<&CarriedBy>)>,
) {
    for mut window in &mut windows {
        if window.interface != ExperimentConfigureUi::NAME {
            continue;
        }
        if unpowered.contains(window.source) {
            window.cap_status(UiStatus::Update);
        }
        if let Ok((h, carried)) = handlers.get(window.source)
            && h.kind == HandlerKind::Handheld
            && carried.is_none_or(|c| c.0 != window.actor)
        {
            window.cap_status(UiStatus::Close);
        }
    }
}

type ConfigureParams = (
    Option<Res<'static, ResearchDb>>,
    Option<Res<'static, Techweb>>,
    Query<'static, 'static, Option<&'static DmName>, With<ResearchServer>>,
);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigureAct {
    SelectServer(String),
    ClearServer,
    SelectExperiment(String),
    ClearExperiment,
    Start,
}

/// The experiment's `to_ui_data()` tag: destructive ones say so (`/datum/experiment/New`).
fn tag(def: &ss13_mapload::research::ExperimentDef) -> String {
    if def.traits & TRAIT_DESTRUCTIVE != 0 {
        format!("Destructive {}", def.exp_tag)
    } else {
        def.exp_tag.clone()
    }
}

impl ServerUi for ExperimentConfigureUi {
    type Source = &'static ExperimentHandler;
    type UserParam = ConfigureParams;
    type Act = ConfigureAct;

    fn static_data(_: &ExperimentHandler) {}

    fn data(
        handler: &ExperimentHandler,
        _: Entity,
        (db, web, servers): &SystemParamItem<'_, '_, ConfigureParams>,
    ) -> ExperimentConfigureData {
        let mut data = ExperimentConfigureData {
            has_start_callback: handler.kind == HandlerKind::Destructive,
            ..default()
        };
        let (Some(db), Some(web)) = (db.as_deref(), web.as_deref()) else {
            return data;
        };
        let all_servers: Vec<String> = servers
            .iter()
            .map(|n| {
                n.map_or_else(String::new, |n| {
                    crate::vending::plain_name(&n.name).to_owned()
                })
            })
            .collect();
        // No servers: no techweb to connect to.
        if all_servers.is_empty() {
            return data;
        }
        data.techwebs.push(ExperimentWeb {
            web_id: web.id.clone(),
            web_org: web.organization.clone(),
            selected: handler.linked,
            all_servers,
        });
        if !handler.linked {
            return data;
        }
        data.experiments = web
            .available_experiments
            .iter()
            .filter(|e| is_compatible(handler, &db.experiments[e.def]))
            .map(|e| {
                let def = &db.experiments[e.def];
                ExperimentInfo {
                    path: e.id.clone(),
                    name: def.name.clone(),
                    description: def.description.clone(),
                    tag: tag(def),
                    performance_hint: def.performance_hint.clone(),
                    progress: e.progress(def),
                    completed: false,
                    selected: handler.selected.as_deref() == Some(e.id.as_str()),
                }
            })
            .collect();
        data
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<ConfigureAct, UiReject> {
        let reference = || {
            params
                .get(EXPERIMENT_REF)
                .map(str::to_owned)
                .ok_or_else(|| UiReject::BadParams("ref: missing".into()))
        };
        match action {
            SELECT_SERVER => Ok(ConfigureAct::SelectServer(reference()?)),
            CLEAR_SERVER => Ok(ConfigureAct::ClearServer),
            SELECT_EXPERIMENT => Ok(ConfigureAct::SelectExperiment(reference()?)),
            CLEAR_EXPERIMENT => Ok(ConfigureAct::ClearExperiment),
            START_EXPERIMENT => Ok(ConfigureAct::Start),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// tg `ui_act` of the handler.
fn configure_acts(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<ExperimentConfigureUi>>>,
) {
    let pending: Vec<(Entity, ConfigureAct)> = acts
        .read(world.resource::<Messages<UiActed<ExperimentConfigureUi>>>())
        .map(|a| (a.source, a.act.clone()))
        .collect();
    for (source, act) in pending {
        if world.get::<Unpowered>(source).is_some() {
            continue;
        }
        let Some(state) = world.get::<ExperimentHandler>(source).cloned() else {
            continue;
        };
        let mut next = state.clone();
        match act {
            ConfigureAct::SelectServer(web_id) => {
                let servers = world
                    .query_filtered::<(), With<ResearchServer>>()
                    .iter(world)
                    .count();
                if servers > 0
                    && world
                        .get_resource::<Techweb>()
                        .is_some_and(|w| w.id == web_id)
                    && !state.linked
                {
                    // `link_techweb`.
                    next.linked = true;
                    next.selected = None;
                }
            }
            ConfigureAct::ClearServer => {
                next.linked = false;
                next.selected = None;
            }
            ConfigureAct::SelectExperiment(id) => {
                // `link_experiment` if `can_select_experiment`.
                let selectable = state.linked
                    && world.get_resource::<ResearchDb>().is_some_and(|db| {
                        world
                            .get_resource::<Techweb>()
                            .and_then(|w| w.experiment(&id))
                            .is_some_and(|e| is_compatible(&state, &db.experiments[e.def]))
                            && performable(db, &id)
                    });
                if selectable {
                    next.selected = Some(id);
                }
            }
            ConfigureAct::ClearExperiment => next.selected = None,
            ConfigureAct::Start => {
                if state.kind == HandlerKind::Destructive {
                    activate(world, source);
                }
                continue;
            }
        }
        if let Some(mut h) = world.get_mut::<ExperimentHandler>(source) {
            *h = next;
        }
    }
}

/// A human on the scanner's tile (tg `ishuman`): a carbon with hands that is not a cyborg.
fn is_human(world: &World, e: Entity) -> bool {
    world.get::<Inventory>(e).is_some()
        && world.get::<DmMob>(e).is_none()
        && world.get::<crate::silicon::cyborg::Cyborg>(e).is_none()
}

/// tg `/obj/machinery/destructive_scanner/proc/activate` and `start_closing`.
pub fn activate(world: &mut World, scanner: Entity) -> bool {
    if world
        .get::<DestructiveScanner>(scanner)
        .is_none_or(|s| s.cycle.is_some())
    {
        return false;
    }
    let Some(&pos) = world.get::<TilePos>(scanner) else {
        return false;
    };
    let on_tile: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| e != scanner)
        .collect();
    if on_tile
        .iter()
        .any(|&e| world.get::<Health>(e).is_some() && is_human(world, e))
    {
        crate::cargo::console::machine_say(world, scanner, "Cannot scan with humans inside.");
        return false;
    }
    // The movables the tube takes in: items and decals go inside, mobs stay under it.
    let mut inside = Vec::new();
    for e in on_tile {
        if world.get::<Health>(e).is_some() {
            inside.push(e);
        } else if world.get::<DmItem>(e).is_some() || world.get::<DmCleanable>(e).is_some() {
            if world.get::<DmItem>(e).is_some_and(|i| i.anchored) {
                continue;
            }
            put_inside(world, e, scanner);
            inside.push(e);
        }
    }
    crate::construction::tile_changed(world, pos);
    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(mut s) = world.get_mut::<DestructiveScanner>(scanner) {
        s.cycle = Some(ScanCycle {
            stage: ScanStage::Closing,
            until: now + CLOSE_SECS,
            inside,
        });
    }
    true
}

/// The destructive scanners' timers.
fn run_destructive_scanners(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut query = world.query::<(Entity, &DestructiveScanner)>();
    let due: Vec<(Entity, ScanCycle)> = query
        .iter(world)
        .filter_map(|(e, s)| s.cycle.clone().filter(|c| now >= c.until).map(|c| (e, c)))
        .collect();
    for (scanner, cycle) in due {
        let next = match cycle.stage {
            ScanStage::Closing => Some((ScanStage::Scanning, SCAN_SECS)),
            ScanStage::Scanning => Some((ScanStage::Opening, OPEN_SECS)),
            ScanStage::Opening => None,
        };
        let new_cycle = next.map(|(stage, secs)| ScanCycle {
            stage,
            until: now + secs,
            inside: cycle.inside.clone(),
        });
        let finished = new_cycle.is_none();
        if let Some(mut state) = world.get_mut::<DestructiveScanner>(scanner) {
            state.cycle = new_cycle;
        }
        if finished {
            open(world, scanner, &cycle.inside);
        }
    }
}

/// tg `open`: the contents come out, the experiment runs over them
/// (`try_run_destructive_experiment`), mobs are gibbed.
fn open(world: &mut World, scanner: Entity, inside: &[Entity]) {
    let Some(&pos) = world.get::<TilePos>(scanner) else {
        return;
    };
    let mut scanned = Vec::new();
    for &e in inside {
        if world.get_entity(e).is_err() {
            continue;
        }
        if world.get::<StoredIn>(e).is_some_and(|s| s.0 == scanner) {
            take_out(world, e, pos);
            scanned.push(e);
        } else if world.get::<Health>(e).is_some() && world.get::<TilePos>(e) == Some(&pos) {
            scanned.push(e);
        }
    }
    crate::construction::tile_changed(world, pos);
    let selected = world
        .get::<ExperimentHandler>(scanner)
        .is_some_and(|h| h.selected.is_some());
    if selected {
        let mut success = false;
        for &e in &scanned {
            if world.get_entity(e).is_ok() && action_experiment(world, scanner, e) {
                success = true;
            }
        }
        if !success {
            crate::cargo::console::machine_say(world, scanner, "Сканирование ничего не дало.");
        }
    }
    for e in scanned {
        if world.get_entity(e).is_ok() && world.get::<Health>(e).is_some() {
            info!("science: {e:?} gibbed by the destructive scanner");
            crate::health::gib(world, e);
        }
    }
}
