//! The R&D console (server), after tg `/obj/machinery/computer/rdconsole` (`rdconsole.dm`) with
//! tgui's `Techweb` interface: a click with an empty hand opens the window.
//!
//! Every act but `toggleLock` is refused while the console is locked (BandaStation locks the
//! base type; MetaStation maps the `unlocked` subtype). `toggleLock` needs the console's
//! `req_access` (research). `researchNode` follows the console's `research_node` checks
//! (available, not researched, affordable) before the techweb's, pays the science budget its
//! bounty and logs the research; `enqueueNode`/`dequeueNode` manage the techweb's research queue.
//! An unpowered console is update-only and refuses acts.
//!
//! Disks (`research_disk.dm`, `_design_disk.dm`): a tech disk (`/obj/item/disk/tech_disk`, the
//! starting nodes; the CentCom one every node) and a design disk (`/obj/item/disk/design_disk`;
//! limb disks their `blueprints`, the BEPIS disk a random experimental node's designs) go into
//! the console's two slots (`item_interaction`). `uploadDisk` researches the tech disk's nodes
//! (`absorb_techdisk`) or adds the design disk's designs (the BEPIS disk also unhides and
//! researches its node); `loadTech` copies the researched nodes onto the tech disk; tech disk
//! copies share one 5 s cooldown (`cooldowncopy`); `ejectDisk` drops the disk.
// shortcut: no emagging, multitool relinking or the experiments section of the nodes; the disk
// menu is a header section, not a routed view.

use std::collections::BTreeSet;
use std::sync::Arc;

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::research::{SHEET_MATERIAL_AMOUNT, build_type, node_flags};
use ss13_mapload::{DmName, DmObj};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
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
use crate::combat::CombatRng;
use crate::inventory::{CarriedBy, IdCard, Inventory, check_access};
use crate::obj_damage::is_under;
use crate::power::Unpowered;
use crate::storage::{put_inside, take_out};
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
    /// `t_disk`: the inserted tech disk.
    pub t_disk: Option<Entity>,
    /// `d_disk`: the inserted design disk.
    pub d_disk: Option<Entity>,
}

pub const DISK_ROOT: &str = "/obj/item/disk";
pub const TECH_DISK_ROOT: &str = "/obj/item/disk/tech_disk";
const TECH_DISK_DEBUG: &str = "/obj/item/disk/tech_disk/debug";
pub const DESIGN_DISK_ROOT: &str = "/obj/item/disk/design_disk";
pub const BEPIS_DISK_ROOT: &str = "/obj/item/disk/design_disk/bepis";
/// `cooldowncopy`, seconds.
const COPY_COOLDOWN: f64 = 5.0;

/// `/obj/item/disk/tech_disk`'s `stored_nodes`.
#[derive(Component, Clone, Debug, Default)]
pub struct TechDisk {
    pub stored_nodes: BTreeSet<usize>,
}

/// `/obj/item/disk/design_disk`'s `blueprints`, and the BEPIS disk's `bepis_node`.
#[derive(Component, Clone, Debug, Default)]
pub struct DesignDisk {
    pub blueprints: Vec<usize>,
    pub bepis_node: Option<usize>,
}

/// The disk's `Initialize`, on first use: its data by its type. False when `disk` is not a tech
/// or design disk.
pub fn ensure_disk(world: &mut World, disk: Entity) -> bool {
    if world.get::<TechDisk>(disk).is_some() || world.get::<DesignDisk>(disk).is_some() {
        return true;
    }
    let Some(path) = world.get::<DmObj>(disk).map(|o| o.path.clone()) else {
        return false;
    };
    let Some(db) = world.get_resource::<ResearchDb>() else {
        return false;
    };
    if is_under(&path, TECH_DISK_ROOT) {
        let stored_nodes = if is_under(&path, TECH_DISK_DEBUG) {
            (0..db.nodes.len()).collect()
        } else {
            db.starting.iter().copied().collect()
        };
        world.entity_mut(disk).insert(TechDisk { stored_nodes });
        return true;
    }
    if !is_under(&path, DESIGN_DISK_ROOT) {
        return false;
    }
    let design = |p: &String| db.design_index.get(p).copied();
    let mut state = DesignDisk {
        blueprints: db
            .limb_disks
            .get(&path)
            .map(|b| b.iter().filter_map(design).collect())
            .unwrap_or_default(),
        bepis_node: None,
    };
    if is_under(&path, BEPIS_DISK_ROOT) {
        let experimental: Vec<usize> = db
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.flags & node_flags::EXPERIMENTAL != 0)
            .map(|(i, _)| i)
            .collect();
        if !experimental.is_empty() {
            let high = experimental.len() as u32 - 1;
            let pick = world
                .get_resource_mut::<CombatRng>()
                .map_or(0, |mut r| r.range(0, high)) as usize;
            let node = experimental[pick];
            let db = world.resource::<ResearchDb>();
            state.blueprints = db.node_designs[node].clone();
            state.bepis_node = Some(node);
        }
    }
    world.entity_mut(disk).insert(state);
    true
}

