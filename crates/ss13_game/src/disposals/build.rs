//! Building the disposal system: `/obj/structure/disposalconstruct` (`construction.dm`), the
//! disposal pipe dispenser (`/obj/machinery/pipedispenser/disposal`), welding pipes, bins and
//! outlets back to constructs.
//!
//! A construct remembers what it makes (`pipe_type`) and its `dir`. Alt-click turns it while
//! loose; a wrench fastens it on plating (a bin, chute or outlet only over a trunk, a pipe
//! only where no other pipe uses its openings), a welder turns it into the real thing. The
//! dispenser's `PipeDispenser` window drops a construct on the dispenser's tile.
// shortcut: no crowbar taking up the tile for you when wrenching; no flipping constructs (the
// dispenser offers only the base kinds); no dragging constructs back into the dispenser; a
// sort junction built here carries no sort tags (no destination tagger on pipes).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::engmisc::{DISPOSAL_CONSTRUCT, DISPOSAL_RECIPES};
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::undertile::accessibility;
use ss13_mapload::{DmAtom, DmName, DmObj};
use ss13_proto::Prototypes;
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Dir, Facing, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::pipe_dispenser::{
    PipeCategory, PipeDispenserData, PipeDispenserStatic, PipeDispenserUi, PipeRecipeEntry,
};
use ss13_tgui::server::{OpenUi, ServerUi, UiActed, UiParams, UiReject};
use ss13_world::{Density, SpatialIndex};

use super::{
    CHUTE_ROOT, DisposalOutlet, DisposalPipe, MACHINE_ROOT, OUTLET_ROOT, PIPE_ROOT, Pipe, PipeKind,
    bin, items_in, mobs_in, outlet_state, pipes,
};
use crate::construction::{Job, spawn_obj};
use crate::obj_damage::Anchored;
use crate::recycling::RecyclingJob;
use crate::tools::{Actor, DoAfter, ToolKind, consume, tell, use_tool};

/// `HIGH_TEMPERATURE_REQUIRED` is met by any lit welder here (`tool_usable` with heat 0).
const PIPE_SLICE_DS: f32 = 30.0;
const CONSTRUCT_WELD_DS: f32 = 8.0;
const MACHINE_SLICE_DS: f32 = 20.0;
/// `wait = world.time + 1 SECONDS`.
const DISPENSE_COOLDOWN: f64 = 1.0;
/// `DISPOSAL_PIPEDISPENSER` subtype.
pub const DISPOSAL_DISPENSER: &str = "/obj/machinery/pipedispenser/disposal";
const TRANSIT_DISPENSER: &str = "/obj/machinery/pipedispenser/disposal/transit_tube";
const CATEGORY: &str = "Disposal Pipes";

/// `/obj/structure/disposalconstruct`.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct DisposalConstruct {
    pub pipe_type: String,
    /// BYOND direction bits.
    pub dir: u8,
    pub anchored: bool,
}

impl DisposalConstruct {
    fn is_pipe(&self) -> bool {
        is_under(&self.pipe_type, PIPE_ROOT)
    }

    /// tg `get_disposal_dir`: the openings the finished pipe will have.
    fn disposal_dir(&self) -> u8 {
        if !self.is_pipe() {
            return 0;
        }
        Pipe::from_type(&self.pipe_type, self.dir, &[]).map_or(0, |p| p.dpdir)
    }
}

fn is_construct(world: &World, e: Entity) -> bool {
    world.get::<DisposalConstruct>(e).is_some()
}

fn dir_of(world: &World, e: Entity) -> u8 {
    world.get::<Facing>(e).map_or(pipes::SOUTH, |f| f.0 as u8)
}

fn set_dir(world: &mut World, e: Entity, dir: u8) {
    let Some(dir) = Dir::from_bits(dir) else {
        return;
    };
    match world.get_mut::<Facing>(e) {
        Some(mut facing) => {
            facing.set_if_neq(Facing(dir));
        }
        None => {
            world.entity_mut(e).insert(Facing(dir));
        }
    }
}

/// `icon_state` of the type `path` makes, from its prototype.
fn type_state(world: &World, path: &str) -> Option<String> {
    let proto = world.get_resource::<Prototypes>()?.get(path)?;
    proto.get::<DmAtom>().map(|a| a.icon_state.clone())
}

