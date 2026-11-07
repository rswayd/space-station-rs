//! The flatpacker and flatpacks (server): tg `/obj/machinery/flatpacker`
//! (`code/game/machinery/flatpacker.dm`) with tgui's `Flatpacker`, and `/obj/item/flatpack`
//! (`code/game/objects/items/flatpacks.dm`).
//!
//! - Materials: its own [`MaterialStore`] of silo materials, `25 sheets * matter bin tier`
//!   each; stacks go in like the autolathe's (out of combat mode, powered, not busy), `eject`
//!   gives sheets back, taking it apart drops them.
//! - A machine board clicked in (not while busy) replaces the one inside (the old one drops) and
//!   prices the machine: every `req_components` entry but the hand-inserted ones (vendor refill
//!   canisters, tg `flatpack_components`) at its design's materials (stacks at their
//!   `mats_per_unit`), plus 5 sheets of iron and 1/20 of iron and glass for frame and cable.
//!   Refill canisters the board needs are clicked in by hand, up to the number required.
//! - `build` (window): refused while busy, without a board, for a design above the servo tier
//!   (`Design too complex.`), without the hand-inserted parts or the materials at
//!   `creation_efficiency`; else after `flatpack_time` it takes the materials and drops a
//!   flatpack holding the board and those parts. `ejectBoard` gives the board to the user.
//! - `RefreshParts`: capacity above, `max_part_tier` = the servo's tier, `flatpack_time = 4.5 s -
//!   servo tier / 2`, `creation_efficiency = max(1.2, round(2 - 0.2 * laser tiers, 0.1))`.
//! - A flatpack on an open, unblocked floor deploys with a multitool after 1 s: the board's
//!   machine with its default parts (the packed parts in place of their kind) and the setup of
//!   a frame-built one (`crate::machine_frame::build_machine`).
// shortcut: no ctrl-click board eject, insertion energy and animations, two-handed carrying or
// slowdown (flatpack carts and typed flatpacks: `crate::misc_items::flatpacks`); the window's
// material bar is plain rows (`crate::flatpacker_client`).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::research::SHEET_MATERIAL_AMOUNT;
use ss13_mapload::{DmCircuit, DmItem, DmName, DmObj};
use ss13_proto::Prototypes;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};
use ss13_world::SpatialIndex;

use crate::cargo::console::machine_say;
use crate::construction::{Job, path_of, spawn_obj, turf_at};
use crate::flatpacker_ui::{
    AMOUNT, BUILD, EJECT, EJECT_BOARD, FlatpackDesign, FlatpackMaterial, FlatpackerData,
    FlatpackerUi, MATERIAL_REF,
};
use crate::inventory::{CarriedBy, Inventory};
use crate::obj_damage::is_under;
use crate::power::Unpowered;
use crate::science::lathe::{MaterialStore, design_cost, optimal_cost};
use crate::science::techweb::ResearchDb;
use crate::stock_parts::PartKind;
use crate::storage::{Stored, StoredIn, put_inside, take_out};
use crate::tools::{Actor, ToolKind, tell, tool_of, use_tool};

use super::EngJob;

pub const FLATPACKER: &str = "/obj/machinery/flatpacker";
pub const FLATPACK: &str = "/obj/item/flatpack";
const MAPPED_FLATPACK_BOARD: &str = "/obj/item/circuitboard/machine/flatpacker";
const MACHINE_BOARD: &str = ss13_mapload::circuit::MACHINE_BOARD_ROOT;
/// Parts that must be packed by hand (`flatpack_components`: vendor boards' canisters).
const REFILL: &str = "/obj/item/vending_refill";
const IRON: &str = "/datum/material/iron";
const GLASS: &str = "/datum/material/glass";
/// `flatpack_time`, `creation_efficiency` (initial).
pub const BASE_SECS: f64 = 4.5;
const BASE_EFFICIENCY: f64 = 2.0;
/// `25 * SHEET_MATERIAL_AMOUNT` per matter bin tier.
const UNITS_PER_BIN: f64 = 25.0 * SHEET_MATERIAL_AMOUNT;
/// The flatpack's `do_after`, deciseconds.
const DEPLOY_DS: f32 = 10.0;
const RPED: &str = "sound/items/tools/rped.ogg";
const DEPLOYED: &str = "sound/machines/terminal/terminal_eject.ogg";

