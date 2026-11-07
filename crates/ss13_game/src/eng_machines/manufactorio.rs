//! Manufactorio production lines (server): tg `/obj/machinery/power/manufacturing`
//! (`code/modules/manufactorio`), built from their boards (`crate::machine_frame`).
//!
//! - Base ([`send_resource`], [`receive_resource`]): a machine passes an item one tile along a
//!   direction: into the manufacturing machine standing there (anchored only), else onto the
//!   tile unless it is blocked by something dense or holds 10 objects already. Stacks merge into
//!   a stack inside when there is room (`may_merge_in_contents_and_do_so`). An anchored machine
//!   is a node on its tile's cable and draws its costs from that net's surplus. A wrench (2 s)
//!   fastens it, alt-click turns a loose one counterclockwise (`simple_rotation`), a screwdriver
//!   and crowbar take it apart. Every machine ticks once a second (`SSmanufacturing`).
//! - Crusher: takes items (not ore) from behind into 5 slots; each tick, 3 kW, it crushes the
//!   last one into its materials: whole sheets go out forward, fractions of a sheet are kept
//!   until they add up. Smelter: takes items from behind into 5 slots; each tick, 4 kW, up to 5
//!   ore become their refined sheets, other items pass through; output forward.
//! - Router: what it receives goes out of one of its other three sides in turn (stacks one at a
//!   time), never back and never to another router; a multitool from a side toggles that output.
//!   Sorter (not dense): what lies on it goes to the direction of the first filter it matches
//!   (or fails, when inverted), else to its unmet direction. Its window (`ManufacturingSorter`,
//!   an empty hand or a multitool) adds up to 10 filters from a list, turns their sides and the
//!   unmet one, moves, deletes and edits them (a name text; an item filter waits for the item
//!   the sorter is hit with).
//! - Lathe: takes material stacks (not ore) from behind into its storage (50 sheets a matter
//!   bin tier); a multitool picks an autolathe design; while 5 kW are there and the materials
//!   last, it prints it every `(construction_time * lathe_time_factor) ** 0.8` ds, forward.
//! - Storage unit: holds up to 16 things (no closet with contents); an empty hand out of combat
//!   mode dumps it (1.25 s), a screwdriver (5 s) breaks it into 10 iron sheets. Unloader: takes
//!   one openable crate or ore box from behind (2 kW); each tick, 2 kW, its contents go forward
//!   (a crate fills first if it never was), then the empty crate goes to its side (right, or
//!   left once flipped with a multitool) and opens.
//!
//! Cost: one pass over the manufacturing machines per second (a handful per line), each a
//! tile lookup or two.
// shortcut: items reach a line only from machines and the tiles they read (no conveyors are
// ported: conveyor `Bumped` intake moves with the conveyor entry), no I/O overlays.
// shortcut: the crusher knows the materials of stacks only (`ResearchDb::stacks`: rods, sheets,
// tiles); other items have no `custom_materials` extracted and are crushed to nothing; no
// boulders, no mobs crushed. The smelter does not burn what passes through (`fire_act`).
// shortcut: the sorter moves items only, once a second (tg 0.2 s after entry); the lathe
// prints only plain-material autolathe designs, finishes on the tick after its time and has no
// ctrl-shift dump, print sound or examine lines. No crafter: crafting recipes are not ported.

use bevy::prelude::*;
use ss13_mapload::{DmItem, DmName, DmObj, DmOre, DmPower, DmStack};
use ss13_shared::chat::Span;
use ss13_shared::{Dir, Facing, TilePos};
use ss13_world::SpatialIndex;

use bevy::ecs::message::MessageCursor;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use super::EngJob;
use crate::construction::Job;
use crate::manufactorio_ui::*;
use crate::obj_damage::{is_under, set_icon_state};
use crate::power::{Grid, NodeKind};
use crate::science::lathe::{MaterialStore, design_cost};
use crate::science::techweb::ResearchDb;
use crate::storage::{Stored, StoredIn, put_inside, take_out};
use crate::tools::{Actor, Stack, ToolKind, stack_amount, tell, tool_of, use_tool};

pub const ROOT: &str = "/obj/machinery/power/manufacturing";
pub const CRUSHER: &str = "/obj/machinery/power/manufacturing/crusher";
pub const SMELTER: &str = "/obj/machinery/power/manufacturing/smelter";
pub const ROUTER: &str = "/obj/machinery/power/manufacturing/router";
pub const SORTER: &str = "/obj/machinery/power/manufacturing/sorter";
pub const STORAGE: &str = "/obj/machinery/power/manufacturing/storagebox";
pub const UNLOADER: &str = "/obj/machinery/power/manufacturing/unloader";
pub const LATHE: &str = "/obj/machinery/power/manufacturing/lathe";
const ORE: &str = "/obj/item/stack/ore";
const MAIL: &str = "/obj/item/mail";
const IRON: &str = "/obj/item/stack/sheet/iron";
const ORE_BOX: &str = crate::mining::rocks::ORE_BOX;

/// `SSmanufacturing.wait`.
pub const TICK_SECS: f32 = 1.0;
/// `MANUFACTURING_TURF_LAG_LIMIT`.
pub const TURF_LIMIT: usize = 10;
/// `SHEET_MATERIAL_AMOUNT`.
const SHEET: f64 = 100.0;
/// Costs as tg adds them to the net (`KILO WATTS` per call).
pub const CRUSH_COST: f64 = 3000.0;
pub const SMELT_COST: f64 = 4000.0;
pub const UNLOAD_COST: f64 = 2000.0;
pub const LATHE_COST: f64 = 5000.0;
/// `max_filters` of the sorter.
pub const MAX_FILTERS: usize = 10;
/// `tgui_input_text(..., 12)` of `name_contains`.
const FILTER_TEXT_LEN: u32 = 12;
/// `SHEET_MATERIAL_AMOUNT * MAX_STACK_SIZE` per matter bin tier.
const UNITS_PER_BIN: f64 = SHEET * 50.0;
/// `capacity` of the crusher, the smelter's 5, `max_stuff` of the storage unit.
pub const CRUSHER_SLOTS: usize = 5;
pub const SMELTER_SLOTS: usize = 5;
pub const STORAGE_SLOTS: usize = 16;
/// Ore a smelter refines per tick.
pub const SMELT_BATCH: u32 = 5;
/// Deciseconds: `default_unfasten_wrench`, the storage unit's dump and its screwdriver.
const WRENCH_DS: f32 = 20.0;
const DUMP_DS: f32 = 12.5;
const BREAK_DS: f32 = 50.0;
/// `storagebox/atom_destruction`: its `custom_materials` back.
const STORAGE_SHEETS: u32 = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Crusher,
    Smelter,
    Router,
    Sorter,
    Storage,
    Unloader,
    Lathe,
}