pub struct ConsolePlugin;

impl Plugin for ConsolePlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<TechwebUi>()
            .add_systems(ss13_world::MapInit, setup_consoles.after(ResearchLoaded))
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
    let mut query = world.query_filtered::<(Entity, &DmObj), With<ss13_world::Mapped>>();
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
        t_disk: None,
        d_disk: None,
    };
    world.entity_mut(e).insert(console);
}

/// The techweb state of a modular computer's Science Hub (`/datum/computer_file/program/science`):
/// unlocked, `lock_access = ACCESS_RESEARCH`.
pub(crate) fn program_console(world: &mut World, pc: Entity) {
    let Some(db) = world.get_resource::<ResearchDb>() else {
        return;
    };
    let catalogue = Arc::new(catalogue(db, world.get_resource::<ResearchIcons>()));
    world.entity_mut(pc).insert(RdConsole {
        locked: false,
        req_access: vec!["research".to_owned()],
        catalogue,
        t_disk: None,
        d_disk: None,
    });
}

fn plain(name: &str) -> &str {
    name.trim_start_matches("\\improper ")
        .trim_start_matches("\\proper ")
}

/// tg `ui_interact` on `attack_hand`.
fn open_console(
    mut clicks: MessageReader<ClickResolved>,
    // A modular computer's Science Hub opens from its program (`crate::modpc`).
    consoles: Query<
        &DmName,
        (
            With<RdConsole>,
            Without<Unpowered>,
            Without<crate::modpc::pda::ModularComputer>,
        ),
    >,
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
    Query<'static, 'static, (Option<&'static TechDisk>, Option<&'static DesignDisk>)>,
);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConsoleAct {
    ToggleLock,
    Research(String),
    Enqueue(String),
    Dequeue(String),
    /// `ejectDisk`: true for the tech disk.
    EjectDisk(bool),
    /// `uploadDisk`: true for the tech disk.
    UploadDisk(bool),
    LoadTech,
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
        // Filled by the caller from the console's disks.
        t_disk: None,
        d_disk: None,
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
        (db, web, names, disks): &SystemParamItem<'_, '_, ConsoleParams>,
    ) -> TechwebData {
        let mut data = console_data(console, user, db.as_deref(), web.as_deref(), names);
        data.t_disk = console
            .t_disk
            .and_then(|d| disks.get(d).ok())
            .and_then(|(t, _)| t)
            .map(|t| t.stored_nodes.iter().map(|&n| n as u16).collect());
        data.d_disk = console
            .d_disk
            .and_then(|d| disks.get(d).ok())
            .and_then(|(_, d)| d)
            .map(|d| {
                d.blueprints
                    .iter()
                    .map(|&b| {
                        db.as_deref()
                            .and_then(|db| db.designs.get(b))
                            .map(|d| d.name.clone())
                            .unwrap_or_default()
                    })
                    .collect()
            });
        data
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
            EJECT_DISK | UPLOAD_DISK => {
                let tech = match params.get(DISK_TYPE) {
                    Some(TECH_DISK) => true,
                    Some(DESIGN_DISK) => false,
                    _ => return Err(UiReject::BadParams("type: tech or design".into())),
                };
                Ok(if action == EJECT_DISK {
                    ConsoleAct::EjectDisk(tech)
                } else {
                    ConsoleAct::UploadDisk(tech)
                })
            }
            LOAD_TECH => Ok(ConsoleAct::LoadTech),
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

fn console_acts(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<TechwebUi>>>,
    // `STATIC_COOLDOWN_DECLARE(cooldowncopy)`: game time the next tech disk copy may start.
    mut copy_ready: Local<f64>,
) {
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
            ConsoleAct::EjectDisk(tech) => {
                eject_disk(world, console, tech);
            }
            ConsoleAct::UploadDisk(false) => upload_design_disk(world, console),
            ConsoleAct::UploadDisk(true) => tech_copy(world, console, false, &mut copy_ready),
            ConsoleAct::LoadTech => tech_copy(world, console, true, &mut copy_ready),
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

/// `uploadDisk` with the tech disk (`load` false) and `loadTech` (true), behind `cooldowncopy`.
fn tech_copy(world: &mut World, console: Entity, load: bool, copy_ready: &mut f64) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now < *copy_ready {
        machine_say(world, console, "Серверы заняты!");
        return;
    }
    let Some(disk) = live_disk(world, console, true) else {
        machine_say(world, console, "Технологический диск не вставлен!");
        return;
    };
    *copy_ready = now + COPY_COOLDOWN;
    if load {
        machine_say(world, console, "Загрузка на технологический диск.");
        let researched = world
            .get_resource::<Techweb>()
            .map(|w| w.researched.clone())
            .unwrap_or_default();
        if let Some(mut t) = world.get_mut::<TechDisk>(disk) {
            t.stored_nodes.extend(researched);
        }
    } else {
        machine_say(world, console, "Загрузка технологии на диск.");
        absorb_techdisk(world, disk);
    }
}

/// The console's tech (or design) disk while it still exists (`QDELETED` clears it).
fn live_disk(world: &mut World, console: Entity, tech: bool) -> Option<Entity> {
    let state = world.get::<RdConsole>(console)?;
    let disk = if tech { state.t_disk } else { state.d_disk }?;
    let present = world.get_entity(disk).is_ok()
        && if tech {
            world.get::<TechDisk>(disk).is_some()
        } else {
            world.get::<DesignDisk>(disk).is_some()
        };
    if !present && let Some(mut c) = world.get_mut::<RdConsole>(console) {
        if tech {
            c.t_disk = None;
        } else {
            c.d_disk = None;
        }
    }
    present.then_some(disk)
}

/// tg `absorb_techdisk`: every node on the disk not yet researched, forced and free.
pub fn absorb_techdisk(world: &mut World, disk: Entity) {
    let Some(nodes) = world.get::<TechDisk>(disk).map(|t| t.stored_nodes.clone()) else {
        return;
    };
    world.try_resource_scope(|world, mut web: Mut<Techweb>| {
        let db = world.resource::<ResearchDb>();
        for node in nodes {
            if node < db.nodes.len() && !web.researched.contains(&node) {
                web.research_node(db, node, true, false);
            }
        }
    });
}

/// `uploadDisk` with the design disk: its designs, then `on_upload`.
fn upload_design_disk(world: &mut World, console: Entity) {
    let Some(disk) = live_disk(world, console, false) else {
        machine_say(world, console, "Диск с дизайном не вставлен!");
        return;
    };
    let Some(state) = world.get::<DesignDisk>(disk).cloned() else {
        return;
    };
    world.try_resource_scope(|world, mut web: Mut<Techweb>| {
        let db = world.resource::<ResearchDb>();
        for &design in &state.blueprints {
            if design < db.designs.len() {
                web.add_design(db, design);
            }
        }
    });
    machine_say(world, console, "Загрузка чертежей с диска.");
    // `/obj/item/disk/design_disk/bepis/on_upload`.
    if let Some(node) = state.bepis_node {
        world.try_resource_scope(|world, mut web: Mut<Techweb>| {
            let db = world.resource::<ResearchDb>();
            web.hidden.remove(&node);
            web.research_node(db, node, true, false);
        });
    }
}

/// tg `eject_disk(type)`: the disk drops on the console's tile.
pub fn eject_disk(world: &mut World, console: Entity, tech: bool) -> bool {
    let Some(state) = world.get::<RdConsole>(console) else {
        return false;
    };
    let Some(disk) = (if tech { state.t_disk } else { state.d_disk }) else {
        return false;
    };
    if let Some(mut c) = world.get_mut::<RdConsole>(console) {
        if tech {
            c.t_disk = None;
        } else {
            c.d_disk = None;
        }
    }
    let Some(&pos) = world.get::<TilePos>(console) else {
        return false;
    };
    if world.get_entity(disk).is_err() {
        return false;
    }
    take_out(world, disk, pos);
    crate::construction::tile_changed(world, pos);
    true
}

/// tg `rdconsole/item_interaction`: a tech or design disk into its slot. True when the click
/// was the console's.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world
        .get::<crate::modpc::pda::ModularComputer>(target)
        .is_some()
    {
        return false;
    }
    let Some(state) = world.get::<RdConsole>(target) else {
        return false;
    };
    let (t_disk, d_disk) = (state.t_disk, state.d_disk);
    if !world
        .get::<DmObj>(held)
        .is_some_and(|o| is_under(&o.path, DISK_ROOT))
    {
        return false;
    }
    if !ensure_disk(world, held) {
        tell(
            world,
            client,
            Span::Warning,
            "Консоль не принимает диски такого формата.",
        );
        return true;
    }
    let tech = world.get::<TechDisk>(held).is_some();
    let occupied = if tech { t_disk } else { d_disk };
    if occupied.is_some_and(|d| world.get_entity(d).is_ok()) {
        tell(
            world,
            client,
            Span::Warning,
            "Диск с технологией уже загружен!",
        );
        return true;
    }
    let name = world
        .get::<DmName>(held)
        .map(|n| n.name.clone())
        .unwrap_or_default();
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        for hand in inventory.hands.iter_mut() {
            if *hand == Some(held) {
                *hand = None;
            }
        }
    }
    world.entity_mut(held).remove::<CarriedBy>();
    put_inside(world, held, target);
    if let Some(mut c) = world.get_mut::<RdConsole>(target) {
        if tech {
            c.t_disk = Some(held);
        } else {
            c.d_disk = Some(held);
        }
    }
    tell(
        world,
        client,
        Span::Notice,
        format!(
            "Вы вставляете {name} в {}!",
            plain(&console_name(world, target))
        ),
    );
    true
}

fn console_name(world: &World, console: Entity) -> String {
    world
        .get::<DmName>(console)
        .map(|n| n.name.clone())
        .unwrap_or_default()
}
