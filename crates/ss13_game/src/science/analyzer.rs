//! The destructive analyzer (server), after tg `/obj/machinery/rnd/destructive_analyzer`
//! (`destructive_analyzer.dm`) with tgui's `DestructiveAnalyzer` window.
//!
//! A click with an item out of combat mode loads it (1 s busy); a click with an empty hand opens
//! the window. `deconstruct` destroys the item and everything inside it (2.4 s busy, 250 J):
//! without a node, each destroyed type in `SSresearch.techweb_point_items` gives its points once
//! per techweb (`deconstructed_items`); with a node the item's type is one of the node's
//! `required_items_to_unlock`, the node is unhidden. `eject_item` drops the item.
// shortcut: no alt-click unload, wires, panel or emag insertion; items carry no
// `resistance_flags` here, so only objects with `INDESTRUCTIBLE` in theirs are refused.

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
use ss13_world::WorldSystems;

use super::techweb::{ResearchDb, Techweb};
use super::*;
use crate::cargo::console::machine_say;
use crate::click::{ClickResolved, ResolveClicks};
use crate::combat::CombatMode;
use crate::inventory::{CarriedBy, Inventory};
use crate::obj_damage::is_under;
use crate::power::{PowerMachine, Unpowered};
use crate::storage::{Stored, put_inside, take_out};
use crate::tools::tell;

pub const ANALYZER_ROOT: &str = "/obj/machinery/rnd/destructive_analyzer";
/// `DESTRUCTIVE_ANALYZER_POWER_USAGE`: `BASE_MACHINE_IDLE_CONSUMPTION * 2.5`, joules.
const POWER_USAGE: f64 = 250.0;
/// `INDESTRUCTIBLE`.
const INDESTRUCTIBLE: u32 = 1 << 6;
const LOAD_SECS: f64 = 1.0;
const DESTROY_SECS: f64 = 2.4;

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Analyzer {
    pub loaded: Option<Entity>,
    /// Game time `busy` ends.
    pub busy_until: f64,
}

pub struct AnalyzerPlugin;

impl Plugin for AnalyzerPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<DestructiveAnalyzerUi>()
            .add_systems(Startup, setup_analyzers)
            .add_systems(
                Update,
                (
                    open_analyzer
                        .after(WorldSystems::Movement)
                        .after(ResolveClicks)
                        .before(UiSystems::Open),
                    analyzer_acts.in_set(UiSystems::Handle),
                    unpowered_windows.in_set(UiSystems::StatusHook),
                ),
            );
    }
}

fn setup_analyzers(world: &mut World) {
    let mut query = world.query::<(Entity, &DmObj)>();
    let found: Vec<Entity> = query
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, ANALYZER_ROOT))
        .map(|(e, _)| e)
        .collect();
    for e in found {
        world.entity_mut(e).insert(Analyzer::default());
    }
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn open_analyzer(
    mut clicks: MessageReader<ClickResolved>,
    analyzers: Query<(), With<Analyzer>>,
    inventories: Query<&Inventory>,
    mut open: MessageWriter<OpenUi>,
) {
    for click in clicks.read() {
        if analyzers.contains(click.target)
            && inventories
                .get(click.mob)
                .is_ok_and(|i| i.active_item().is_none())
        {
            open.write(OpenUi::new::<DestructiveAnalyzerUi>(
                click.client,
                click.mob,
                click.target,
                "Destructive Analyzer",
            ));
        }
    }
}

fn unpowered_windows(
    mut windows: Query<&mut UiWindow>,
    unpowered: Query<(), (With<Analyzer>, With<Unpowered>)>,
) {
    for mut window in &mut windows {
        if window.interface == DestructiveAnalyzerUi::NAME && unpowered.contains(window.source) {
            window.cap_status(UiStatus::Update);
        }
    }
}

/// tg `techweb_point_display_generic`.
pub fn point_display(db: &ResearchDb, points: &[(String, f64)]) -> String {
    points
        .iter()
        .map(|(t, n)| {
            let name = db
                .point_types
                .iter()
                .find(|(p, _)| p == t)
                .map_or("ERRORED POINT TYPE", |(_, a)| a.as_str());
            format!("{name}: {n}")
        })
        .collect::<Vec<_>>()
        .join("<BR>")
}

type AnalyzerParams = (
    Option<Res<'static, ResearchDb>>,
    Option<Res<'static, Techweb>>,
    Query<
        'static,
        'static,
        (
            &'static DmItem,
            Option<&'static DmName>,
            Option<&'static DmObj>,
            Option<&'static Appearance>,
        ),
    >,
);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AnalyzerAct {
    Eject,
    Deconstruct(Option<String>),
}

impl ServerUi for DestructiveAnalyzerUi {
    type Source = &'static Analyzer;
    type UserParam = AnalyzerParams;
    type Act = AnalyzerAct;

    fn static_data(_: &Analyzer) {}