impl Kind {
    pub fn of_path(path: &str) -> Option<Self> {
        [
            (CRUSHER, Kind::Crusher),
            (SMELTER, Kind::Smelter),
            (ROUTER, Kind::Router),
            (SORTER, Kind::Sorter),
            (STORAGE, Kind::Storage),
            (UNLOADER, Kind::Unloader),
            (LATHE, Kind::Lathe),
        ]
        .into_iter()
        .find(|(root, _)| is_under(path, root))
        .map(|(_, k)| k)
    }
}

/// `/datum/sortrouter_filter` subtypes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FilterKind {
    IsStack,
    IsOre,
    IsMail,
    /// `name_contains` (lowercase).
    NameContains(String),
    /// `is_path_specific`: exactly this type.
    Path(String),
    /// `is_path_specific/subtypes`.
    Subtypes(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SortFilter {
    /// The window's `ref`.
    pub id: u32,
    /// `return_name()`.
    pub name: String,
    pub kind: FilterKind,
    pub dir: Dir,
    pub inverted: bool,
}

/// A manufacturing machine's live state.
#[derive(Component, Clone, Debug)]
pub struct Manufacturing {
    pub kind: Kind,
    /// On its tile's cable net (`connect_to_network`).
    pub connected: bool,
    /// Output held inside because the way out was blocked (`withholding`, `withheld`).
    pub withheld: Option<Entity>,
    /// Crusher: sheet type -> sheets below a whole one (`held_mats`).
    pub held_mats: Vec<(String, f64)>,
    /// Router: outputs off (`disabled_dirs`), outputs left this round (`directions`).
    pub disabled: Vec<Dir>,
    pub directions: Vec<Dir>,
    /// Unloader: crates go to the left instead of the right.
    pub flip: bool,
    /// Sorter: its filters in order, and `dir_if_not_met`.
    pub filters: Vec<SortFilter>,
    pub unmet: Dir,
    /// Sorter: the next filter id, the item filter waiting for an item (`currently_listening`).
    pub next_id: u32,
    pub awaiting: Option<u32>,
    /// Lathe: the chosen design (`chosen_design_path`, a [`ResearchDb`] index) and when the
    /// running print ends (`busy`).
    pub design: Option<usize>,
    pub busy_until: Option<f64>,
}

impl Manufacturing {
    pub fn new(kind: Kind, dir: Dir) -> Self {
        Self {
            kind,
            connected: false,
            withheld: None,
            held_mats: Vec::new(),
            disabled: Vec::new(),
            directions: Dir::CARDINALS.to_vec(),
            flip: false,
            filters: Vec::new(),
            unmet: dir,
            next_id: 0,
            awaiting: None,
            design: None,
            busy_until: None,
        }
    }
}

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // Wrench, multitool (router outputs, unloader side) and the storage unit's screwdriver,
    // before `crate::machine_frame` takes machines apart.
    app.add_click_handler(ClickPhase::HeldOnTarget, 1549, |w, c| {
        c.held
            .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target))
    });
    // `storagebox/attack_hand`: dumping it.
    app.add_click_handler(ClickPhase::EmptyHand, 1509, |w, c| {
        attack_hand(w, c.client, c.mob, c.target)
    });
    app.register_ui::<ManufacturingSorterUi>().add_systems(
        Update,
        (
            sorter_acts.in_set(UiSystems::Handle),
            prompt_answers.after(crate::click::ResolveClicks),
        ),
    );
    app.add_systems(
        ss13_world::MapInit,
        setup_mapped.after(crate::power::init_power),
    )
    .add_systems(
        Update,
        tick.after(crate::power::PowerSystems)
            .run_if(any_with_component::<Manufacturing>),
    );
}

fn setup_mapped(world: &mut World) {
    let mut q = world.query_filtered::<(Entity, &DmObj), With<ss13_world::Mapped>>();
    let found: Vec<Entity> = q
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, ROOT))
        .map(|(e, _)| e)
        .collect();
    for e in found {
        setup(world, e);
    }
}

/// `Initialize`; also for one built from a frame (`crate::machine_frame::setup_built`).
pub fn setup(world: &mut World, e: Entity) {
    let Some(kind) = world.get::<DmObj>(e).and_then(|o| Kind::of_path(&o.path)) else {
        return;
    };
    let dir = dir_of(world, e);
    world.entity_mut(e).insert(Manufacturing::new(kind, dir));
    if kind == Kind::Lathe {
        let materials = world
            .get_resource::<ResearchDb>()
            .map_or(0, |db| db.materials.len());
        let store = MaterialStore::new(materials, Some(lathe_capacity(world, e)));
        world.entity_mut(e).insert(store);
    }
    sync_grid(world, e);
}

pub fn dir_of(world: &World, e: Entity) -> Dir {
    world.get::<Facing>(e).map_or(Dir::South, |f| f.0)
}

use crate::obj_damage::anchored;

/// tg `turn(dir, 90)`.
pub fn ccw(dir: Dir) -> Dir {
    match dir {
        Dir::North => Dir::West,
        Dir::West => Dir::South,
        Dir::South => Dir::East,
        Dir::East => Dir::North,
        other => other,
    }
}

/// tg `turn(dir, -90)`.
pub fn cw(dir: Dir) -> Dir {
    ccw(ccw(ccw(dir)))
}