/// tg `update_icon_state` and the anchored density of a construct.
fn refresh(world: &mut World, e: Entity) {
    let Some(c) = world.get::<DisposalConstruct>(e).cloned() else {
        return;
    };
    let bin_like = is_under(&c.pipe_type, "/obj/machinery/disposal/bin");
    let state = if bin_like {
        format!("{}disposal", if c.anchored { "con" } else { "" })
    } else {
        let prefix = if c.is_pipe() { "con" } else { "" };
        format!(
            "{prefix}{}",
            type_state(world, &c.pipe_type).unwrap_or_default()
        )
    };
    let icon_id = world.get::<DmAtom>(e).and_then(|a| {
        world
            .get_resource::<IconIndex>()
            .and_then(|i| i.get(&a.icon, &state))
    });
    if let Some(id) = icon_id
        && let Some(mut look) = world.get_mut::<Appearance>(e)
        && look.icon_state != id
    {
        look.icon_state = id;
    }
    // `set_anchored`: `set_density(anchorvalue ? initial(pipe_type.density) : FALSE)`: bins,
    // chutes and outlets are dense, pipes are not.
    let machine = is_under(&c.pipe_type, MACHINE_ROOT) || is_under(&c.pipe_type, OUTLET_ROOT);
    let dense = c.anchored && machine;
    let has = world.get::<Density>(e).is_some();
    if dense && !has {
        world.entity_mut(e).insert(Density);
    } else if !dense && has {
        world.entity_mut(e).remove::<Density>();
    }
}

/// A construct of `pipe_type` on `pos`; `None` without its prototype.
pub fn spawn_construct(
    world: &mut World,
    pipe_type: &str,
    pos: TilePos,
    dir: u8,
    anchored: bool,
) -> Option<Entity> {
    let e = spawn_obj(world, DISPOSAL_CONSTRUCT, pos)?;
    world.entity_mut(e).insert((
        DisposalConstruct {
            pipe_type: pipe_type.to_owned(),
            dir,
            anchored,
        },
        Anchored(anchored),
    ));
    set_dir(world, e, dir);
    refresh(world, e);
    Some(e)
}

/// tg `can_place`: at most one bin, chute or outlet construct on a tile.
fn can_place(world: &World, pipe_type: &str, pos: TilePos) -> bool {
    if is_under(pipe_type, PIPE_ROOT) {
        return true;
    }
    !world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| {
            world
                .get::<DisposalConstruct>(e)
                .is_some_and(|c| !c.is_pipe())
        })
}

/// tg `/datum/element/simple_rotation` Alt-click: clockwise while loose.
pub fn click_alt(world: &mut World, client: Entity, _mob: Entity, target: Entity) -> bool {
    let Some(c) = world.get::<DisposalConstruct>(target).cloned() else {
        return false;
    };
    if c.anchored {
        tell(world, client, Span::Warning, "Сначала открепите от пола!");
        return true;
    }
    let dir = pipes::turn(c.dir, -90);
    if dir == 0 {
        return true;
    }
    world
        .get_mut::<DisposalConstruct>(target)
        .expect("read")
        .dir = dir;
    set_dir(world, target, dir);
    refresh(world, target);
    true
}

/// `UNDERFLOOR_INTERACTABLE`: the tile is off, or it is not a floor at all (`!isfloorturf`).
fn plating(world: &World, pos: TilePos) -> bool {
    accessibility(world, pos) >= 2
        || crate::construction::turf_at(world, pos)
            .and_then(|t| crate::construction::path_of(world, t))
            .is_none_or(|p| !is_under(&p, "/turf/open/floor"))
}

