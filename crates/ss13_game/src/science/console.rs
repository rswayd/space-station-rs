//! The R&D console (server), after tg `/obj/machinery/computer/rdconsole` (`rdconsole.dm`) with
//! tgui's `Techweb` interface: a click with an empty hand opens the window.
//!
//! Every act but `toggleLock` is refused while the console is locked (BandaStation locks the
//! base type; MetaStation maps the `unlocked` subtype). `toggleLock` needs the console's
//! `req_access` (research). `researchNode` follows the console's `research_node` checks
//! (available, not researched, affordable) before the techweb's, pays the science budget its
//! bounty and logs the research; `enqueueNode`/`dequeueNode` manage the techweb's research queue.
//! An unpowered console is update-only and refuses acts.
// shortcut: no tech/design disks (`ejectDisk`, `uploadDisk`, `loadTech` are unknown acts),
// emagging, multitool relinking or the experiments section of the nodes.

use std::sync::Arc;

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::research::{SHEET_MATERIAL_AMOUNT, build_type};
use ss13_mapload::{DmName, DmObj};
use ss13_shared::chat::Span;
use ss13_tgui::UiInterface as _;
use ss13_tgui::UiStatus;
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};
use ss13_world::WorldSystems;

use super::points::{ResearchIcons, ResearchLoaded, pay_bounty};
use super::techweb::{POINT_GENERIC, ResearchDb, ResearchLog, Techweb};
use super::*;
use crate::cargo::console::machine_say;
use crate::cargo::economy::Economy;
use crate::click::{ClickResolved, ResolveClicks};
use crate::inventory::{IdCard, Inventory, check_access};
use crate::obj_damage::is_under;
use crate::power::Unpowered;
use crate::tools::tell;

pub const CONSOLE_ROOT: &str = ss13_mapload::research::CONSOLE_ROOT;
/// `ACCESS_RD`: queued nodes go first.
const ACCESS_RD: &str = "rd";

/// An R&D console: its board's lock and the access that toggles it.
#[derive(Component, Clone, Debug)]
pub struct RdConsole {
    pub locked: bool,
    pub req_access: Vec<String>,
    catalogue: Arc<TechwebStatic>,
}

pub struct ConsolePlugin;

impl Plugin for ConsolePlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<TechwebUi>()
            .add_systems(Startup, setup_consoles.after(ResearchLoaded))
            .add_systems(
                Update,
                (
                    open_console
                        .after(WorldSystems::Movement)
                        .after(ResolveClicks)
                        .before(UiSystems::Open),
                    console_acts.in_set(UiSystems::Handle),
                    unpowered_windows.in_set(UiSystems::StatusHook),
                ),
            );
    }
}

/// tg `ui_static_data`: every node and the designs they unlock.
pub fn catalogue(db: &ResearchDb, icons: Option<&ResearchIcons>) -> TechwebStatic {
    let mut design_ids: Vec<usize> = db.node_designs.iter().flatten().copied().collect();
    design_ids.sort_unstable();
    design_ids.dedup();
    let compressed = |d: usize| design_ids.binary_search(&d).map_or(0, |i| i as u16);
    let designs = design_ids
        .iter()
        .map(|&d| {
            let design = &db.designs[d];
            DesignInfo {
                name: design.name.clone(),
                cost: design
                    .materials
                    .iter()
                    .map(|(m, n)| {
                        let name = db
                            .material_index
                            .get(m)
                            .map_or_else(|| m.clone(), |&i| db.materials[i].name.clone());
                        (name, n.floor().max(1.0) as u32)
                    })
                    .collect(),
                build_types: design.build_type,
                department_flags: design.departmental_flags,
                icon: icons.and_then(|i| i.designs.get(d).copied().flatten()),
            }
        })
        .collect();
    let nodes = db
        .nodes
        .iter()
        .enumerate()
        .map(|(i, node)| NodeInfo {
            path: node.id.clone(),
            name: node.name.clone(),
            description: node.description.clone(),
            costs: node.costs.clone(),
            prerequisites: db.prerequisites[i].iter().map(|&p| p as u16).collect(),
            designs: db.node_designs[i].iter().map(|&d| compressed(d)).collect(),
            unlocked_nodes: db.unlocks[i].iter().map(|&n| n as u16).collect(),
            required_experiments: node.required_experiments.clone(),
            discount_experiments: node.discount_experiments.clone(),
        })
        .collect();
    TechwebStatic {
        point_types: db.point_types.clone(),
        nodes,
        designs,
        build_types: build_type::NAMES
            .iter()
            .map(|&(b, n)| (b, n.to_owned()))
            .collect(),
        departments: db.departments.clone(),
        sheet_amount: SHEET_MATERIAL_AMOUNT,
    }
}