/// A flatpacker's live state; its materials are its [`MaterialStore`], the board and packed
/// parts are stored inside it.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Flatpacker {
    /// `busy`: when the flatpack is done (game seconds).
    pub busy_until: Option<f64>,
    pub board: Option<Entity>,
    /// `flatpacked_components`.
    pub components: Vec<Entity>,
    /// `needed_mats`: material index -> units.
    pub needed: Vec<(usize, f64)>,
    /// `print_tier`.
    pub print_tier: u32,
    /// What the board needs packed by hand: path, name, count.
    pub hand_parts: Vec<(String, String, u32)>,
    /// `initial(build.name)`.
    pub design_name: String,
    /// `max_part_tier`, `flatpack_time` (seconds), `creation_efficiency`.
    pub max_tier: u32,
    pub secs: f64,
    pub efficiency: f64,
}

impl Default for Flatpacker {
    fn default() -> Self {
        Self {
            busy_until: None,
            board: None,
            components: Vec::new(),
            needed: Vec::new(),
            print_tier: 1,
            hand_parts: Vec::new(),
            design_name: String::new(),
            max_tier: 1,
            secs: BASE_SECS,
            efficiency: BASE_EFFICIENCY,
        }
    }
}

/// A flatpack: the board it deploys is stored inside it with its packed parts.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Flatpack {
    pub board: Entity,
}

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // `base_item_interaction` (boards, packed parts, stacks) and the flatpack's
    // `multitool_act` (before `crate::machine_frame`).
    app.add_click_handler(ClickPhase::HeldOnTarget, 1569, |w, c| {
        c.held
            .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target))
    });
    // `ui_interact`.
    app.add_click_handler(ClickPhase::EmptyHand, 1989, |w, c| {
        interact(w, c.client, c.mob, c.target)
    });
    app.register_ui::<FlatpackerWindow>()
        .add_systems(
            ss13_world::MapInit,
            setup_mapped.after(crate::power::init_power),
        )
        .add_systems(
            Update,
            (
                flatpacker_acts.in_set(UiSystems::Handle),
                finish_builds.run_if(any_with_component::<Flatpacker>),
            ),
        );
}

fn setup_mapped(world: &mut World) {
    // shortcut: a mapped flatpack's `board` var edit is not read; every station map packs the
    // flatpacker's own board, so that is what a mapped one holds.
    let mut packs = world.query_filtered::<(Entity, &DmItem), With<ss13_world::Mapped>>();
    let packs: Vec<Entity> = packs
        .iter(world)
        .filter(|(_, i)| i.path == FLATPACK)
        .map(|(e, _)| e)
        .collect();
    for pack in packs {
        let Some(&pos) = world.get::<TilePos>(pack) else {
            continue;
        };
        let Some(board) = spawn_obj(world, MAPPED_FLATPACK_BOARD, pos) else {
            continue;
        };
        pack_board(world, pack, board);
    }
    let mut q = world.query_filtered::<(Entity, &DmObj), With<ss13_world::Mapped>>();
    let found: Vec<Entity> = q
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, FLATPACKER))
        .map(|(e, _)| e)
        .collect();
    for e in found {
        setup(world, e);
    }
}