/// `connect_to_network` when anchored, `disconnect_from_network` when not.
fn sync_grid(world: &mut World, e: Entity) {
    let Some(state) = world.get::<Manufacturing>(e) else {
        return;
    };
    let want = anchored(world, e);
    if state.connected == want {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    let layer = world
        .get::<DmPower>(e)
        .map_or(2, |p| p.cable_layer.clamp(0.0, 255.0) as u8);
    if let Some(mut grid) = world.get_resource_mut::<Grid>() {
        if want {
            grid.add_node(e, pos, layer, NodeKind::Machine);
        } else {
            grid.remove_node(e);
        }
    }
    if let Some(mut m) = world.get_mut::<Manufacturing>(e) {
        m.connected = want;
    }
}

/// tg `surplus()` of the machine's net.
pub fn surplus(world: &World, e: Entity) -> f64 {
    let Some(grid) = world.get_resource::<Grid>() else {
        return 0.0;
    };
    grid.node_net(e)
        .and_then(|n| grid.net(n))
        .map_or(0.0, |n| n.surplus())
}

/// tg `add_load()`.
fn add_load(world: &mut World, e: Entity, amount: f64) {
    let Some(mut grid) = world.get_resource_mut::<Grid>() else {
        return;
    };
    if let Some(id) = grid.node_net(e)
        && let Some(net) = grid.net_mut(id)
    {
        net.load += amount;
    }
}

fn kind_of(world: &World, e: Entity) -> Option<Kind> {
    world.get::<Manufacturing>(e).map(|m| m.kind)
}

/// `contents - circuit` without the held output.
pub fn contents(world: &World, m: Entity) -> Vec<Entity> {
    let withheld = world.get::<Manufacturing>(m).and_then(|s| s.withheld);
    world
        .get::<Stored>(m)
        .map(|s| {
            s.items()
                .iter()
                .copied()
                .filter(|&e| Some(e) != withheld)
                .collect()
        })
        .unwrap_or_default()
}

fn item_path(world: &World, e: Entity) -> Option<&str> {
    world
        .get::<DmItem>(e)
        .map(|i| i.path.as_str())
        .or_else(|| world.get::<DmObj>(e).map(|o| o.path.as_str()))
}

fn merge_key(world: &World, e: Entity) -> Option<String> {
    world.get::<Stack>(e)?;
    let path = world.get::<DmItem>(e)?.path.clone();
    Some(
        world
            .get::<DmStack>(e)
            .map(|s| s.merge_type.clone())
            .filter(|m| !m.is_empty())
            .unwrap_or(path),
    )
}

fn max_amount(world: &World, e: Entity) -> u32 {
    world.get::<DmStack>(e).map_or(50, |s| s.max_amount.max(1))
}

/// `may_merge_in_contents_and_do_so`: into a stack inside that has room for all of it.
pub fn merge_into_contents(world: &mut World, m: Entity, item: Entity) -> bool {
    let Some(key) = merge_key(world, item) else {
        return false;
    };
    let amount = stack_amount(world, item);
    let Some(target) = contents(world, m).into_iter().find(|&o| {
        o != item
            && merge_key(world, o).as_deref() == Some(key.as_str())
            && stack_amount(world, o) + amount <= max_amount(world, o)
    }) else {
        return false;
    };
    if let Some(mut s) = world.get_mut::<Stack>(target) {
        s.amount += amount;
    }
    world.despawn(item);
    true
}

fn machine_on(world: &World, pos: TilePos) -> Option<Entity> {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| world.get::<Manufacturing>(e).is_some())
}

/// `get_overfloor_objects`: objects and items on the tile, not mobs.
fn overfloor(world: &World, pos: TilePos, except: Entity) -> usize {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .filter(|&&e| {
            e != except
                && world.get::<crate::health::Health>(e).is_none()
                && (world.get::<DmItem>(e).is_some() || world.get::<DmObj>(e).is_some())
        })
        .count()
}

/// `is_blocked_turf(exclude_mobs = TRUE)`.
fn blocked(world: &World, pos: TilePos) -> bool {
    world
        .resource::<SpatialIndex>()
        .dense_on_tile(pos)
        .iter()
        .any(|&e| world.get::<crate::health::Health>(e).is_none())
}

/// `send_resource(sending, dir)`: `item` one tile towards `dir`. `None` only checks.
pub fn send_resource(world: &mut World, machine: Entity, item: Option<Entity>, dir: Dir) -> bool {
    let Some(&pos) = world.get::<TilePos>(machine) else {
        return false;
    };
    let next = pos.step(dir);
    if let Some(target) = machine_on(world, next) {
        if !anchored(world, target) {
            return false;
        }
        let Some(item) = item else {
            return true;
        };
        // tg passes the direction of travel here but the direction to the source from
        // `Bumped`; every receiver reads it as the side it came from.
        return receive_resource(world, target, item, Some(machine), dir.reverse());
    }
    if blocked(world, next) || overfloor(world, next, machine) >= TURF_LIMIT {
        return false;
    }
    if let Some(item) = item {
        take_out(world, item, next);
    }
    true
}

/// `receive_resource`: `item` offered from the side `from_dir`. True when taken.
pub fn receive_resource(
    world: &mut World,
    machine: Entity,
    item: Entity,
    from: Option<Entity>,
    from_dir: Dir,
) -> bool {
    let Some(kind) = kind_of(world, machine) else {
        return false;
    };
    let dir = dir_of(world, machine);
    let behind = from_dir == dir.reverse();
    let is_item = world.get::<DmItem>(item).is_some();
    match kind {
        Kind::Crusher => {
            let ore = item_path(world, item).is_some_and(|p| is_under(p, ORE));
            if ore || !is_item || surplus(world, machine) < CRUSH_COST || !behind {
                return false;
            }
            // tg merges only when it fails to (`&&` without `!`); the smelter's check is meant.
            take_into(world, machine, item, CRUSHER_SLOTS)
        }
        Kind::Smelter => {
            if !is_item || surplus(world, machine) < SMELT_COST || !behind {
                return false;
            }
            take_into(world, machine, item, SMELTER_SLOTS)
        }
        Kind::Storage => {
            let full_closet = world.get::<crate::closets::Closet>(item).is_some()
                && world
                    .get::<Stored>(item)
                    .is_some_and(|s| !s.items().is_empty());
            !full_closet && take_into(world, machine, item, STORAGE_SLOTS)
        }
        Kind::Unloader => receive_crate(world, machine, item, behind),
        Kind::Lathe => {
            let ore = item_path(world, item).is_some_and(|p| is_under(p, ORE));
            if ore || world.get::<Stack>(item).is_none() || !behind {
                return false;
            }
            lathe_insert(world, machine, item)
        }
        Kind::Router => route(world, machine, item, from, from_dir),
        Kind::Sorter => {
            let Some(&pos) = world.get::<TilePos>(machine) else {
                return false;
            };
            if overfloor(world, pos, machine) >= TURF_LIMIT {
                return false;
            }
            take_out(world, item, pos);
            true
        }
    }
}

/// Into the machine's contents when a slot is free or it merges into a stack inside.
fn take_into(world: &mut World, machine: Entity, item: Entity, slots: usize) -> bool {
    if contents(world, machine).len() >= slots {
        return merge_into_contents(world, machine, item);
    }
    put_inside(world, item, machine);
    true
}

fn receive_crate(world: &mut World, machine: Entity, item: Entity, behind: bool) -> bool {
    if surplus(world, machine) < UNLOAD_COST || !behind || !contents(world, machine).is_empty() {
        return false;
    }
    let openable = match world.get::<crate::closets::Closet>(item) {
        Some(c) => c.can_open(),
        None => item_path(world, item).is_some_and(|p| is_under(p, ORE_BOX)),
    };
    if !openable {
        return false;
    }
    put_inside(world, item, machine);
    true
}