fn setup_consoles(world: &mut World) {
    let mut query = world.query::<(Entity, &DmObj)>();
    let found: Vec<Entity> = query
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, CONSOLE_ROOT))
        .map(|(e, _)| e)
        .collect();
    let Some(db) = world.get_resource::<ResearchDb>() else {
        return;
    };
    let catalogue = Arc::new(catalogue(db, world.get_resource::<ResearchIcons>()));
    info!("research: {} R&D consoles", found.len());
    for e in found {
        setup_console(world, e, Some(catalogue.clone()));
    }
}

/// An R&D console's state (map load, and `crate::machine_frame` without a shared `catalogue`).
pub(crate) fn setup_console(world: &mut World, e: Entity, shared: Option<Arc<TechwebStatic>>) {
    let Some(db) = world.get_resource::<ResearchDb>() else {
        return;
    };
    let Some(path) = world.get::<DmObj>(e).map(|o| o.path.clone()) else {
        return;
    };
    let catalogue =
        shared.unwrap_or_else(|| Arc::new(catalogue(db, world.get_resource::<ResearchIcons>())));
    let def = db.consoles.get(&path);
    let console = RdConsole {
        locked: def.is_some_and(|d| d.locked),
        req_access: def.map(|d| d.req_access.clone()).unwrap_or_default(),
        catalogue,
    };
    world.entity_mut(e).insert(console);
}

fn plain(name: &str) -> &str {
    name.trim_start_matches("\\improper ")
        .trim_start_matches("\\proper ")
}

/// tg `ui_interact` on `attack_hand`.
fn open_console(
    mut clicks: MessageReader<ClickResolved>,
    consoles: Query<&DmName, (With<RdConsole>, Without<Unpowered>)>,
    inventories: Query<&Inventory>,
    mut open: MessageWriter<OpenUi>,
) {
    for click in clicks.read() {
        let Ok(name) = consoles.get(click.target) else {
            continue;
        };
        if inventories
            .get(click.mob)
            .is_ok_and(|i| i.active_item().is_none())
        {
            open.write(OpenUi::new::<TechwebUi>(
                click.client,
                click.mob,
                click.target,
                plain(&name.name),
            ));
        }
    }
}

fn unpowered_windows(
    mut windows: Query<&mut UiWindow>,
    unpowered: Query<(), (With<RdConsole>, With<Unpowered>)>,
) {
    for mut window in &mut windows {
        if window.interface == TechwebUi::NAME && unpowered.contains(window.source) {
            window.cap_status(UiStatus::Update);
        }
    }
}

type ConsoleParams = (
    Option<Res<'static, ResearchDb>>,
    Option<Res<'static, Techweb>>,
    Query<'static, 'static, &'static DmName>,
);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConsoleAct {
    ToggleLock,
    Research(String),
    Enqueue(String),
    Dequeue(String),
}