/// A flatpack's `Initialize(mapload, new_board)`: `board` inside it, the pack named after its
/// machine (also the typed flatpacks of `crate::misc_items::flatpacks`).
pub fn pack_board(world: &mut World, pack: Entity, board: Entity) {
    world.entity_mut(pack).insert(Flatpack { board });
    put_inside(world, board, pack);
    let name = circuit_of(world, board)
        .map(|d| proto_name(world, &d.build_path))
        .unwrap_or_default();
    world
        .entity_mut(pack)
        .insert(Name::new(format!("flatpack ({name})")));
}

/// `Initialize`; also for one built from a frame (`crate::machine_frame::setup_built`).
pub fn setup(world: &mut World, e: Entity) {
    let materials = world
        .get_resource::<ResearchDb>()
        .map_or(0, |db| db.materials.len());
    world.entity_mut(e).insert((
        Flatpacker::default(),
        MaterialStore::new(materials, Some(0)),
    ));
    refresh_parts(world, e);
}

/// `RefreshParts`: (capacity units, `max_part_tier`, `flatpack_time` seconds,
/// `creation_efficiency`).
pub fn part_numbers(r: &crate::stock_parts::PartRatings) -> (u32, u32, f64, f64) {
    let capacity = f64::from(r.sum(PartKind::MatterBin)) * UNITS_PER_BIN;
    let servo = r.first(PartKind::Servo).unwrap_or(1);
    let secs = BASE_SECS - f64::from(servo) / 2.0;
    let efficiency =
        ((BASE_EFFICIENCY - 0.2 * f64::from(r.sum(PartKind::MicroLaser))) * 10.0).round() / 10.0;
    (capacity as u32, servo, secs, efficiency.max(1.2))
}

/// From [`crate::stock_parts::refresh`].
pub(crate) fn refresh_parts(world: &mut World, e: Entity) {
    if world.get::<Flatpacker>(e).is_none() {
        return;
    }
    let (capacity, tier, secs, efficiency) = part_numbers(&crate::stock_parts::ratings(world, e));
    if let Some(mut f) = world.get_mut::<Flatpacker>(e) {
        f.max_tier = tier;
        f.secs = secs;
        f.efficiency = efficiency;
    }
    if let Some(mut store) = world.get_mut::<MaterialStore>(e) {
        store.max = Some(capacity);
    }
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn plain(name: &str) -> String {
    crate::obj_damage::strip(name).to_owned()
}

fn proto_name(world: &World, path: &str) -> String {
    world
        .get_resource::<Prototypes>()
        .and_then(|p| p.get(path))
        .and_then(|p| p.get::<DmName>())
        .map(|n| plain(&n.name))
        .unwrap_or_else(|| path.rsplit('/').next().unwrap_or(path).replace('_', " "))
}

/// A board item's vars: its own (prototype components), else its prototype's.
fn circuit_of(world: &World, board: Entity) -> Option<DmCircuit> {
    if let Some(c) = world.get::<DmCircuit>(board) {
        return Some(c.clone());
    }
    let path = &world.get::<DmItem>(board)?.path;
    world
        .get_resource::<Prototypes>()?
        .get(path)?
        .get::<DmCircuit>()
        .cloned()
}

/// What a board's machine costs (`analyze_cost` of each part, the frame and cable), its
/// `print_tier` and the parts packed by hand (path, count).
pub fn board_cost(
    db: &ResearchDb,
    def: &DmCircuit,
    tier_of: impl Fn(&str) -> u32,
) -> (Vec<(usize, f64)>, u32, Vec<(String, u32)>) {
    let mut cost: Vec<(usize, f64)> = Vec::new();
    let mut add = |m: usize, n: f64| match cost.iter_mut().find(|(i, _)| *i == m) {
        Some(entry) => entry.1 += n,
        None => cost.push((m, n)),
    };
    let mut tier = 1;
    let mut hand = Vec::new();
    for part in &def.req_components {
        if is_under(&part.path, REFILL) {
            hand.push((part.path.clone(), part.amount));
            continue;
        }
        tier = tier.max(tier_of(&part.path));
        let per_unit = db.stacks.get(&part.path).cloned().or_else(|| {
            let design = db
                .designs
                .iter()
                .position(|d| d.build_path.as_deref() == Some(part.path.as_str()))?;
            Some(design_cost(db, design))
        });
        for (m, n) in per_unit.unwrap_or_default() {
            add(m, n * f64::from(part.amount));
        }
    }
    // 5 sheets of iron (the frame) and 5 cable coil.
    if let Some(&iron) = db.material_index.get(IRON) {
        add(
            iron,
            SHEET_MATERIAL_AMOUNT * 5.0 + SHEET_MATERIAL_AMOUNT / 20.0,
        );
    }
    if let Some(&glass) = db.material_index.get(GLASS) {
        add(glass, SHEET_MATERIAL_AMOUNT / 20.0);
    }
    (cost, tier, hand)
}

/// Takes `item` out of `mob`'s hands.
fn from_hands(world: &mut World, mob: Entity, item: Entity) {
    world.entity_mut(item).remove::<CarriedBy>();
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob)
        && let Some(slot) = inventory.hands.iter_mut().find(|h| **h == Some(item))
    {
        *slot = None;
    }
}