/// tg `wrench_act`, `welder_act`. True when the click was the construct's.
fn construct_tool(world: &mut World, actor: Actor, kind: ToolKind, target: Entity) -> bool {
    let (Some(c), Some(&pos)) = (
        world.get::<DisposalConstruct>(target).cloned(),
        world.get::<TilePos>(target),
    ) else {
        return false;
    };
    let name = pipe_name(world, &c.pipe_type);
    match kind {
        ToolKind::Wrench => {
            if c.anchored {
                set_anchored(world, target, false);
                tell(
                    world,
                    actor.client,
                    Span::Notice,
                    format!("Вы открепляете {name} от пола."),
                );
                return true;
            }
            if !plating(world, pos) {
                tell(
                    world,
                    actor.client,
                    Span::Warning,
                    format!("{name} можно прикрепить только к снятому покрытию!"),
                );
                return true;
            }
            let walled = crate::construction::turf_at(world, pos)
                .and_then(|t| crate::construction::path_of(world, t))
                .is_some_and(|p| is_under(&p, "/turf/closed"));
            if !c.is_pipe() && walled {
                tell(
                    world,
                    actor.client,
                    Span::Warning,
                    "На стенах можно строить только трубы!",
                );
                return true;
            }
            if c.is_pipe() {
                let dpdir = c.disposal_dir();
                let here: Vec<Entity> = world.resource::<SpatialIndex>().on_tile(pos).to_vec();
                for pipe in here {
                    let Some(existing) = world.get::<DisposalPipe>(pipe) else {
                        continue;
                    };
                    if existing.pipe.kind == PipeKind::Broken {
                        if dir_of(world, pipe) & dpdir != 0 {
                            world.despawn(pipe);
                        }
                    } else if existing.pipe.dpdir & dpdir != 0 {
                        tell(world, actor.client, Span::Warning, "Здесь уже есть труба!");
                        return true;
                    }
                }
            } else if !trunk_on(world, pos) {
                tell(
                    world,
                    actor.client,
                    Span::Warning,
                    format!("{name} требует магистраль под собой!"),
                );
                return true;
            }
            set_anchored(world, target, true);
            tell(
                world,
                actor.client,
                Span::Notice,
                format!("Вы прикрепляете {name} к полу."),
            );
            true
        }
        ToolKind::Welder => {
            if !c.anchored {
                tell(
                    world,
                    actor.client,
                    Span::Warning,
                    "Сначала прикрепите к полу!",
                );
                return true;
            }
            if !c.is_pipe() && machine_on(world, pos) {
                tell(
                    world,
                    actor.client,
                    Span::Warning,
                    "Здесь уже стоит машина мусоропровода!",
                );
                return true;
            }
            if use_tool(
                world,
                actor,
                target,
                CONSTRUCT_WELD_DS,
                1,
                Job::Recycling(RecyclingJob::WeldConstruct),
            ) {
                tell(
                    world,
                    actor.client,
                    Span::Notice,
                    format!("Вы начинаете приваривать {name}..."),
                );
            }
            true
        }
        _ => false,
    }
}

fn set_anchored(world: &mut World, e: Entity, anchored: bool) {
    world
        .get_mut::<DisposalConstruct>(e)
        .expect("checked")
        .anchored = anchored;
    world.entity_mut(e).insert(Anchored(anchored));
    refresh(world, e);
}

fn pipe_name(world: &World, path: &str) -> String {
    world
        .get_resource::<Prototypes>()
        .and_then(|p| p.get(path))
        .and_then(|p| p.get::<DmName>())
        .map_or_else(|| "трубу".to_owned(), |n| n.name.clone())
}

fn trunk_on(world: &World, pos: TilePos) -> bool {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| {
            world
                .get::<DisposalPipe>(e)
                .is_some_and(|p| p.pipe.kind == PipeKind::Trunk)
        })
}

fn machine_on(world: &World, pos: TilePos) -> bool {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| {
            world.get::<bin::DisposalBin>(e).is_some() || world.get::<DisposalOutlet>(e).is_some()
        })
}

/// `welder_act`: the construct becomes the finished pipe or machine.
fn finish_construct(world: &mut World, target: Entity) {
    let (Some(c), Some(&pos)) = (
        world.get::<DisposalConstruct>(target).cloned(),
        world.get::<TilePos>(target),
    ) else {
        return;
    };
    world.despawn(target);
    let Some(built) = spawn_obj(world, &c.pipe_type, pos) else {
        return;
    };
    set_dir(world, built, c.dir);
    if c.is_pipe() {
        built_pipe(world, built, &c.pipe_type, c.dir, pos);
    } else {
        built_machine(world, built, &c.pipe_type, pos, c.dir);
    }
}