    fn data(
        analyzer: &Analyzer,
        _: Entity,
        (db, web, items): &SystemParamItem<'_, '_, AnalyzerParams>,
    ) -> AnalyzerData {
        let (Some(db), Some(web)) = (db.as_deref(), web.as_deref()) else {
            return AnalyzerData::default();
        };
        let mut data = AnalyzerData {
            server_connected: true,
            ..default()
        };
        let Some((item, name, obj, appearance)) = analyzer.loaded.and_then(|e| items.get(e).ok())
        else {
            return data;
        };
        data.loaded_item = Some(name.map_or_else(|| item.path.clone(), |n| n.name.clone()));
        data.item_icon = appearance.map(|a| ProductIcon {
            icon_state: a.icon_state,
            color: a.color,
        });
        data.indestructible = obj.is_none_or(|o| o.resistance_flags & INDESTRUCTIBLE == 0);
        data.already_deconstructed = web.deconstructed_items.contains(&item.path);
        data.recoverable_points = db
            .point_items
            .get(&item.path)
            .map(|p| point_display(db, p))
            .unwrap_or_default();
        data.node_data = db
            .unlock_items
            .get(&item.path)
            .into_iter()
            .flatten()
            .map(|&n| AnalyzerNode {
                node_name: db.nodes[n].name.clone(),
                node_path: db.nodes[n].id.clone(),
                node_hidden: web.hidden.contains(&n),
            })
            .collect();
        data
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<AnalyzerAct, UiReject> {
        match action {
            EJECT_ITEM => Ok(AnalyzerAct::Eject),
            DECONSTRUCT => Ok(AnalyzerAct::Deconstruct(
                params.get(DECONSTRUCT_PATH).map(str::to_owned),
            )),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

fn analyzer_acts(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<DestructiveAnalyzerUi>>>,
) {
    let pending: Vec<(Entity, Entity, AnalyzerAct)> = acts
        .read(world.resource::<Messages<UiActed<DestructiveAnalyzerUi>>>())
        .map(|a| (a.client, a.source, a.act.clone()))
        .collect();
    for (client, analyzer, act) in pending {
        if world.get::<Unpowered>(analyzer).is_some() {
            continue;
        }
        let Some(state) = world.get::<Analyzer>(analyzer).copied() else {
            continue;
        };
        let busy = now(world) < state.busy_until;
        match act {
            AnalyzerAct::Eject if busy => {
                tell(world, client, Span::Warning, "already busy!");
            }
            AnalyzerAct::Eject => {
                unload(world, analyzer);
            }
            AnalyzerAct::Deconstruct(node) => {
                if !deconstruct(world, analyzer, node.as_deref()) {
                    machine_say(world, analyzer, "Destructive analysis failed!");
                }
            }
        }
    }
}

/// tg `unload_item`.
fn unload(world: &mut World, analyzer: Entity) -> bool {
    let Some(item) = world.get::<Analyzer>(analyzer).and_then(|a| a.loaded) else {
        return false;
    };
    let Some(&pos) = world.get::<TilePos>(analyzer) else {
        return false;
    };
    if world.get_entity(item).is_ok() {
        take_out(world, item, pos);
        crate::construction::tile_changed(world, pos);
    }
    if let Some(mut a) = world.get_mut::<Analyzer>(analyzer) {
        a.loaded = None;
    }
    true
}

/// `get_all_contents()`: the item and everything inside it, recursively.
fn all_contents(world: &World, item: Entity) -> Vec<Entity> {
    let mut out = vec![item];
    let mut i = 0;
    while i < out.len() {
        if let Some(stored) = world.get::<Stored>(out[i]) {
            out.extend(stored.items().iter().copied());
        }
        i += 1;
    }
    out
}

/// tg `try_deconstruct_loaded_item(node_path)`.
pub fn deconstruct(world: &mut World, analyzer: Entity, node: Option<&str>) -> bool {
    let Some(item) = world.get::<Analyzer>(analyzer).and_then(|a| a.loaded) else {
        return false;
    };
    if world.get_entity(item).is_err() || !world.contains_resource::<Techweb>() {
        return false;
    }
    let node = match node {
        None => None,
        Some(path) => match world.resource::<ResearchDb>().node_index.get(path) {
            Some(&n) => Some(n),
            None => return false,
        },
    };
    let gain_points = node.is_none();
    // `destroy_item`.
    let until = now(world) + DESTROY_SECS;
    if world.get::<PowerMachine>(analyzer).is_some() {
        crate::kitchen::machine_use_energy(world, analyzer, POWER_USAGE);
    }
    for thing in all_contents(world, item) {
        let path = world.get::<DmItem>(thing).map(|i| i.path.clone());
        if gain_points && let Some(path) = path {
            world.resource_scope(|world, mut web: Mut<Techweb>| {
                let db = world.resource::<ResearchDb>();
                if let Some(points) = db.point_items.get(&path)
                    && !points.is_empty()
                    && web.deconstructed_items.insert(path)
                {
                    web.adjust_multiple_points(db, points);
                }
            });
        }
    }
    if world.get_entity(item).is_ok() {
        world.despawn(item);
    }
    if let Some(mut a) = world.get_mut::<Analyzer>(analyzer) {
        a.loaded = None;
        a.busy_until = until;
    }
    if let Some(node) = node {
        world.resource_scope(|world, mut web: Mut<Techweb>| {
            web.unhide_node(world.resource::<ResearchDb>(), node);
        });
    }
    true
}

/// tg `base_item_interaction`: loads the held item. True when the click was the analyzer's.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(state) = world.get::<Analyzer>(target).copied() else {
        return false;
    };
    if world.get::<CombatMode>(mob).is_some_and(|c| c.0) {
        return false;
    }
    // `is_insertion_ready`.
    let refusal = if now(world) < state.busy_until {
        Some("still busy!")
    } else if world.get::<Unpowered>(target).is_some() {
        Some("no power!")
    } else if state.loaded.is_some() {
        Some("item already loaded!")
    } else {
        None
    };
    if let Some(text) = refusal {
        tell(world, client, Span::Warning, text);
        return true;
    }
    let name = world
        .get::<DmName>(held)
        .map(|n| n.name.clone())
        .unwrap_or_default();
    world.entity_mut(held).remove::<CarriedBy>();
    put_inside(world, held, target);
    let until = now(world) + LOAD_SECS;
    if let Some(mut a) = world.get_mut::<Analyzer>(target) {
        a.loaded = Some(held);
        a.busy_until = until;
    }
    tell(
        world,
        client,
        Span::Notice,
        format!("You place the {name} inside the destructive analyzer."),
    );
    true
}