fn count_packed(world: &World, f: &Flatpacker, path: &str) -> u32 {
    f.components
        .iter()
        .filter(|&&c| world.get::<DmItem>(c).is_some_and(|i| i.path == path))
        .count() as u32
}

/// The window: an empty hand on a flatpacker.
pub fn interact(world: &mut World, client: Entity, mob: Entity, e: Entity) -> bool {
    if world.get::<Flatpacker>(e).is_none() {
        return false;
    }
    let title = world
        .get::<DmName>(e)
        .map(|n| plain(&n.name))
        .unwrap_or_else(|| "flatpacker".to_owned());
    world.write_message(OpenUi::new::<FlatpackerWindow>(client, mob, e, title));
    true
}

/// The held item on a flatpacker or a flatpack. True when it was ours.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    e: Entity,
) -> bool {
    if world.get::<Flatpack>(e).is_some() {
        if let Some((ToolKind::Multitool, _)) = tool_of(world, held) {
            try_deploy(
                world,
                Actor {
                    client,
                    mob,
                    item: Some(held),
                },
                e,
            );
            return true;
        }
        return false;
    }
    let Some(f) = world.get::<Flatpacker>(e).cloned() else {
        return false;
    };
    let Some(path) = world.get::<DmItem>(held).map(|i| i.path.clone()) else {
        return false;
    };
    if is_under(&path, MACHINE_BOARD) {
        insert_board(world, client, mob, held, e);
        return true;
    }
    if f.board.is_some() && f.hand_parts.iter().any(|(p, _, _)| *p == path) {
        let required = f
            .hand_parts
            .iter()
            .find(|(p, _, _)| *p == path)
            .map_or(0, |h| h.2);
        if count_packed(world, &f, &path) >= required {
            tell(world, client, Span::Warning, "max count reached!");
            return true;
        }
        if crate::inventory::no_drop(world, held) {
            tell(world, client, Span::Warning, "It is stuck in hand!");
            return true;
        }
        from_hands(world, mob, held);
        put_inside(world, held, e);
        if let Some(mut f) = world.get_mut::<Flatpacker>(e) {
            f.components.push(held);
        }
        return true;
    }
    // The material container (`on_item_insert`: stacks, out of combat mode).
    if world.get::<crate::tools::Stack>(held).is_none()
        || world
            .get::<crate::combat::CombatMode>(mob)
            .is_some_and(|c| c.0)
        || world.get::<Unpowered>(e).is_some()
        || world
            .get::<crate::construction::PanelOpen>(e)
            .is_some_and(|p| p.0)
        || !world.contains_resource::<ResearchDb>()
    {
        return false;
    }
    crate::science::lathe::insert_stack(world, client, e, Some(e), held, &path);
    true
}