/// Live state of a pipe built at runtime: what `init_disposals` gives map pipes.
pub fn built_pipe(world: &mut World, e: Entity, path: &str, dir: u8, pos: TilePos) {
    let Some(pipe) = Pipe::from_type(path, dir, &[]) else {
        return;
    };
    let linked = (pipe.kind == PipeKind::Trunk)
        .then(|| machine_for_trunk(world, e, pos))
        .flatten();
    world.entity_mut(e).insert(DisposalPipe { pipe, linked });
}

/// A trunk on `pos` links the bin, chute or outlet above it, and it the trunk.
fn machine_for_trunk(world: &mut World, trunk: Entity, pos: TilePos) -> Option<Entity> {
    let on_tile: Vec<Entity> = world.resource::<SpatialIndex>().on_tile(pos).to_vec();
    let mut linked = None;
    for e in on_tile {
        if let Some(mut b) = world.get_mut::<bin::DisposalBin>(e) {
            b.trunk = Some(trunk);
            linked = Some(e);
        }
    }
    for e in world.resource::<SpatialIndex>().on_tile(pos).to_vec() {
        if let Some(mut o) = world.get_mut::<DisposalOutlet>(e) {
            o.trunk = Some(trunk);
            linked = Some(e);
        }
    }
    linked
}

fn built_machine(world: &mut World, e: Entity, path: &str, pos: TilePos, dir: u8) {
    let Some(atom) = world.get::<DmAtom>(e).cloned() else {
        return;
    };
    let trunk = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&t| {
            world
                .get::<DisposalPipe>(t)
                .is_some_and(|p| p.pipe.kind == PipeKind::Trunk)
        });
    let icons = world.get_resource::<IconIndex>();
    if is_under(path, MACHINE_ROOT) {
        let mut state = bin::DisposalBin::new(is_under(path, CHUTE_ROOT), path, &atom, icons);
        state.trunk = trunk;
        if trunk.is_none() {
            state.pressure_charging = false;
            state.flush = false;
        }
        world.entity_mut(e).insert(state);
        bin::refresh_look(world, e);
    } else if is_under(path, OUTLET_ROOT) {
        let mut atom = atom;
        atom.dir = dir;
        let mut state = outlet_state(&atom, icons);
        state.trunk = trunk;
        world.entity_mut(e).insert(state);
    }
    if let Some(t) = trunk
        && let Some(mut pipe) = world.get_mut::<DisposalPipe>(t)
    {
        pipe.linked = Some(e);
    }
}

/// A placed pipe, trunk included: back to a construct (`welder_act`, `can_be_deconstructed`).
fn pipe_tool(world: &mut World, actor: Actor, target: Entity) -> bool {
    let Some(pipe) = world.get::<DisposalPipe>(target).cloned() else {
        return false;
    };
    if pipe.linked.is_some() && pipe.pipe.kind == PipeKind::Trunk {
        tell(
            world,
            actor.client,
            Span::Warning,
            "Сначала разберите машину мусоропровода над этой трубой!",
        );
        return true;
    }
    if use_tool(
        world,
        actor,
        target,
        PIPE_SLICE_DS,
        1,
        Job::Recycling(RecyclingJob::SlicePipe),
    ) {
        tell(
            world,
            actor.client,
            Span::Notice,
            "Вы начинаете разрезать трубу...",
        );
    }
    true
}

/// A placed bin, chute (`panel_open`, not charging) or outlet: the floorweld.
fn machine_tool(world: &mut World, actor: Actor, kind: ToolKind, target: Entity) -> bool {
    let outlet = world.get::<DisposalOutlet>(target).is_some();
    let Some(b) = world.get::<bin::DisposalBin>(target).cloned() else {
        if !outlet || kind != ToolKind::Welder {
            return false;
        }
        if use_tool(
            world,
            actor,
            target,
            MACHINE_SLICE_DS,
            1,
            Job::Recycling(RecyclingJob::SliceMachine),
        ) {
            tell(
                world,
                actor.client,
                Span::Notice,
                "Вы срезаете приварку выхода...",
            );
        }
        return true;
    };
    let busy = b.pressure_charging || b.full_pressure || b.flush;
    match kind {
        ToolKind::Screwdriver if !busy => {
            let open = crate::construction::panel_open(world, target);
            world
                .entity_mut(target)
                .insert(crate::construction::PanelOpen(!open));
            let text = if open {
                "Вы прикручиваете крепления силового разъёма."
            } else {
                "Вы откручиваете крепления силового разъёма."
            };
            tell(world, actor.client, Span::Notice, text);
            true
        }
        ToolKind::Welder if !busy && crate::construction::panel_open(world, target) => {
            if use_tool(
                world,
                actor,
                target,
                MACHINE_SLICE_DS,
                1,
                Job::Recycling(RecyclingJob::SliceMachine),
            ) {
                tell(world, actor.client, Span::Notice, "Вы срезаете приварку...");
            }
            true
        }
        _ => false,
    }
}