/// tg `ui_data` for `user`.
pub fn console_data(
    console: &RdConsole,
    user: Entity,
    db: Option<&ResearchDb>,
    web: Option<&Techweb>,
    names: &Query<&DmName>,
) -> TechwebData {
    let (Some(db), Some(web)) = (db, web) else {
        return TechwebData {
            locked: console.locked,
            ..default()
        };
    };
    let queuer = |node: usize| web.queue.iter().find(|(n, _)| *n == node).map(|(_, u)| *u);
    TechwebData {
        stored_research: true,
        locked: console.locked,
        nodes: web
            .shown_nodes()
            .map(|(node, tier)| NodeState {
                id: node as u16,
                is_free: web.is_free(db, node),
                can_unlock: web.can_unlock(db, node),
                have_experiments_done: web.have_experiments_for_node(db, node),
                tier,
                enqueued_by_user: queuer(node) == Some(user),
                discount_boosted: web.boosted.contains(&node),
            })
            .collect(),
        queue: web
            .queue
            .iter()
            .map(|(n, u)| {
                let name = names.get(*u).map(|n| n.name.clone()).unwrap_or_default();
                (*n as u16, name)
            })
            .collect(),
        points: web.points.iter().map(|(t, n)| (t.clone(), *n)).collect(),
        points_last_tick: web
            .last_bitcoins
            .iter()
            .map(|(t, n)| (t.clone(), *n))
            .collect(),
        web_org: web.organization.clone(),
        sec_protocols: true,
        completed_experiments: web.completed_experiments.iter().cloned().collect(),
    }
}

impl ServerUi for TechwebUi {
    type Source = &'static RdConsole;
    type UserParam = ConsoleParams;
    type Act = ConsoleAct;

    fn static_data(console: &RdConsole) -> TechwebStatic {
        (*console.catalogue).clone()
    }