/// `router/receive_resource`.
fn route(
    world: &mut World,
    machine: Entity,
    item: Entity,
    from: Option<Entity>,
    from_dir: Dir,
) -> bool {
    if from.is_some_and(|f| kind_of(world, f) == Some(Kind::Router)) {
        return false;
    }
    let Some(state) = world.get::<Manufacturing>(machine).cloned() else {
        return false;
    };
    let usable = |dirs: &[Dir]| -> Vec<Dir> {
        dirs.iter()
            .copied()
            .filter(|d| *d != from_dir && !state.disabled.contains(d))
            .collect()
    };
    let mut directions = state.directions.clone();
    let mut filtered = usable(&directions);
    if filtered.is_empty() {
        // tg resets but tries nothing this time; the reset round is meant to be used.
        directions = Dir::CARDINALS.to_vec();
        filtered = usable(&directions);
    }
    let mut sent = false;
    for target in filtered {
        directions.retain(|d| *d != target);
        // `handle_stack`: one at a time; the rest stays with the sender (tg loses it).
        let amount = stack_amount(world, item);
        let piece = if world.get::<Stack>(item).is_some() && amount > 1 {
            split_one(world, item)
        } else {
            Some(item)
        };
        let Some(piece) = piece else {
            break;
        };
        if send_resource(world, machine, Some(piece), target) {
            sent = piece == item;
            if let Some(mut m) = world.get_mut::<Manufacturing>(machine) {
                m.directions = directions.clone();
            }
            world.entity_mut(machine).insert(Facing(from_dir));
            return sent;
        }
        if piece != item {
            if let Some(mut s) = world.get_mut::<Stack>(item) {
                s.amount += 1;
            }
            world.despawn(piece);
        }
    }
    if let Some(mut m) = world.get_mut::<Manufacturing>(machine) {
        m.directions = directions;
    }
    sent
}

/// `split_stack(1)`: a new stack of one beside the original (inside the router's tile).
fn split_one(world: &mut World, item: Entity) -> Option<Entity> {
    let path = world.get::<DmItem>(item)?.path.clone();
    let pos = world.get::<TilePos>(item).copied().or_else(|| {
        let holder = world.get::<StoredIn>(item)?.0;
        world.get::<TilePos>(holder).copied()
    })?;
    let piece = crate::construction::spawn_obj(world, &path, pos)?;
    world.entity_mut(piece).insert(Stack { amount: 1 });
    if let Some(mut s) = world.get_mut::<Stack>(item) {
        s.amount -= 1;
    }
    Some(piece)
}

/// A new stack of `amount` inside `machine`.
fn make_inside(world: &mut World, machine: Entity, path: &str, amount: u32) -> Option<Entity> {
    let pos = *world.get::<TilePos>(machine)?;
    let e = crate::construction::spawn_obj(world, path, pos)?;
    world.entity_mut(e).insert(Stack { amount });
    put_inside(world, e, machine);
    Some(e)
}

/// The held output out forward; true when nothing is held any more.
fn flush_withheld(world: &mut World, machine: Entity) -> bool {
    let Some(held) = world.get::<Manufacturing>(machine).and_then(|m| m.withheld) else {
        return true;
    };
    if world.get_entity(held).is_err() || world.get::<StoredIn>(held).map(|s| s.0) != Some(machine)
    {
        set_withheld(world, machine, None);
        return true;
    }
    let dir = dir_of(world, machine);
    if !send_resource(world, machine, Some(held), dir) {
        return false;
    }
    set_withheld(world, machine, None);
    true
}

fn set_withheld(world: &mut World, machine: Entity, held: Option<Entity>) {
    if let Some(mut m) = world.get_mut::<Manufacturing>(machine) {
        m.withheld = held;
    }
}

/// The once-a-second pass (`SSmanufacturing`).
fn tick(world: &mut World, mut elapsed: Local<f32>) {
    *elapsed += world.resource::<Time>().delta_secs();
    if *elapsed < TICK_SECS {
        return;
    }
    *elapsed = (*elapsed - TICK_SECS).min(TICK_SECS);
    let machines: Vec<Entity> = world
        .query_filtered::<Entity, With<Manufacturing>>()
        .iter(world)
        .collect();
    for m in machines {
        sync_grid(world, m);
        process(world, m);
    }
}

/// One `process()` of a machine.
pub fn process(world: &mut World, machine: Entity) {
    match kind_of(world, machine) {
        Some(Kind::Crusher) => crush(world, machine),
        Some(Kind::Smelter) => smelt(world, machine),
        Some(Kind::Sorter) => sort(world, machine),
        Some(Kind::Unloader) => unload(world, machine),
        Some(Kind::Lathe) => lathe(world, machine),
        _ => {}
    }
}

/// Sheet type -> sheets of what `item` is made of (stacks only).
fn materials_of(world: &World, item: Entity) -> Vec<(String, f64)> {
    let (Some(path), Some(db)) = (
        world.get::<DmItem>(item).map(|i| i.path.as_str()),
        world.get_resource::<crate::science::techweb::ResearchDb>(),
    ) else {
        return Vec::new();
    };
    let amount = f64::from(stack_amount(world, item));
    db.stacks
        .get(path)
        .into_iter()
        .flatten()
        .filter_map(|&(mat, units)| {
            let sheet = db.materials.get(mat)?.sheet_type.clone()?;
            Some((sheet, units * amount / SHEET))
        })
        .collect()
}

/// `crusher/process`.
fn crush(world: &mut World, machine: Entity) {
    if !flush_withheld(world, machine) {
        return;
    }
    // A whole sheet of what was kept goes out first.
    let whole = world.get_mut::<Manufacturing>(machine).and_then(|mut m| {
        let i = m.held_mats.iter().position(|(_, q)| *q >= 1.0 - 1e-9)?;
        let n = (m.held_mats[i].1 + 1e-9).floor();
        m.held_mats[i].1 -= n;
        let path = m.held_mats[i].0.clone();
        if m.held_mats[i].1 <= 1e-9 {
            m.held_mats.remove(i);
        }
        Some((path, n as u32))
    });
    if let Some((path, n)) = whole {
        let made = make_inside(world, machine, &path, n);
        set_withheld(world, machine, made);
        return;
    }
    let Some(victim) = contents(world, machine).last().copied() else {
        return;
    };
    if surplus(world, machine) < CRUSH_COST {
        return;
    }
    add_load(world, machine, CRUSH_COST);
    let mats = materials_of(world, victim);
    world.despawn(victim);
    let dir = dir_of(world, machine);
    let mut blocked_out = false;
    for (path, sheets) in mats {
        let n = if blocked_out {
            0.0
        } else {
            (sheets + 1e-9).floor()
        };
        let rest = sheets - n;
        if rest > 1e-9
            && let Some(mut m) = world.get_mut::<Manufacturing>(machine)
        {
            match m.held_mats.iter().position(|(p, _)| *p == path) {
                Some(i) => m.held_mats[i].1 += rest,
                None => m.held_mats.push((path.clone(), rest)),
            }
        }
        if n < 1.0 {
            continue;
        }
        let Some(made) = make_inside(world, machine, &path, n as u32) else {
            continue;
        };
        if !send_resource(world, machine, Some(made), dir) {
            set_withheld(world, machine, Some(made));
            // tg drops the materials after a blocked one; they are kept to come out later.
            blocked_out = true;
        }
    }
}