/// Tools on constructs and on placed pipes, bins and outlets. True when the click was theirs.
pub fn item_interaction(world: &mut World, actor: Actor, kind: ToolKind, target: Entity) -> bool {
    if is_construct(world, target) {
        return construct_tool(world, actor, kind, target);
    }
    if kind == ToolKind::Welder && world.get::<DisposalPipe>(target).is_some() {
        return pipe_tool(world, actor, target);
    }
    machine_tool(world, actor, kind, target)
}

/// End of a [`RecyclingJob`] welding `do_after`.
pub fn complete_weld(world: &mut World, action: &DoAfter, job: RecyclingJob) {
    let target = action.target;
    if let Some(item) = action.item
        && !consume(world, item, action.amount)
    {
        return;
    }
    match job {
        RecyclingJob::WeldConstruct => finish_construct(world, target),
        RecyclingJob::SlicePipe => slice_pipe(world, target),
        RecyclingJob::SliceMachine => slice_machine(world, target),
        _ => {}
    }
}

/// `disposalpipe/atom_deconstruct(disassembled)`.
fn slice_pipe(world: &mut World, target: Entity) {
    let (Some(pipe), Some(&pos), path) = (
        world.get::<DisposalPipe>(target).cloned(),
        world.get::<TilePos>(target),
        crate::construction::path_of(world, target).unwrap_or_default(),
    ) else {
        return;
    };
    world.despawn(target);
    spawn_construct(world, &path, pos, pipe.pipe.dir, true);
}

/// `disposal/on_deconstruction`, `outlet welder_act`: a loose, dense construct stays for a bin,
/// an anchored one for an outlet; contents spill onto the tile.
fn slice_machine(world: &mut World, target: Entity) {
    let Some(&pos) = world.get::<TilePos>(target) else {
        return;
    };
    let path = crate::construction::path_of(world, target).unwrap_or_default();
    let dir = dir_of(world, target);
    let bin = world.get::<bin::DisposalBin>(target).is_some();
    if bin {
        bin::eject(world, target);
        for item in items_in(world, target) {
            crate::storage::take_out(world, item, pos);
        }
        for mob in mobs_in(world, target) {
            bin::let_out(world, mob);
        }
    }
    let trunk = world
        .get::<bin::DisposalBin>(target)
        .and_then(|b| b.trunk)
        .or_else(|| world.get::<DisposalOutlet>(target).and_then(|o| o.trunk));
    world.despawn(target);
    if let Some(t) = trunk
        && let Some(mut pipe) = world.get_mut::<DisposalPipe>(t)
    {
        pipe.linked = None;
    }
    let Some(c) = spawn_construct(world, &path, pos, dir, !bin) else {
        return;
    };
    if bin {
        world.entity_mut(c).insert(Density);
    }
}

// --- The dispenser

/// `/obj/machinery/pipedispenser/disposal`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Default)]
pub struct DisposalDispenser {
    ready_at: f64,
}

/// Disposal dispensers get their state as they appear; transit tube ones are other machines.
pub fn init_dispensers(mut commands: Commands, added: Query<(Entity, &DmObj), Added<DmObj>>) {
    for (e, obj) in &added {
        if is_under(&obj.path, DISPOSAL_DISPENSER) && !is_under(&obj.path, TRANSIT_DISPENSER) {
            commands.entity(e).insert(DisposalDispenser::default());
        }
    }
}

pub struct DisposalDispenserWindow;

impl UiInterface for DisposalDispenserWindow {
    const NAME: &'static str = PipeDispenserUi::NAME;
    type Data = PipeDispenserData;
    type StaticData = PipeDispenserStatic;
}