/// `base_item_interaction` with a machine board.
pub fn insert_board(world: &mut World, client: Entity, mob: Entity, board: Entity, e: Entity) {
    let Some(f) = world.get::<Flatpacker>(e).cloned() else {
        return;
    };
    if f.busy_until.is_some() {
        tell(world, client, Span::Warning, "busy!");
        return;
    }
    let Some(def) = circuit_of(world, board) else {
        return;
    };
    if crate::inventory::no_drop(world, board) {
        return;
    }
    from_hands(world, mob, board);
    put_inside(world, board, e);
    if let Some(old) = f.board
        && let Some(&pos) = world.get::<TilePos>(e)
    {
        take_out(world, old, pos);
    }
    let (needed, print_tier, hand) = match world.get_resource::<ResearchDb>() {
        Some(db) => board_cost(db, &def, |p| crate::stock_parts::tier(world, p)),
        None => (Vec::new(), 1, Vec::new()),
    };
    let hand_parts = hand
        .into_iter()
        .map(|(p, n)| {
            let name = proto_name(world, &p);
            (p, name, n)
        })
        .collect();
    let design_name = proto_name(world, &def.build_path);
    if let Some(mut f) = world.get_mut::<Flatpacker>(e) {
        f.board = Some(board);
        f.needed = needed;
        f.print_tier = print_tier;
        f.hand_parts = hand_parts;
        f.design_name = design_name;
    }
}

/// `ui_act("build")`. True when packing started.
pub fn start_build(world: &mut World, e: Entity) -> bool {
    let (Some(f), Some(store)) = (
        world.get::<Flatpacker>(e).cloned(),
        world.get::<MaterialStore>(e).cloned(),
    ) else {
        return false;
    };
    if f.busy_until.is_some() || f.board.is_none() {
        return false;
    }
    if f.print_tier > f.max_tier {
        machine_say(world, e, "Design too complex.");
        return false;
    }
    for (path, name, n) in &f.hand_parts {
        if count_packed(world, &f, path) != *n {
            machine_say(world, e, &format!("Not enough {name}."));
            return false;
        }
    }
    if !store.has_materials(&f.needed, f.efficiency, 1) {
        machine_say(world, e, "Not enough materials to begin production.");
        return false;
    }
    if let Some(&pos) = world.get::<TilePos>(e) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, RPED, 50.0, true));
    }
    let at = now(world) + f.secs;
    if let Some(mut f) = world.get_mut::<Flatpacker>(e) {
        f.busy_until = Some(at);
    }
    true
}

/// `finish_build`: the materials taken, a flatpack with the board and the packed parts.
fn finish_build(world: &mut World, e: Entity) {
    let Some(f) = world.get::<Flatpacker>(e).cloned() else {
        return;
    };
    let reset = Flatpacker {
        max_tier: f.max_tier,
        secs: f.secs,
        efficiency: f.efficiency,
        ..default()
    };
    world.entity_mut(e).insert(reset);
    let (Some(board), Some(&pos)) = (f.board, world.get::<TilePos>(e)) else {
        return;
    };
    // The board left while packing (taken apart): nothing to pack.
    if world.get::<StoredIn>(board).is_none_or(|s| s.0 != e) {
        return;
    }
    if let Some(mut store) = world.get_mut::<MaterialStore>(e) {
        if !store.has_materials(&f.needed, f.efficiency, 1) {
            return;
        }
        store.use_materials(&f.needed, f.efficiency, 1);
    }
    spawn_flatpack(world, pos, board, &f.components, &f.design_name);
}