/// `smelter/process`.
fn smelt(world: &mut World, machine: Entity) {
    let held = world.get::<Manufacturing>(machine).and_then(|m| m.withheld);
    if held.is_none()
        && let Some(item) = contents(world, machine).last().copied()
    {
        if surplus(world, machine) >= SMELT_COST {
            set_icon_state(world, machine, "smelter_on");
            add_load(world, machine, SMELT_COST);
            let refined = world
                .get::<DmOre>(item)
                .map(|o| o.refined_type.clone())
                .filter(|r| !r.is_empty());
            let out = match refined {
                Some(sheet) => {
                    let n = stack_amount(world, item).min(SMELT_BATCH);
                    let made = make_inside(world, machine, &sheet, n);
                    crate::tools::use_stack(world, item, n);
                    made
                }
                None => Some(item),
            };
            set_withheld(world, machine, out);
        } else {
            set_icon_state(world, machine, "smelter");
        }
    }
    flush_withheld(world, machine);
}

/// Whether `filter` matches `item` (`meets_conditions`).
pub fn meets(world: &World, filter: &FilterKind, item: Entity) -> bool {
    let path = item_path(world, item).unwrap_or_default();
    match filter {
        FilterKind::IsStack => world.get::<Stack>(item).is_some(),
        FilterKind::IsOre => is_under(path, ORE),
        FilterKind::IsMail => is_under(path, MAIL),
        FilterKind::NameContains(text) => world.get::<DmName>(item).is_some_and(|n| {
            crate::obj_damage::strip(&n.name)
                .to_lowercase()
                .contains(text.as_str())
        }),
        FilterKind::Path(p) => !p.is_empty() && path == p.as_str(),
        // tg `istype(checking.type, value)` on a path; `ispath` is meant.
        FilterKind::Subtypes(p) => !p.is_empty() && is_under(path, p),
    }
}

/// `sorter/launch_everything`: every loose item on it to its filter's side.
fn sort(world: &mut World, machine: Entity) {
    if !anchored(world, machine) {
        return;
    }
    let (Some(&pos), Some(state)) = (
        world.get::<TilePos>(machine),
        world.get::<Manufacturing>(machine).cloned(),
    ) else {
        return;
    };
    let items: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| e != machine && world.get::<DmItem>(e).is_some() && !anchored(world, e))
        .collect();
    for item in items {
        let dir = state
            .filters
            .iter()
            .find(|f| meets(world, &f.kind, item) != f.inverted)
            .map_or(state.unmet, |f| f.dir);
        // tg stops after the first filtered item; the rest wait a tick for nothing.
        send_resource(world, machine, Some(item), dir);
    }
}

/// `unloader/process`.
fn unload(world: &mut World, machine: Entity) {
    let Some(held) = contents(world, machine).first().copied() else {
        return;
    };
    if surplus(world, machine) < UNLOAD_COST {
        return;
    }
    add_load(world, machine, UNLOAD_COST);
    // `PopulateContents` of a crate never opened.
    let fresh = world
        .get_mut::<crate::closets::Closet>(held)
        .is_some_and(|mut c| !std::mem::replace(&mut c.contents_initialized, true));
    if fresh && let Some(fills) = world.get::<ss13_mapload::DmContents>(held).cloned() {
        crate::storage::spawn_contents(world, held, &fills.populate, 0);
    }
    let dir = dir_of(world, machine);
    let inside: Vec<Entity> = world
        .get::<Stored>(held)
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    for thing in inside {
        send_resource(world, machine, Some(thing), dir);
    }
    let empty = world
        .get::<Stored>(held)
        .is_none_or(|s| s.items().is_empty());
    let flip = world.get::<Manufacturing>(machine).is_some_and(|m| m.flip);
    let side = if flip { ccw(dir) } else { cw(dir) };
    if empty
        && send_resource(world, machine, Some(held), side)
        && world.get::<crate::closets::Closet>(held).is_some()
    {
        crate::closets::open(world, held);
    }
}

/// The held item on a manufacturing machine: wrench, multitool, the storage unit's
/// screwdriver. True when it was ours.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(kind) = kind_of(world, target) else {
        return false;
    };
    if world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0)
    {
        return false;
    }
    // `is_path_specific/sorter_hit`: the item hit with sets the waiting filter.
    if kind == Kind::Sorter
        && let Some(id) = world.get::<Manufacturing>(target).and_then(|m| m.awaiting)
    {
        set_item_filter(world, client, target, id, held);
        return true;
    }
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    match tool_of(world, held).map(|t| t.0) {
        Some(ToolKind::Wrench) => {
            let anchored = anchored(world, target);
            use_tool(
                world,
                actor,
                target,
                WRENCH_DS,
                0,
                Job::Unfasten { anchored },
            );
            true
        }
        Some(ToolKind::Screwdriver) if kind == Kind::Storage => {
            tell(world, client, Span::Notice, "разбираем...");
            use_tool(
                world,
                actor,
                target,
                BREAK_DS,
                0,
                Job::EngMachine(EngJob::BreakStorage),
            );
            true
        }
        Some(ToolKind::Multitool) if kind == Kind::Router => {
            toggle_output(world, client, mob, target);
            true
        }
        Some(ToolKind::Multitool) if kind == Kind::Lathe => {
            ask_design(world, client, mob, target);
            true
        }
        Some(ToolKind::Multitool) if kind == Kind::Sorter => {
            open_sorter(world, client, mob, target);
            true
        }
        Some(ToolKind::Multitool) if kind == Kind::Unloader => {
            if let Some(mut m) = world.get_mut::<Manufacturing>(target) {
                m.flip = !m.flip;
            }
            tell(world, client, Span::Notice, "перевёрнуто");
            true
        }
        _ => false,
    }
}