/// Recipe index, the fitting's dir.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DispenseAct(usize, u8);

/// `ui_data`'s recipes: the base kinds facing north, a bendable one also bent.
fn categories() -> Vec<PipeCategory> {
    let mut recipes = Vec::new();
    for (i, r) in DISPOSAL_RECIPES.iter().enumerate() {
        let entry = |name: String, dir: u8| PipeRecipeEntry {
            pipe_name: name,
            pipe_index: i as u32 + 1,
            all_layers: false,
            dir,
        };
        recipes.push(entry(r.name.to_owned(), 1));
        if r.bendable {
            recipes.push(entry(format!("Bent {}", r.name), 5));
        }
    }
    vec![PipeCategory {
        cat_name: CATEGORY.to_owned(),
        recipes,
    }]
}

impl ServerUi for DisposalDispenserWindow {
    type Source = &'static DisposalDispenser;
    type UserParam = ();
    type Act = DispenseAct;

    fn static_data(_: &DisposalDispenser) -> PipeDispenserStatic {
        PipeDispenserStatic {
            paint_colors: Vec::new(),
            categories: categories(),
        }
    }

    fn data(_: &DisposalDispenser, _: Entity, _: &()) -> PipeDispenserData {
        PipeDispenserData::default()
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<DispenseAct, UiReject> {
        if action != "pipe_type" {
            return Err(UiReject::UnknownAction);
        }
        if params.get("category") != Some(CATEGORY) {
            return Err(UiReject::BadParams("category".to_owned()));
        }
        // "No spawning arbitrary paths": an index into the recipes only.
        let index = params.number("pipe_type")?;
        if index.fract() != 0.0 || index < 1.0 || index > DISPOSAL_RECIPES.len() as f64 {
            return Err(UiReject::BadParams("pipe_type".to_owned()));
        }
        let dir = params.number("pipe_dir")?;
        let dir = [1u8, 2, 4, 8, 5, 6, 9, 10]
            .into_iter()
            .find(|&d| f64::from(d) == dir)
            .ok_or_else(|| UiReject::BadParams("pipe_dir".to_owned()))?;
        Ok(DispenseAct(index as usize - 1, dir))
    }
}

pub fn dispenser_acts(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<DisposalDispenserWindow>>>,
) {
    let pending: Vec<(Entity, Entity, DispenseAct)> = acts
        .read(world.resource::<Messages<UiActed<DisposalDispenserWindow>>>())
        .map(|a| (a.client, a.source, a.act))
        .collect();
    for (client, source, DispenseAct(index, dir)) in pending {
        dispense(world, client, source, index, dir);
    }
}

fn dispense(world: &mut World, client: Entity, source: Entity, index: usize, dir: u8) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(mut d) = world.get_mut::<DisposalDispenser>(source) else {
        return;
    };
    if now < d.ready_at {
        return;
    }
    d.ready_at = now + DISPENSE_COOLDOWN;
    let (Some(&pos), Some(recipe)) = (world.get::<TilePos>(source), DISPOSAL_RECIPES.get(index))
    else {
        return;
    };
    if !can_place(world, recipe.path, pos) {
        tell(world, client, Span::Warning, "Здесь недостаточно места!");
        return;
    }
    spawn_construct(world, recipe.path, pos, dir, false);
}

/// A click with an empty hand: the window.
pub fn dispenser_interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<DisposalDispenser>(target).is_none() {
        return false;
    }
    let title = world
        .get::<DmName>(target)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default();
    world.write_message(OpenUi::new::<DisposalDispenserWindow>(
        client, mob, target, title,
    ));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_construct_knows_its_openings() {
        let c = DisposalConstruct {
            pipe_type: "/obj/structure/disposalpipe/junction".to_owned(),
            dir: pipes::NORTH,
            anchored: false,
        };
        assert_eq!(c.disposal_dir(), pipes::NORTH | pipes::EAST | pipes::SOUTH);
        let bin = DisposalConstruct {
            pipe_type: "/obj/machinery/disposal/bin".to_owned(),
            dir: pipes::SOUTH,
            anchored: false,
        };
        assert_eq!(bin.disposal_dir(), 0);
        assert!(!bin.is_pipe());
    }
}