/// `new /obj/item/flatpack(loc, board)` holding `parts`.
pub fn spawn_flatpack(
    world: &mut World,
    pos: TilePos,
    board: Entity,
    parts: &[Entity],
    machine_name: &str,
) -> Entity {
    let pack = spawn_obj(world, FLATPACK, pos).unwrap_or_else(|| {
        world
            .spawn((
                pos,
                DmItem {
                    path: FLATPACK.into(),
                    ..default()
                },
            ))
            .id()
    });
    world.entity_mut(pack).insert((
        Flatpack { board },
        Name::new(format!("flatpack ({machine_name})")),
    ));
    put_inside(world, board, pack);
    for &p in parts {
        if world.get_entity(p).is_ok() {
            put_inside(world, p, pack);
        }
    }
    pack
}

fn finish_builds(world: &mut World) {
    let t = now(world);
    let due: Vec<Entity> = world
        .query::<(Entity, &Flatpacker)>()
        .iter(world)
        .filter(|(_, f)| f.busy_until.is_some_and(|at| at <= t))
        .map(|(e, _)| e)
        .collect();
    for e in due {
        finish_build(world, e);
    }
}

/// Why a flatpack cannot deploy where it is (`multitool_act`); `None`: it can.
fn deploy_refusal(world: &World, pack: Entity) -> Option<&'static str> {
    let Some(&pos) = world.get::<TilePos>(pack) else {
        return Some("must deploy on the floor");
    };
    if world.get::<CarriedBy>(pack).is_some() || world.get::<StoredIn>(pack).is_some() {
        return Some("must deploy on the floor");
    }
    let open = turf_at(world, pos)
        .and_then(|t| path_of(world, t))
        .is_some_and(|p| is_under(&p, "/turf/open"));
    if !open {
        return Some("can't deploy here");
    }
    // `is_blocked_turf(source_atom = src)`: dense things but the pack itself.
    let blocked = world
        .resource::<SpatialIndex>()
        .dense_on_tile(pos)
        .iter()
        .any(|&d| d != pack);
    blocked.then_some("no space for deployment")
}

/// A multitool on a flatpack: 1 s, then [`deploy`].
pub fn try_deploy(world: &mut World, actor: Actor, pack: Entity) {
    if let Some(why) = deploy_refusal(world, pack) {
        tell(world, actor.client, Span::Warning, why);
        return;
    }
    tell(world, actor.client, Span::Notice, "deploying!");
    use_tool(
        world,
        actor,
        pack,
        DEPLOY_DS,
        0,
        Job::EngMachine(EngJob::DeployFlatpack),
    );
}

/// The flatpack deploys into its board's machine. True when it did.
pub fn deploy(world: &mut World, client: Entity, pack: Entity) -> bool {
    let Some(&Flatpack { board }) = world.get::<Flatpack>(pack) else {
        return false;
    };
    if let Some(why) = deploy_refusal(world, pack) {
        tell(world, client, Span::Warning, why);
        return false;
    }
    let (Some(def), Some(&pos)) = (circuit_of(world, board), world.get::<TilePos>(pack)) else {
        return false;
    };
    let packed: Vec<Entity> = world
        .get::<Stored>(pack)
        .map(|s| s.items().to_vec())
        .unwrap_or_default()
        .into_iter()
        .filter(|&i| i != board)
        .collect();
    // `replacement_parts`: the board's default parts, a packed part in place of its kind.
    let mut parts = vec![board];
    let mut spare = packed.clone();
    for p in &def.default_parts {
        for _ in 0..p.amount {
            let own = spare.iter().position(|&s| {
                world
                    .get::<DmItem>(s)
                    .is_some_and(|i| is_under(&i.path, &p.path))
            });
            match own {
                Some(i) => parts.push(spare.remove(i)),
                None => {
                    if let Some(part) = spawn_obj(world, &p.path, pos) {
                        parts.push(part);
                    }
                }
            }
        }
    }
    for &p in &parts {
        if world.get::<StoredIn>(p).is_some() {
            take_out(world, p, pos);
        }
    }
    let board_path = world
        .get::<DmItem>(board)
        .map(|i| i.path.clone())
        .unwrap_or_default();
    let name = world
        .get::<Name>(pack)
        .map(|n| n.as_str().to_owned())
        .unwrap_or_else(|| "flatpack".to_owned());
    // Unused packed parts stay on the tile.
    for s in spare {
        take_out(world, s, pos);
    }
    world.despawn(pack);
    crate::construction::tile_changed(world, pos);
    if crate::machine_frame::build_machine(world, pos, board_path, parts, true, None).is_none() {
        return false;
    }
    crate::security::show(
        world,
        pos,
        7,
        ss13_shared::chat::ChatChannel::Local,
        (Span::Warning, format!("{name} deploys!")),
        &[],
    );
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, DEPLOYED, 70.0, true),
    );
    true
}