/// `router/multitool_act`: the output on the user's side on or off.
pub fn toggle_output(world: &mut World, client: Entity, mob: Entity, router: Entity) {
    let (Some(&from), Some(&to)) = (world.get::<TilePos>(router), world.get::<TilePos>(mob)) else {
        return;
    };
    let side = Dir::CARDINALS.into_iter().find(|d| from.step(*d) == to);
    let Some(side) = side else {
        tell(world, client, Span::Warning, "встаньте на одну линию!");
        return;
    };
    if let Some(mut m) = world.get_mut::<Manufacturing>(router) {
        if let Some(i) = m.disabled.iter().position(|d| *d == side) {
            m.disabled.remove(i);
        } else {
            m.disabled.push(side);
        }
    }
    tell(world, client, Span::Notice, "выход переключён");
}

/// `storagebox/attack_hand`: out of combat mode, a dump after 1.25 s.
fn attack_hand(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    // `INTERACT_ATOM_ATTACK_HAND`: the sorter's window.
    if kind_of(world, target) == Some(Kind::Sorter) {
        open_sorter(world, client, mob, target);
        return true;
    }
    if kind_of(world, target) != Some(Kind::Storage) {
        return false;
    }
    if world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0)
    {
        return false;
    }
    tell(world, client, Span::Notice, "вываливаем...");
    let actor = Actor {
        client,
        mob,
        item: None,
    };
    crate::tools::do_after(
        world,
        actor,
        target,
        DUMP_DS,
        0,
        Job::EngMachine(EngJob::DumpStorage),
    );
    true
}