    fn data(
        console: &RdConsole,
        user: Entity,
        (db, web, names): &SystemParamItem<'_, '_, ConsoleParams>,
    ) -> TechwebData {
        console_data(console, user, db.as_deref(), web.as_deref(), names)
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<ConsoleAct, UiReject> {
        let node = || {
            params
                .get(NODE_PATH)
                .map(str::to_owned)
                .ok_or_else(|| UiReject::BadParams("node_path: missing".into()))
        };
        match action {
            TOGGLE_LOCK => Ok(ConsoleAct::ToggleLock),
            RESEARCH_NODE => Ok(ConsoleAct::Research(node()?)),
            ENQUEUE_NODE => Ok(ConsoleAct::Enqueue(node()?)),
            DEQUEUE_NODE => Ok(ConsoleAct::Dequeue(node()?)),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// The access of the ID `get_idcard(hand_first = TRUE)` finds, and of the worn ID alone
/// (`wear_id?.GetAccess()`).
pub(super) fn id_access(world: &World, mob: Entity) -> (Vec<String>, Vec<String>) {
    let Some(inventory) = world.get::<Inventory>(mob) else {
        return (Vec::new(), Vec::new());
    };
    let card = |e: Option<Entity>| {
        e.and_then(|e| crate::inventory::get_id(world, e))
            .and_then(|e| world.get::<IdCard>(e))
            .map(|c| c.access.clone())
    };
    let any = [
        inventory.hands[inventory.active],
        inventory.hands[1 - inventory.active],
        inventory.id,
    ]
    .into_iter()
    .find_map(card)
    .unwrap_or_default();
    (any, card(inventory.id).unwrap_or_default())
}

fn console_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<TechwebUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, ConsoleAct)> = acts
        .read(world.resource::<Messages<UiActed<TechwebUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, console, act) in pending {
        if world.get::<Unpowered>(console).is_some() {
            continue;
        }
        let Some(state) = world.get::<RdConsole>(console).cloned() else {
            continue;
        };
        if state.locked && act != ConsoleAct::ToggleLock {
            machine_say(
                world,
                console,
                "Консоль заблокирована. Дальнейшие действия невозможны.",
            );
            continue;
        }
        match act {
            ConsoleAct::ToggleLock => {
                let (access, _) = id_access(world, actor);
                if check_access(&state.req_access, &[], &access) {
                    if let Some(mut c) = world.get_mut::<RdConsole>(console) {
                        c.locked = !c.locked;
                    }
                } else {
                    tell(world, client, Span::Warning, "Несанкционированный доступ.");
                }
            }
            ConsoleAct::Research(path) => {
                research(world, console, actor, &path);
            }
            ConsoleAct::Enqueue(path) => queue(world, console, actor, &path, true),
            ConsoleAct::Dequeue(path) => queue(world, console, actor, &path, false),
        }
    }
}

/// tg `/obj/machinery/computer/rdconsole/proc/research_node`.
fn research(world: &mut World, console: Entity, actor: Entity, path: &str) -> bool {
    let (Some(db), Some(web)) = (
        world.get_resource::<ResearchDb>(),
        world.get_resource::<Techweb>(),
    ) else {
        machine_say(
            world,
            console,
            "Не удалось разблокировать узел: технология не найдена!",
        );
        return false;
    };
    let node = db.node_index.get(path).copied();
    let failure = match node {
        Some(n) if !web.available.contains(&n) => {
            Some("Не удалось разблокировать узел: технология недоступна!")
        }
        Some(n) if web.researched.contains(&n) => {
            Some("Не удалось разблокировать узел: технология уже изучена!")
        }
        None => Some("Не удалось разблокировать узел: технология недоступна!"),
        Some(n) if !web.can_afford(&web.price(db, n)) => {
            Some("Недостаточно очков для исследования...")
        }
        Some(_) => None,
    };
    if let Some(text) = failure {
        machine_say(world, console, text);
        return false;
    }
    let node = node.expect("checked above");
    let price = web.price(db, node);
    let researched = world.resource_scope(|world, mut web: Mut<Techweb>| {
        let db = world.resource::<ResearchDb>();
        web.research_node(db, node, false, true)
    });
    if !researched {
        machine_say(
            world,
            console,
            "Не удалось исследовать узел: внутренняя ошибка базы данных!",
        );
        return false;
    }
    world.try_resource_scope(|world, mut economy: Mut<Economy>| {
        pay_bounty(world.resource::<ResearchDb>(), &mut economy);
    });
    let name = world.resource::<ResearchDb>().node_name(node).to_owned();
    info!("research: {name} ({path}) researched by {actor:?}");
    machine_say(world, console, &format!("Успешно исследовано {name}."));
    let researcher = world
        .get::<Inventory>(actor)
        .and_then(|i| i.id)
        .and_then(|id| crate::inventory::get_id(world, id))
        .and_then(|id| world.get::<crate::outfit::IdRegistration>(id))
        .map_or_else(|| "Unknown".to_owned(), |r| r.registered_name.clone());
    let location = world
        .get::<ss13_shared::TilePos>(console)
        .map(|p| format!("({},{},{})", p.x, p.y, p.z))
        .unwrap_or_default();
    world.resource_mut::<Techweb>().logs.push(ResearchLog {
        node_name: name,
        node_cost: price
            .iter()
            .find(|(t, _)| t == POINT_GENERIC)
            .map_or(0.0, |(_, n)| *n),
        node_researcher: researcher,
        node_research_location: location,
    });
    true
}

/// tg `enqueue_node` / `dequeue_node` of the console.
fn queue(world: &mut World, console: Entity, actor: Entity, path: &str, enqueue: bool) {
    let (_, worn) = id_access(world, actor);
    let ok = world.try_resource_scope(|world, mut web: Mut<Techweb>| {
        let Some(&node) = world
            .get_resource::<ResearchDb>()
            .and_then(|db| db.node_index.get(path))
        else {
            return false;
        };
        if !web.available.contains(&node) || web.researched.contains(&node) {
            return false;
        }
        if enqueue {
            web.enqueue_node(node, actor, worn.iter().any(|a| a == ACCESS_RD));
        } else {
            web.dequeue_node(node, actor);
        }
        true
    }) == Some(true);
    if !ok {
        let text = if enqueue {
            "Не удалось поставить узел в очередь: технология не найдена, либо узел уже исследован \
             или он недоступен!"
        } else {
            "Не удалось убрать узел из очереди: технология не найдена, либо узел уже исследован или \
             он недоступен!"
        };
        machine_say(world, console, text);
    }
}