/// The do_after of [`try_deploy`] is done.
pub fn complete_deploy(world: &mut World, client: Entity, pack: Entity) {
    deploy(world, client, pack);
}

pub struct FlatpackerWindow;

impl ss13_tgui::UiInterface for FlatpackerWindow {
    const NAME: &'static str = FlatpackerUi::NAME;
    type Data = FlatpackerData;
    type StaticData = ();
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FlatpackerAct {
    Build,
    EjectBoard,
    Eject(usize, f64),
}

impl ServerUi for FlatpackerWindow {
    type Source = (Entity, &'static Flatpacker, &'static MaterialStore);
    type UserParam = (
        Option<Res<'static, ResearchDb>>,
        Query<'static, 'static, &'static DmItem>,
    );
    type Act = FlatpackerAct;

    fn static_data(_: (Entity, &Flatpacker, &MaterialStore)) {}

    fn data(
        (_, f, store): (Entity, &Flatpacker, &MaterialStore),
        _: Entity,
        (db, items): &bevy::ecs::system::SystemParamItem<'_, '_, Self::UserParam>,
    ) -> FlatpackerData {
        let name = |i: usize| {
            db.as_ref()
                .and_then(|db| db.materials.get(i))
                .map(|m| m.name.clone())
                .unwrap_or_default()
        };
        let materials = store
            .amounts
            .iter()
            .enumerate()
            .filter(|(_, n)| **n > 0)
            .map(|(i, &n)| FlatpackMaterial {
                index: i as u32,
                name: name(i),
                amount: n,
            })
            .collect();
        let design = f.board.map(|_| {
            let packed = |path: &str| {
                f.components
                    .iter()
                    .filter(|&&c| items.get(c).is_ok_and(|i| i.path == path))
                    .count() as u32
            };
            let reason = if f.print_tier > f.max_tier {
                "This design is too advanced for this machine. ".to_owned()
            } else if !store.has_materials(&f.needed, f.efficiency, 1) {
                "Not enough materials. ".to_owned()
            } else {
                f.hand_parts
                    .iter()
                    .find_map(|(path, name, n)| {
                        let diff = n.saturating_sub(packed(path));
                        (diff > 0).then(|| format!("Please insert {diff} {name}"))
                    })
                    .unwrap_or_default()
            };
            FlatpackDesign {
                name: f.design_name.clone(),
                required: f
                    .needed
                    .iter()
                    .map(|&(i, n)| FlatpackMaterial {
                        index: i as u32,
                        name: name(i),
                        amount: optimal_cost(n * f.efficiency),
                    })
                    .collect(),
                disable_reason: reason,
            }
        });
        FlatpackerData {
            materials,
            busy: f.busy_until.is_some(),
            design,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<FlatpackerAct, UiReject> {
        match action {
            BUILD => Ok(FlatpackerAct::Build),
            EJECT_BOARD => Ok(FlatpackerAct::EjectBoard),
            EJECT => {
                let index = params.number(MATERIAL_REF)?;
                if !(0.0..256.0).contains(&index) {
                    return Err(UiReject::BadParams("ref: not a material".into()));
                }
                Ok(FlatpackerAct::Eject(index as usize, params.number(AMOUNT)?))
            }
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// tg `ui_act`, every check at the moment of effect.
fn flatpacker_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<FlatpackerWindow>>>) {
    let pending: Vec<(Entity, Entity, FlatpackerAct)> = acts
        .read(world.resource::<Messages<UiActed<FlatpackerWindow>>>())
        .map(|a| (a.actor, a.source, a.act))
        .collect();
    for (actor, e, act) in pending {
        let Some(f) = world.get::<Flatpacker>(e).cloned() else {
            continue;
        };
        match act {
            FlatpackerAct::Build => {
                start_build(world, e);
            }
            // Not while packing: tg would pack a board already in the user's hand.
            FlatpackerAct::EjectBoard => {
                if f.busy_until.is_none()
                    && let Some(board) = f.board
                {
                    crate::cooking::give_to(world, actor, e, board);
                    if let Some(mut f) = world.get_mut::<Flatpacker>(e) {
                        f.board = None;
                        f.needed.clear();
                        f.print_tier = 1;
                        f.hand_parts.clear();
                    }
                }
            }
            FlatpackerAct::Eject(material, amount) => {
                if world.contains_resource::<ResearchDb>() {
                    crate::science::lathe::eject_sheets(
                        world,
                        e,
                        actor,
                        material,
                        amount,
                        Some(e),
                        false,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stock_parts::PartRatings;
    use ss13_mapload::CircuitPart;
    use ss13_mapload::research::{DesignDef, MaterialDef, ResearchData};

    #[test]
    fn refresh_parts_numbers() {
        let parts = |bin: u32, laser: u32, servo: u32| PartRatings {
            parts: vec![
                (PartKind::MatterBin, bin),
                (PartKind::MatterBin, bin),
                (PartKind::MicroLaser, laser),
                (PartKind::MicroLaser, laser),
                (PartKind::Servo, servo),
            ],
            cells: vec![],
        };
        // The board's tier 1 parts: 5000 units, tier 1, 4 s, 1.6.
        assert_eq!(part_numbers(&parts(1, 1, 1)), (5000, 1, 4.0, 1.6));
        assert_eq!(part_numbers(&parts(4, 4, 4)), (20_000, 4, 2.5, 1.2));
    }

    #[test]
    fn a_board_costs_its_parts_frame_and_cable() {
        let mat = |id: &str| MaterialDef {
            id: id.into(),
            name: id.rsplit('/').next().unwrap().into(),
            ..default()
        };
        let db = ResearchDb::new(ResearchData {
            materials: vec![mat(IRON), mat(GLASS)],
            stacks: vec![(
                "/obj/item/stack/sheet/glass".into(),
                vec![(GLASS.into(), 100.0)],
            )],
            designs: vec![DesignDef {
                id: "/datum/design/servo".into(),
                build_path: Some("/obj/item/stock_parts/servo".into()),
                materials: vec![(IRON.into(), 50.0)],
                ..default()
            }],
            ..default()
        });
        let def = DmCircuit {
            build_path: "/obj/machinery/vending/x".into(),
            req_components: vec![
                CircuitPart {
                    path: "/obj/item/stock_parts/servo".into(),
                    amount: 2,
                },
                CircuitPart {
                    path: "/obj/item/stack/sheet/glass".into(),
                    amount: 1,
                },
                CircuitPart {
                    path: "/obj/item/vending_refill/x".into(),
                    amount: 1,
                },
            ],
            ..default()
        };
        let (cost, tier, hand) = board_cost(&db, &def, |_| 1);
        let iron = db.material_index[IRON];
        let glass = db.material_index[GLASS];
        let of = |m| cost.iter().find(|(i, _)| *i == m).unwrap().1;
        assert_eq!(of(iron), 100.0 + 505.0);
        assert_eq!(of(glass), 100.0 + 5.0);
        assert_eq!(tier, 1);
        assert_eq!(hand, vec![("/obj/item/vending_refill/x".to_owned(), 1)]);
    }
}