/// `dump_inventory_contents` of a storage unit onto its tile.
pub fn dump(world: &mut World, machine: Entity) {
    if kind_of(world, machine) != Some(Kind::Storage) {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(machine) else {
        return;
    };
    for item in contents(world, machine) {
        take_out(world, item, pos);
    }
}

/// `storagebox/atom_destruction`: 10 iron sheets and its contents on the tile.
pub fn break_storage(world: &mut World, machine: Entity) {
    if kind_of(world, machine) != Some(Kind::Storage) {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(machine) else {
        return;
    };
    dump(world, machine);
    crate::construction::spawn_stack(world, IRON, pos, STORAGE_SHEETS);
    on_destroy(world, machine);
    world.despawn(machine);
}

/// `Destroy`: off the net. The held output is among the contents dumped on the tile
/// (`atom_destruction` moves it out).
pub fn on_destroy(world: &mut World, machine: Entity) {
    if world
        .get::<Manufacturing>(machine)
        .is_some_and(|m| m.connected)
        && let Some(mut grid) = world.get_resource_mut::<Grid>()
    {
        grid.remove_node(machine);
    }
}

/// `simple_rotation` (alt-click) of a loose manufacturing machine. True when `target` is one.
pub fn rotate(world: &mut World, target: Entity) -> bool {
    if kind_of(world, target).is_none() || anchored(world, target) {
        return false;
    }
    let dir = ccw(dir_of(world, target));
    world.entity_mut(target).insert(Facing(dir));
    true
}

// --- Lathe -----------------------------------------------------------------------------------

/// `RefreshParts`: `bin.tier * SHEET_MATERIAL_AMOUNT * MAX_STACK_SIZE`.
fn lathe_capacity(world: &World, e: Entity) -> u32 {
    let tier = crate::stock_parts::ratings(world, e)
        .sum(crate::stock_parts::PartKind::MatterBin)
        .max(1);
    (f64::from(tier) * UNITS_PER_BIN) as u32
}

/// The autolathe's `initial` designs (`/datum/techweb/autounlocking/autolathe`
/// `researched_designs`) it can print, by name.
pub fn lathe_designs(db: &ResearchDb) -> Vec<usize> {
    let mut ids: Vec<usize> = db
        .designs
        .iter()
        .enumerate()
        .filter(|(_, d)| {
            d.build_type & ss13_mapload::research::build_type::AUTOLATHE != 0
                && d.category.iter().any(|c| c == "initial")
                && d.build_path.is_some()
                && !d.material_requirements
                && !d.materials.is_empty()
        })
        .map(|(i, _)| i)
        .collect();
    ids.sort_by(|&a, &b| db.designs[a].name.cmp(&db.designs[b].name));
    ids
}

/// `materials.insert_item`: what fits of the stack. True when all of it went in.
fn lathe_insert(world: &mut World, machine: Entity, item: Entity) -> bool {
    let Some(path) = world.get::<DmItem>(item).map(|i| i.path.clone()) else {
        return false;
    };
    let per_unit = world
        .get_resource::<ResearchDb>()
        .and_then(|db| db.stacks.get(&path).cloned())
        .unwrap_or_default();
    if per_unit.is_empty() {
        return false;
    }
    let amount = stack_amount(world, item);
    let capacity = lathe_capacity(world, machine);
    let taken = world
        .get_mut::<MaterialStore>(machine)
        .map_or(0, |mut store| {
            store.max = Some(capacity);
            store.insert_stack(&per_unit, amount)
        });
    if taken == 0 {
        return false;
    }
    crate::tools::use_stack(world, item, taken);
    // The rest stays with the sender (tg reports success and keeps what did not fit).
    taken == amount
}

use crate::tools::now;

fn set_busy(world: &mut World, machine: Entity, until: Option<f64>) {
    if let Some(mut m) = world.get_mut::<Manufacturing>(machine) {
        m.busy_until = until;
    }
}

/// `lathe/process`: a print starts when power and materials allow; it ends after
/// `(construction_time * lathe_time_factor) ** 0.8` deciseconds.
fn lathe(world: &mut World, machine: Entity) {
    if !flush_withheld(world, machine) {
        return;
    }
    let Some((Some(design), busy)) = world
        .get::<Manufacturing>(machine)
        .map(|m| (m.design, m.busy_until))
    else {
        return;
    };
    if surplus(world, machine) < LATHE_COST {
        // `finalize_build`: a power cut ends the print.
        set_busy(world, machine, None);
        return;
    }
    let t = now(world);
    if let Some(until) = busy {
        add_load(world, machine, LATHE_COST);
        if t >= until {
            set_busy(world, machine, None);
            make_design(world, machine, design);
        }
        return;
    }
    let Some(db) = world.get_resource::<ResearchDb>() else {
        return;
    };
    let Some(d) = db.designs.get(design) else {
        return;
    };
    let secs = (d.construction_time * d.lathe_time_factor).powf(0.8) / 10.0;
    let cost = design_cost(db, design);
    let enough = world
        .get::<MaterialStore>(machine)
        .is_some_and(|s| s.has_materials(&cost, 1.0, 1));
    if !enough {
        return;
    }
    add_load(world, machine, LATHE_COST);
    set_busy(world, machine, Some(t + secs));
}

/// `do_make_item`: the materials go, the item comes out forward (or waits inside).
fn make_design(world: &mut World, machine: Entity, design: usize) {
    let Some(db) = world.get_resource::<ResearchDb>() else {
        return;
    };
    let Some(d) = db.designs.get(design) else {
        return;
    };
    let (Some(path), stack) = (d.build_path.clone(), d.stack) else {
        return;
    };
    let cost = design_cost(db, design);
    let Some(mut store) = world.get_mut::<MaterialStore>(machine) else {
        return;
    };
    if !store.has_materials(&cost, 1.0, 1) {
        return;
    }
    store.use_materials(&cost, 1.0, 1);
    let made = if stack {
        let amount = world
            .get_resource::<ss13_proto::Prototypes>()
            .and_then(|p| p.get(&path))
            .and_then(|p| p.get::<DmStack>())
            .map_or(1, |s| s.amount.max(1));
        make_inside(world, machine, &path, amount)
    } else {
        world.get::<TilePos>(machine).copied().and_then(|pos| {
            let e = crate::construction::spawn_obj(world, &path, pos)?;
            put_inside(world, e, machine);
            Some(e)
        })
    };
    let Some(made) = made else {
        return;
    };
    let dir = dir_of(world, machine);
    if !send_resource(world, machine, Some(made), dir) {
        set_withheld(world, machine, Some(made));
    }
}

/// `lathe/multitool_act`: the design list.
fn ask_design(world: &mut World, client: Entity, mob: Entity, machine: Entity) {
    let Some(db) = world.get_resource::<ResearchDb>() else {
        return;
    };
    let ids = lathe_designs(db);
    let items = ids.iter().map(|&i| db.designs[i].name.clone()).collect();
    let kind = PromptKind::List {
        items,
        default: None,
    };
    let prompt = ask_world(
        world,
        Ask::new(client, mob, "Select Design", "Select Design", kind),
    );
    world.entity_mut(prompt).insert(ManuPrompt {
        machine,
        asks: Asks::Design(ids),
    });
}

// --- Sorter window ---------------------------------------------------------------------------

/// What a manufacturing prompt answers.
#[derive(Clone, Debug)]
pub enum Asks {
    Design(Vec<usize>),
    NewFilter,
    FilterText(u32),
}

#[derive(Component, Clone, Debug)]
struct ManuPrompt {
    machine: Entity,
    asks: Asks,
}

/// The `new_filter` list, sorted by name (`sort_list(filter_by_name)`).
// shortcut: no `is_tagged` filter (destination tags of parcels are not ported).
pub fn filter_choices() -> [(&'static str, FilterKind); 6] {
    [
        ("input is mail", FilterKind::IsMail),
        ("input is ore", FilterKind::IsOre),
        ("input is specific item", FilterKind::Path(String::new())),
        (
            "input is specific kind of item",
            FilterKind::Subtypes(String::new()),
        ),
        ("input is stack", FilterKind::IsStack),
        (
            "input's name contains",
            FilterKind::NameContains(String::new()),
        ),
    ]
}

fn open_sorter(world: &mut World, client: Entity, mob: Entity, sorter: Entity) {
    let name = world
        .get::<DmName>(sorter)
        .map(|n| crate::obj_damage::strip(&n.name).to_owned())
        .unwrap_or_default();
    world.write_message(OpenUi::new::<ManufacturingSorterUi>(
        client, mob, sorter, name,
    ));
}

impl ServerUi for ManufacturingSorterUi {
    type Source = &'static Manufacturing;
    type UserParam = ();
    type Act = SorterAct;

    fn static_data(_: &Manufacturing) {}

    fn data(m: &Manufacturing, _: Entity, _: &()) -> SorterData {
        SorterData {
            unmet_dir: m.unmet as u8,
            filters: m
                .filters
                .iter()
                .map(|f| SorterFilterRow {
                    name: f.name.clone(),
                    r#ref: f.id.to_string(),
                    inverted: f.inverted,
                    dir: f.dir as u8,
                })
                .collect(),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<SorterAct, UiReject> {
        let id = || {
            params
                .get(SORTER_REF)
                .and_then(|r| r.parse::<u32>().ok())
                .ok_or_else(|| UiReject::BadParams("ref: expected a filter".into()))
        };
        match action {
            SORTER_DEL => Ok(SorterAct::Delete(id()?)),
            SORTER_NEW => Ok(SorterAct::New),
            SORTER_ROTATE => Ok(SorterAct::Rotate(id()?)),
            SORTER_ROTATE_UNMET => Ok(SorterAct::RotateUnmet),
            SORTER_EDIT => Ok(SorterAct::Edit(id()?)),
            SORTER_SHIFT => {
                let amount = params.number(SORTER_AMOUNT)?;
                if !amount.is_finite() {
                    return Err(UiReject::BadParams("amount: expected a number".into()));
                }
                Ok(SorterAct::Shift(id()?, amount.clamp(-100.0, 100.0) as i64))
            }
            _ => Err(UiReject::UnknownAction),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SorterAct {
    Delete(u32),
    New,
    Rotate(u32),
    RotateUnmet,
    Edit(u32),
    Shift(u32, i64),
}

/// The next of `GLOB.cardinals` (north, south, east, west).
fn next_cardinal(dir: Dir) -> Dir {
    let i = Dir::CARDINALS.iter().position(|d| *d == dir).unwrap_or(3);
    Dir::CARDINALS[(i + 1) % 4]
}

fn sorter_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<ManufacturingSorterUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, SorterAct)> = acts
        .read(world.resource::<Messages<UiActed<ManufacturingSorterUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, mob, sorter, act) in pending {
        if kind_of(world, sorter) == Some(Kind::Sorter) {
            apply_sorter_act(world, client, mob, sorter, act);
        }
    }
}

/// A window act on a sorter; ids it does not have are ignored.
pub fn apply_sorter_act(
    world: &mut World,
    client: Entity,
    mob: Entity,
    sorter: Entity,
    act: SorterAct,
) {
    let Some(mut m) = world.get::<Manufacturing>(sorter).cloned() else {
        return;
    };
    let find = |m: &Manufacturing, id: u32| m.filters.iter().position(|f| f.id == id);
    match act {
        SorterAct::Delete(id) => {
            if let Some(i) = find(&m, id) {
                m.filters.remove(i);
                if m.awaiting == Some(id) {
                    m.awaiting = None;
                }
            }
        }
        SorterAct::Rotate(id) => {
            if let Some(i) = find(&m, id) {
                m.filters[i].dir = next_cardinal(m.filters[i].dir);
            }
        }
        SorterAct::RotateUnmet => m.unmet = next_cardinal(m.unmet),
        SorterAct::Shift(id, amount) => {
            if let Some(i) = find(&m, id) {
                let len = m.filters.len() as i64;
                let to = (i as i64 + amount).rem_euclid(len) as usize;
                let f = m.filters.remove(i);
                m.filters.insert(to.min(m.filters.len()), f);
            }
        }
        SorterAct::New => {
            if m.filters.len() >= MAX_FILTERS {
                return;
            }
            let items = filter_choices()
                .iter()
                .map(|(n, _)| (*n).to_owned())
                .collect();
            let kind = PromptKind::List {
                items,
                default: None,
            };
            let prompt = ask_world(
                world,
                Ask::new(client, mob, "New Filter", "Select a filter", kind),
            );
            world.entity_mut(prompt).insert(ManuPrompt {
                machine: sorter,
                asks: Asks::NewFilter,
            });
        }
        SorterAct::Edit(id) => {
            let Some(i) = find(&m, id) else {
                return;
            };
            match m.filters[i].kind.clone() {
                FilterKind::NameContains(value) => {
                    let kind = PromptKind::Text {
                        default: value,
                        max_length: FILTER_TEXT_LEN,
                        multiline: false,
                    };
                    let prompt = ask_world(
                        world,
                        Ask::new(client, mob, "Name", "What should it contain?", kind),
                    );
                    world.entity_mut(prompt).insert(ManuPrompt {
                        machine: sorter,
                        asks: Asks::FilterText(id),
                    });
                }
                FilterKind::Path(_) | FilterKind::Subtypes(_) => {
                    // `is_path_specific/edit`: listening toggles.
                    let base = filter_choices()
                        .into_iter()
                        .find(|(_, k)| {
                            std::mem::discriminant(k) == std::mem::discriminant(&m.filters[i].kind)
                        })
                        .map_or("", |(n, _)| n);
                    if m.awaiting == Some(id) {
                        m.awaiting = None;
                        m.filters[i].name = base.to_owned();
                    } else {
                        m.awaiting = Some(id);
                        m.filters[i].name = "awaiting item".to_owned();
                        tell(
                            world,
                            client,
                            Span::Notice,
                            "Hit the sorter with the item of choice to set the filter.",
                        );
                    }
                }
                _ => tell(world, client, Span::Notice, "This filter is not editable."),
            }
        }
    }
    if let Some(mut live) = world.get_mut::<Manufacturing>(sorter) {
        *live = m;
    }
}

/// `sorter_hit`: the waiting item filter takes the item's type and name.
fn set_item_filter(world: &mut World, client: Entity, sorter: Entity, id: u32, item: Entity) {
    let Some(path) = item_path(world, item).map(str::to_owned) else {
        return;
    };
    let name = world
        .get::<DmName>(item)
        .map(|n| crate::obj_damage::strip(&n.name).to_owned())
        .unwrap_or_else(|| path.clone());
    let Some(mut m) = world.get_mut::<Manufacturing>(sorter) else {
        return;
    };
    m.awaiting = None;
    if let Some(f) = m.filters.iter_mut().find(|f| f.id == id) {
        f.kind = match f.kind {
            FilterKind::Subtypes(_) => FilterKind::Subtypes(path),
            _ => FilterKind::Path(path),
        };
        f.name = name;
    }
    tell(world, client, Span::Notice, "filter set");
}

fn prompt_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, mob, answer) in answers {
        let Some(ManuPrompt { machine, asks }) = world.get::<ManuPrompt>(prompt).cloned() else {
            continue;
        };
        // `can_perform_action(src)` after the input.
        // shortcut: no silicon reach (`ALLOW_SILICON_REACH`); add with remote machine access.
        let (Some(answer), true) = (answer, crate::storage::in_reach(world, mob, machine)) else {
            continue;
        };
        apply_answer(world, client, machine, asks, answer);
    }
}

/// A prompt's answer, checked against the machine as it is now.
pub fn apply_answer(
    world: &mut World,
    client: Entity,
    machine: Entity,
    asks: Asks,
    answer: PromptAnswer,
) {
    let kind = kind_of(world, machine);
    match (asks, answer) {
        (Asks::Design(ids), PromptAnswer::Item(i)) if kind == Some(Kind::Lathe) => {
            let Some(&design) = ids.get(i) else {
                return;
            };
            if let Some(mut m) = world.get_mut::<Manufacturing>(machine) {
                m.design = Some(design);
                m.busy_until = None;
            }
            tell(world, client, Span::Notice, "set");
        }
        (Asks::NewFilter, PromptAnswer::Item(i)) if kind == Some(Kind::Sorter) => {
            let Some((name, filter)) = filter_choices().into_iter().nth(i) else {
                return;
            };
            let Some(mut m) = world.get_mut::<Manufacturing>(machine) else {
                return;
            };
            if m.filters.len() >= MAX_FILTERS {
                return;
            }
            let id = m.next_id;
            m.next_id += 1;
            m.filters.push(SortFilter {
                id,
                name: name.to_owned(),
                kind: filter,
                dir: Dir::North,
                inverted: false,
            });
        }
        (Asks::FilterText(id), PromptAnswer::Text(text)) if kind == Some(Kind::Sorter) => {
            let Some(mut m) = world.get_mut::<Manufacturing>(machine) else {
                return;
            };
            if let Some(f) = m.filters.iter_mut().find(|f| f.id == id)
                && matches!(f.kind, FilterKind::NameContains(_))
            {
                let value: String = text.chars().take(FILTER_TEXT_LEN as usize).collect();
                f.name = format!("input's name contains {value}");
                // `findtext` is case-insensitive.
                f.kind = FilterKind::NameContains(value.to_lowercase());
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turns() {
        assert_eq!(ccw(Dir::North), Dir::West);
        assert_eq!(cw(Dir::North), Dir::East);
        assert_eq!(cw(ccw(Dir::South)), Dir::South);
    }

    #[test]
    fn kinds_by_path() {
        assert_eq!(Kind::of_path(CRUSHER), Some(Kind::Crusher));
        assert_eq!(
            Kind::of_path("/obj/machinery/power/manufacturing/router"),
            Some(Kind::Router)
        );
        assert_eq!(Kind::of_path(ROOT), None);
    }
}
