//! The modular shield (server): tg `/obj/machinery/modular_shield_generator` (and its `gate`),
//! the `/obj/machinery/modular_shield/module` network (node, cable, charger, relay, well), the
//! `/obj/structure/emergency_shield/modular` field (`code/game/machinery/modular_shield.dm`) and
//! the control console `/obj/machinery/computer/modular_shield`
//! (`code/game/machinery/computer/modular_shield.dm`), with tgui's `ModularShieldGen` and
//! `ModularShieldConsole`.
//!
//! - Numbers (`RefreshParts`, `calculate_*`): innate strength `40 + 20 * capacitor tiers`, regen
//!   `3 + 2 * servo tiers`, radius `3 + 0.25 * laser tiers`; chargers add `2 * servo tiers` of
//!   regen, wells `20 * capacitor tiers` of strength, relays `0.25 * laser tiers` of radius.
//!   `max_radius` halves (at least 3) unless exterior-only or a gate. Inactive regen is the max
//!   (a quarter while recovering); active it is `max / (0.5 + 2 * radius / max_radius)`, halved
//!   when the interior penalty applies. A radius above the max turns it off.
//! - Every machine tick a powered generator regains `regen * seconds` up to its max; full, it
//!   stops recovering. It starts recovering at 0 strength. Without power it turns off.
//! - On (window, console, not initiating or recovering): a circle of fields on the open turfs at
//!   `radius` around it (outside `radius - 1`; the midpoint perimeter from radius 10; exterior
//!   only skips floors), or for a gate a line along its facing up to `max_radius` tiles until a
//!   closed turf or another field. Fields block air; after 2 s they turn dense. Damage to a
//!   field drains the generator instead; under 5 strength it turns off and recovers.
//! - Modules connect with a multitool to the generator or node they face (boosters not through
//!   cables); a node passes its generator on to what is connected through it. A wrench with the
//!   panel open turns a module or gate clockwise and reconnects it.
//! - A multitool saves the generator to its buffer; on the console it links it. The console lists
//!   linked generators and toggles or renames them.
// shortcut: no wires (`WIRE_HACK`), alpha by strength, colour, hot-atmos, EMP, explosion or hulk
// drain amounts (all damage drains by its amount), explosion blocking, shuttle-move shutdown,
// monkey button mashing, cable sprites by connection; the console is not built from a frame.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmName, DmObj};
use ss13_shared::chat::Span;
use ss13_shared::{Dir, Facing, TilePos};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};
use ss13_world::SpatialIndex;

use crate::construction::{PanelOpen, path_of, spawn_obj, tile_changed, turf_at};
use crate::modular_shield_ui::{
    ID, LinkedGenerator, ModularShieldConsoleData, ModularShieldConsoleUi, ModularShieldGenData,
    ModularShieldGenUi, NAME, NEW_RADIUS, RENAME, SET_RADIUS, TOGGLE_EXTERIOR, TOGGLE_SHIELDS,
};
use crate::obj_damage::is_under;
use crate::power::{MACHINE_TICK_SECS, MachineTicks, Unpowered};
use crate::stock_parts::PartKind;
use crate::tools::{ToolKind, multitool_buffer, set_multitool_buffer, tell, tool_of};

pub const GENERATOR: &str = "/obj/machinery/modular_shield_generator";
pub const GATE: &str = "/obj/machinery/modular_shield_generator/gate";
pub const MODULE: &str = "/obj/machinery/modular_shield/module";
const NODE: &str = "/obj/machinery/modular_shield/module/node";
const CABLE: &str = "/obj/machinery/modular_shield/module/node/cable";
const CHARGER: &str = "/obj/machinery/modular_shield/module/charger";
const RELAY: &str = "/obj/machinery/modular_shield/module/relay";
const WELL: &str = "/obj/machinery/modular_shield/module/well";
pub const CONSOLE: &str = "/obj/machinery/computer/modular_shield";
pub const FIELD: &str = "/obj/structure/emergency_shield/modular";
/// `finish_field` after `2 SECONDS`.
pub const INITIATE_SECS: f64 = 2.0;
/// `shield_drain`: under this the shield fails.
const FAIL_BELOW: f32 = 5.0;
/// From this radius the perimeter is the midpoint circle.
const LARGE_RADIUS: f32 = 10.0;

/// A generator's live state (tg vars).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct ShieldGenerator {
    pub gate: bool,
    pub active: bool,
    /// `initiating`: when the fields turn dense.
    pub initiating_until: Option<f64>,
    pub recovering: bool,
    pub max_strength: f32,
    pub stored_strength: f32,
    pub max_regeneration: f32,
    pub current_regeneration: f32,
    pub max_radius: f32,
    pub radius: f32,
    pub exterior_only: bool,
    pub internal_penalty: bool,
    pub shields: Vec<Entity>,
    /// `connected_modules`.
    pub modules: Vec<Entity>,
    pub display_name: String,
}

impl ShieldGenerator {
    fn new(gate: bool) -> Self {
        Self {
            gate,
            active: false,
            initiating_until: None,
            recovering: true,
            max_strength: 40.0,
            stored_strength: 0.0,
            max_regeneration: 3.0,
            current_regeneration: 0.75,
            max_radius: 3.0,
            radius: 3.0,
            exterior_only: false,
            internal_penalty: !gate,
            shields: Vec::new(),
            modules: Vec::new(),
            display_name: if gate {
                "Shield Gate".into()
            } else {
                "Shield Generator".into()
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleKind {
    Node,
    Cable,
    Charger,
    Relay,
    Well,
}

impl ModuleKind {
    fn of_path(path: &str) -> Option<Self> {
        Some(if is_under(path, CABLE) {
            Self::Cable
        } else if is_under(path, NODE) {
            Self::Node
        } else if is_under(path, CHARGER) {
            Self::Charger
        } else if is_under(path, RELAY) {
            Self::Relay
        } else if is_under(path, WELL) {
            Self::Well
        } else {
            return None;
        })
    }

    fn is_node(self) -> bool {
        matches!(self, Self::Node | Self::Cable)
    }

    fn is_booster(self) -> bool {
        matches!(self, Self::Charger | Self::Relay | Self::Well)
    }
}

/// A network module.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct ShieldModule {
    pub kind: ModuleKind,
    pub generator: Option<Entity>,
    /// `connected_node`.
    pub node: Option<Entity>,
    /// A node's `connected_through_us`.
    pub through_us: Vec<Entity>,
}

/// A field tile and its generator.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModularShield {
    pub generator: Entity,
}

/// The console's linked generators.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ShieldConsole {
    pub generators: Vec<Entity>,
}

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // Multitool (buffer, connect, link) and the panel-open wrench rotation (before
    // `crate::machine_frame`).
    app.add_click_handler(ClickPhase::HeldOnTarget, 1559, |w, c| {
        c.held
            .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target))
    });
    // `ui_interact` of the generator and the console.
    app.add_click_handler(ClickPhase::EmptyHand, 1999, |w, c| {
        interact(w, c.client, c.mob, c.target)
    });
    app.register_ui::<ShieldGenWindow>()
        .register_ui::<ShieldConsoleWindow>()
        .add_systems(
            ss13_world::MapInit,
            setup_mapped.after(crate::power::init_power),
        )
        .add_systems(
            Update,
            (
                gen_acts.in_set(UiSystems::Handle),
                console_acts.in_set(UiSystems::Handle),
                finish_fields.run_if(any_with_component::<ShieldGenerator>),
                machine_tick.run_if(any_with_component::<ShieldGenerator>),
            ),
        );
}

fn setup_mapped(world: &mut World) {
    let mut q = world.query_filtered::<(Entity, &DmObj), With<ss13_world::Mapped>>();
    let found: Vec<Entity> = q
        .iter(world)
        .filter(|(_, o)| {
            is_under(&o.path, GENERATOR) || is_under(&o.path, MODULE) || is_under(&o.path, CONSOLE)
        })
        .map(|(e, _)| e)
        .collect();
    for e in found {
        setup(world, e);
    }
    // `module/Initialize`: `try_connect()` once everything exists.
    let mut q = world.query_filtered::<Entity, With<ShieldModule>>();
    let modules: Vec<Entity> = q.iter(world).collect();
    for m in modules {
        try_connect(world, m);
    }
}

/// `Initialize`; also for machines built from a frame (`crate::machine_frame::setup_built`),
/// which connect at once.
pub fn setup(world: &mut World, e: Entity) {
    let Some(path) = path_of(world, e) else {
        return;
    };
    if is_under(&path, GENERATOR) {
        world
            .entity_mut(e)
            .insert(ShieldGenerator::new(is_under(&path, GATE)));
        refresh(world, e);
    } else if let Some(kind) = ModuleKind::of_path(&path) {
        world.entity_mut(e).insert(ShieldModule {
            kind,
            generator: None,
            node: None,
            through_us: Vec::new(),
        });
        if world.get::<ss13_world::Mapped>(e).is_none() {
            try_connect(world, e);
        }
    } else if is_under(&path, CONSOLE) {
        world.entity_mut(e).insert(ShieldConsole::default());
    }
}

fn gen_of(world: &World, e: Entity) -> Option<&ShieldGenerator> {
    world.get::<ShieldGenerator>(e)
}

fn module(world: &World, e: Entity) -> Option<&ShieldModule> {
    world.get::<ShieldModule>(e)
}

fn facing(world: &World, e: Entity) -> Dir {
    world.get::<Facing>(e).map_or(Dir::South, |f| f.0)
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

/// The generator's own numbers (`RefreshParts`): (strength, regen, radius).
pub fn innate(r: &crate::stock_parts::PartRatings) -> (f32, f32, f32) {
    (
        40.0 + 20.0 * r.sum(PartKind::Capacitor) as f32,
        3.0 + 2.0 * r.sum(PartKind::Servo) as f32,
        3.0 + 0.25 * r.sum(PartKind::MicroLaser) as f32,
    )
}

/// A booster's (regen, strength, radius) boost.
fn boost(world: &World, m: Entity) -> (f32, f32, f32) {
    let Some(kind) = module(world, m).map(|m| m.kind) else {
        return (0.0, 0.0, 0.0);
    };
    let r = crate::stock_parts::ratings(world, m);
    match kind {
        ModuleKind::Charger => (2.0 * r.sum(PartKind::Servo) as f32, 0.0, 0.0),
        ModuleKind::Well => (0.0, 20.0 * r.sum(PartKind::Capacitor) as f32, 0.0),
        ModuleKind::Relay => (0.0, 0.0, 0.25 * r.sum(PartKind::MicroLaser) as f32),
        _ => (0.0, 0.0, 0.0),
    }
}

/// `current_regeneration` of the generator's state (`calculate_regeneration`).
pub fn regeneration(g: &ShieldGenerator) -> f32 {
    if !g.active {
        return if g.recovering {
            g.max_regeneration * 0.25
        } else {
            g.max_regeneration
        };
    }
    let mut regen = g.max_regeneration / (0.5 + (g.radius * 2.0) / g.max_radius);
    if !g.exterior_only && g.internal_penalty {
        regen *= 0.5;
    }
    regen
}

/// `RefreshParts` + `calculate_boost`: every number again from its parts and modules.
pub fn refresh(world: &mut World, e: Entity) {
    if gen_of(world, e).is_none() {
        return;
    }
    let (strength, regen, radius) = innate(&crate::stock_parts::ratings(world, e));
    let modules: Vec<Entity> = gen_of(world, e)
        .map(|g| g.modules.clone())
        .unwrap_or_default();
    let mut boosts = (0.0, 0.0, 0.0);
    for m in modules
        .iter()
        .filter(|&&m| module(world, m).is_some_and(|s| s.generator == Some(e)))
    {
        let b = boost(world, *m);
        boosts = (boosts.0 + b.0, boosts.1 + b.1, boosts.2 + b.2);
    }
    let Some(mut g) = world.get_mut::<ShieldGenerator>(e) else {
        return;
    };
    g.max_regeneration = regen + boosts.0;
    g.max_strength = strength + boosts.1;
    let mut max_radius = radius + boosts.2;
    if !g.exterior_only && g.internal_penalty {
        max_radius = (max_radius * 0.5).max(3.0);
    }
    g.max_radius = max_radius;
    let too_far = g.radius > max_radius;
    if too_far {
        g.radius = max_radius.round();
    }
    g.current_regeneration = regeneration(&g);
    if too_far {
        deactivate(world, e);
    }
}

/// `deactivate_shields`.
pub fn deactivate(world: &mut World, e: Entity) {
    let shields = world
        .get_mut::<ShieldGenerator>(e)
        .map(|mut g| {
            g.active = false;
            g.initiating_until = None;
            let r = regeneration(&g);
            g.current_regeneration = r;
            std::mem::take(&mut g.shields)
        })
        .unwrap_or_default();
    for s in shields {
        if let Some(&pos) = world.get::<TilePos>(s) {
            world.despawn(s);
            tile_changed(world, pos);
        }
    }
}

/// `toggle_shields`.
pub fn toggle(world: &mut World, e: Entity) {
    let Some(g) = gen_of(world, e).cloned() else {
        return;
    };
    if g.initiating_until.is_some() {
        return;
    }
    if g.active {
        deactivate(world, e);
    } else if !g.recovering {
        activate(world, e);
    }
}

fn open_turf(world: &World, pos: TilePos) -> Option<String> {
    turf_at(world, pos)
        .and_then(|t| path_of(world, t))
        .filter(|p| is_under(p, "/turf/open"))
}

fn has_field(world: &World, pos: TilePos) -> bool {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| world.get::<ModularShield>(e).is_some())
}

/// `circle_range_turfs(center, radius)`: offsets within `radius * (radius + 0.5)`.
pub fn circle(radius: f32) -> Vec<(i32, i32)> {
    let r = radius.floor() as i32;
    let rsq = radius * (radius + 0.5);
    let mut out = Vec::new();
    for dy in -r..=r {
        for dx in -r..=r {
            if (dx * dx + dy * dy) as f32 <= rsq {
                out.push((dx, dy));
            }
        }
    }
    out
}

/// `get_perimeter(center, radius)`: the midpoint circle.
pub fn perimeter(radius: f32) -> Vec<(i32, i32)> {
    let r = radius.floor() as i32;
    if r < 1 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let (mut dx, mut dy, mut t1) = (r, 0, r / 16);
    while dx >= dy {
        for p in [
            (dx, dy),
            (-dx, dy),
            (dx, -dy),
            (-dx, -dy),
            (dy, dx),
            (-dy, dx),
            (dy, -dx),
            (-dy, -dx),
        ] {
            if !out.contains(&p) {
                out.push(p);
            }
        }
        dy += 1;
        t1 += dy;
        let t2 = t1 - dx;
        if t2 > 0 {
            t1 = t2;
            dx -= 1;
        }
    }
    out
}

/// The tiles a generator's field covers now.
fn field_tiles(world: &World, e: Entity, g: &ShieldGenerator) -> Vec<TilePos> {
    let Some(&c) = world.get::<TilePos>(e) else {
        return Vec::new();
    };
    let at = |(dx, dy): (i32, i32)| TilePos::new(c.x + dx, c.y + dy, c.z);
    let ring: Vec<TilePos> = if g.radius >= LARGE_RADIUS {
        perimeter(g.radius).into_iter().map(at).collect()
    } else {
        let inside = circle(g.radius - 1.0);
        circle(g.radius)
            .into_iter()
            .filter(|o| !inside.contains(o))
            .map(at)
            .collect()
    };
    ring.into_iter()
        .filter(|&p| {
            open_turf(world, p)
                .is_some_and(|t| !(g.exterior_only && is_under(&t, "/turf/open/floor")))
                && !has_field(world, p)
        })
        .collect()
}

/// A gate's line along its facing (`gate/activate_shields`).
fn gate_tiles(world: &World, e: Entity, g: &ShieldGenerator) -> Vec<TilePos> {
    let Some(&start) = world.get::<TilePos>(e) else {
        return Vec::new();
    };
    let dir = facing(world, e);
    let mut cur = start;
    let mut out = Vec::new();
    for _ in 0..(g.max_radius.round() as i32).max(0) {
        if open_turf(world, cur).is_none() || has_field(world, cur) {
            break;
        }
        out.push(cur);
        cur = cur.step(dir);
    }
    out
}

/// `activate_shields`.
pub fn activate(world: &mut World, e: Entity) {
    let Some(mut g) = gen_of(world, e).cloned() else {
        return;
    };
    if g.active || world.get::<Unpowered>(e).is_some() {
        return;
    }
    let tiles = if g.gate {
        gate_tiles(world, e, &g)
    } else {
        field_tiles(world, e, &g)
    };
    if g.gate {
        g.radius = tiles.len() as f32;
        if tiles.is_empty() {
            // "we couldnt even generate a single tile".
            return;
        }
    }
    let shields: Vec<Entity> = tiles
        .into_iter()
        .map(|pos| spawn_field(world, pos, e))
        .collect();
    g.active = true;
    g.initiating_until = Some(now(world) + INITIATE_SECS);
    g.shields = shields;
    g.current_regeneration = regeneration(&g);
    world.entity_mut(e).insert(g);
}

fn spawn_field(world: &mut World, pos: TilePos, generator: Entity) -> Entity {
    let field = spawn_obj(world, FIELD, pos)
        .unwrap_or_else(|| world.spawn((Name::new("modular energy shield"), pos)).id());
    // Not dense until `finish_field`.
    world
        .entity_mut(field)
        .remove::<ss13_world::Density>()
        .insert((
            ModularShield { generator },
            ss13_atmos::AtmosBlocker(ss13_atmos::AtmosPass::No),
        ));
    tile_changed(world, pos);
    field
}

fn finish_fields(world: &mut World) {
    let t = now(world);
    let due: Vec<Entity> = world
        .query::<(Entity, &ShieldGenerator)>()
        .iter(world)
        .filter(|(_, g)| g.initiating_until.is_some_and(|at| at <= t))
        .map(|(e, _)| e)
        .collect();
    for e in due {
        let shields = world
            .get_mut::<ShieldGenerator>(e)
            .map(|mut g| {
                g.initiating_until = None;
                g.shields.clone()
            })
            .unwrap_or_default();
        for s in shields {
            if let Some(&pos) = world.get::<TilePos>(s) {
                world.entity_mut(s).insert(ss13_world::Density);
                tile_changed(world, pos);
            }
        }
    }
}

/// `shield_drain`.
pub fn drain(world: &mut World, e: Entity, amount: f32) {
    let failed = world.get_mut::<ShieldGenerator>(e).is_some_and(|mut g| {
        g.stored_strength -= amount;
        if g.stored_strength < FAIL_BELOW {
            g.recovering = true;
            g.stored_strength = 0.0;
            true
        } else {
            false
        }
    });
    if failed {
        deactivate(world, e);
    }
}

/// Damage to a modular field (from `crate::obj_damage`): the generator takes it. True when
/// `target` is one.
pub fn absorb(world: &mut World, target: Entity, amount: f32, brute_or_burn: bool) -> bool {
    let Some(field) = world.get::<ModularShield>(target).copied() else {
        return false;
    };
    if gen_of(world, field.generator).is_none() {
        // `if(isnull(shield_generator)) qdel(src)`.
        if let Some(&pos) = world.get::<TilePos>(target) {
            world.despawn(target);
            tile_changed(world, pos);
        }
        return true;
    }
    if brute_or_burn {
        drain(world, field.generator, amount);
    }
    true
}

/// `process(seconds_per_tick)` of one generator; `power_change` turns it off without power.
pub fn process(world: &mut World, e: Entity, seconds: f32) {
    if world.get::<Unpowered>(e).is_some() {
        if gen_of(world, e).is_some_and(|g| g.active) {
            deactivate(world, e);
        }
        return;
    }
    let Some(mut g) = world.get_mut::<ShieldGenerator>(e) else {
        return;
    };
    g.stored_strength = (g.stored_strength + g.current_regeneration * seconds).min(g.max_strength);
    if g.stored_strength >= g.max_strength && g.recovering {
        g.recovering = false;
        let r = regeneration(&g);
        g.current_regeneration = r;
    }
}

fn machine_tick(world: &mut World, mut last_tick: Local<u64>) {
    let ticks = world.resource::<MachineTicks>().0;
    if ticks == *last_tick {
        return;
    }
    let elapsed = ticks.saturating_sub(*last_tick).min(3) as f32;
    *last_tick = ticks;
    let gens: Vec<Entity> = world
        .query_filtered::<Entity, With<ShieldGenerator>>()
        .iter(world)
        .collect();
    for e in gens {
        process(world, e, elapsed * MACHINE_TICK_SECS as f32);
    }
}

/// `try_connect`: the generator or node the module faces.
pub fn try_connect(world: &mut World, m: Entity) -> &'static str {
    let Some(s) = module(world, m).cloned() else {
        return "";
    };
    if s.generator.is_some() || s.node.is_some() {
        return "already connected to something!";
    }
    let Some(&pos) = world.get::<TilePos>(m) else {
        return "no connection!";
    };
    let front = pos.step(facing(world, m));
    let on_front: Vec<Entity> = world.resource::<SpatialIndex>().on_tile(front).to_vec();
    if let Some(&g) = on_front.iter().find(|&&o| gen_of(world, o).is_some()) {
        set_generator(world, m, Some(g));
        return "connected to generator";
    }
    let Some(&node) = on_front
        .iter()
        .find(|&&o| o != m && module(world, o).is_some_and(|n| n.kind.is_node()))
    else {
        return "no connection!";
    };
    let node_state = module(world, node).cloned().expect("a node");
    if node_state.kind == ModuleKind::Cable && s.kind.is_booster() {
        return "cant connect";
    }
    if let Some(mut n) = world.get_mut::<ShieldModule>(node)
        && !n.through_us.contains(&m)
    {
        n.through_us.push(m);
    }
    if let Some(mut own) = world.get_mut::<ShieldModule>(m) {
        own.node = Some(node);
    }
    match node_state.generator {
        Some(g) if gen_of(world, g).is_some() => {
            set_generator(world, m, Some(g));
            "connected to generator"
        }
        _ => "connected to node",
    }
}

/// Joins (or leaves) a generator, with whatever is connected through a node.
fn set_generator(world: &mut World, m: Entity, g: Option<Entity>) {
    let Some(s) = module(world, m).cloned() else {
        return;
    };
    if let Some(old) = s.generator
        && old != g.unwrap_or(Entity::PLACEHOLDER)
        && let Some(mut og) = world.get_mut::<ShieldGenerator>(old)
    {
        og.modules.retain(|&x| x != m);
    }
    if let Some(mut own) = world.get_mut::<ShieldModule>(m) {
        own.generator = g;
    }
    if let Some(g) = g
        && let Some(mut ng) = world.get_mut::<ShieldGenerator>(g)
        && !ng.modules.contains(&m)
    {
        ng.modules.push(m);
    }
    if s.kind.is_node() {
        for child in s.through_us {
            if module(world, child).is_some_and(|c| c.node == Some(m)) {
                set_generator(world, child, g);
            }
        }
    }
    for touched in [s.generator, g].into_iter().flatten() {
        refresh(world, touched);
    }
}

/// Leaves its generator and node (a wrench turn, destruction).
fn disconnect(world: &mut World, m: Entity) {
    let Some(s) = module(world, m).cloned() else {
        return;
    };
    set_generator(world, m, None);
    if let Some(node) = s.node
        && let Some(mut n) = world.get_mut::<ShieldModule>(node)
    {
        n.through_us.retain(|&x| x != m);
    }
    if let Some(mut own) = world.get_mut::<ShieldModule>(m) {
        own.node = None;
    }
}

/// `Destroy` of a generator, module or console part (from `crate::obj_damage::deconstruct`).
pub fn on_destroy(world: &mut World, e: Entity) {
    if let Some(g) = gen_of(world, e).cloned() {
        deactivate(world, e);
        for m in g.modules {
            if let Some(mut s) = world.get_mut::<ShieldModule>(m) {
                s.generator = None;
            }
        }
        let mut q = world.query::<&mut ShieldConsole>();
        for mut c in q.iter_mut(world) {
            c.generators.retain(|&x| x != e);
        }
    } else if let Some(s) = module(world, e).cloned() {
        disconnect(world, e);
        for child in s.through_us {
            if let Some(mut c) = world.get_mut::<ShieldModule>(child)
                && c.node == Some(e)
            {
                c.node = None;
            }
            set_generator(world, child, None);
        }
    }
}

fn clockwise(dir: Dir) -> Dir {
    match dir {
        Dir::North => Dir::East,
        Dir::East => Dir::South,
        Dir::South => Dir::West,
        _ => Dir::North,
    }
}

fn panel_open(world: &World, e: Entity) -> bool {
    world.get::<PanelOpen>(e).is_some_and(|p| p.0)
}

/// The held item on a generator, module or console. True when it was ours.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    e: Entity,
) -> bool {
    let _ = mob;
    let is_gen = gen_of(world, e).is_some();
    let is_module = module(world, e).is_some();
    let is_console = world.get::<ShieldConsole>(e).is_some();
    if !is_gen && !is_module && !is_console {
        return false;
    }
    let Some((kind, _)) = tool_of(world, held) else {
        return false;
    };
    match kind {
        ToolKind::Multitool if is_gen => {
            set_multitool_buffer(world, held, e);
            tell(world, client, Span::Notice, "saved to buffer");
            true
        }
        ToolKind::Multitool if is_module => {
            let text = try_connect(world, e);
            tell(world, client, Span::Notice, text);
            true
        }
        ToolKind::Multitool => {
            let Some(g) = multitool_buffer(world, held).filter(|&g| gen_of(world, g).is_some())
            else {
                return false;
            };
            if let Some(mut c) = world.get_mut::<ShieldConsole>(e)
                && !c.generators.contains(&g)
            {
                c.generators.push(g);
            }
            world
                .entity_mut(held)
                .remove::<crate::tools::MultitoolBuffer>();
            tell(
                world,
                client,
                Span::Notice,
                "You upload the data from the multitool buffer.",
            );
            true
        }
        // `default_change_direction_wrench` (modules reconnect; gates turn).
        ToolKind::Wrench if is_module || gen_of(world, e).is_some_and(|g| g.gate) => {
            if !panel_open(world, e) {
                return false;
            }
            let dir = clockwise(facing(world, e));
            world.entity_mut(e).insert(Facing(dir));
            let name = crate::medical::name_of(world, e);
            tell(
                world,
                client,
                Span::Notice,
                format!("Вы поворачиваете {}.", crate::obj_damage::strip(&name)),
            );
            if is_module {
                // A node first lets go of what is connected through it.
                if let Some(s) = module(world, e).cloned()
                    && s.kind.is_node()
                {
                    for child in s.through_us {
                        set_generator(world, child, None);
                    }
                }
                disconnect(world, e);
                try_connect(world, e);
            }
            true
        }
        _ => false,
    }
}

/// An empty hand: the generator's window (not a gate's) or the console's.
pub fn interact(world: &mut World, client: Entity, mob: Entity, e: Entity) -> bool {
    let title = world
        .get::<DmName>(e)
        .map(|n| crate::obj_damage::strip(&n.name).to_owned())
        .unwrap_or_default();
    if let Some(g) = gen_of(world, e) {
        if g.gate {
            return false;
        }
        world.write_message(OpenUi::new::<ShieldGenWindow>(client, mob, e, title));
        return true;
    }
    if world.get::<ShieldConsole>(e).is_some() {
        world.write_message(OpenUi::new::<ShieldConsoleWindow>(client, mob, e, title));
        return true;
    }
    false
}

/// `ui_act("set_radius")`: clamped to 1..max, whole from 10; not while active.
pub fn set_radius(world: &mut World, e: Entity, radius: f32) {
    let Some(mut g) = world.get_mut::<ShieldGenerator>(e) else {
        return;
    };
    if g.active || !radius.is_finite() {
        return;
    }
    let r = radius.clamp(1.0, g.max_radius.max(1.0));
    g.radius = if r >= LARGE_RADIUS { r.round() } else { r };
    let regen = regeneration(&g);
    g.current_regeneration = regen;
}

pub struct ShieldGenWindow;

impl ss13_tgui::UiInterface for ShieldGenWindow {
    const NAME: &'static str = ModularShieldGenUi::NAME;
    type Data = ModularShieldGenData;
    type StaticData = ();
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GenAct {
    SetRadius(f64),
    Toggle,
    ToggleExterior,
}

impl ServerUi for ShieldGenWindow {
    type Source = &'static ShieldGenerator;
    type UserParam = ();
    type Act = GenAct;

    fn static_data(_: &ShieldGenerator) {}

    fn data(g: &ShieldGenerator, _: Entity, _: &()) -> ModularShieldGenData {
        ModularShieldGenData {
            max_radius: g.max_radius,
            current_radius: g.radius,
            max_strength: g.max_strength,
            max_regeneration: g.max_regeneration,
            current_regeneration: g.current_regeneration,
            current_strength: g.stored_strength,
            active: g.active,
            recovering: g.recovering,
            exterior_only: g.exterior_only,
            initiating_field: g.initiating_until.is_some(),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<GenAct, UiReject> {
        match action {
            SET_RADIUS => Ok(GenAct::SetRadius(params.number(NEW_RADIUS)?)),
            TOGGLE_SHIELDS => Ok(GenAct::Toggle),
            TOGGLE_EXTERIOR => Ok(GenAct::ToggleExterior),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

fn gen_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<ShieldGenWindow>>>) {
    let pending: Vec<(Entity, GenAct)> = acts
        .read(world.resource::<Messages<UiActed<ShieldGenWindow>>>())
        .map(|a| (a.source, a.act))
        .collect();
    for (e, act) in pending {
        if gen_of(world, e).is_none() {
            continue;
        }
        match act {
            GenAct::SetRadius(r) => set_radius(world, e, r as f32),
            GenAct::Toggle => toggle(world, e),
            GenAct::ToggleExterior => {
                if let Some(mut g) = world.get_mut::<ShieldGenerator>(e) {
                    g.exterior_only = !g.exterior_only;
                }
                refresh(world, e);
            }
        }
    }
}

pub struct ShieldConsoleWindow;

impl ss13_tgui::UiInterface for ShieldConsoleWindow {
    const NAME: &'static str = ModularShieldConsoleUi::NAME;
    type Data = ModularShieldConsoleData;
    type StaticData = ();
}

#[derive(Clone, Debug, PartialEq)]
pub enum ConsoleAct {
    Toggle(usize),
    Rename(usize, String),
}

impl ServerUi for ShieldConsoleWindow {
    type Source = &'static ShieldConsole;
    type UserParam =
        Query<'static, 'static, (&'static ShieldGenerator, Option<&'static Unpowered>)>;
    type Act = ConsoleAct;

    fn static_data(_: &ShieldConsole) {}

    fn data(
        c: &ShieldConsole,
        _: Entity,
        gens: &bevy::ecs::system::SystemParamItem<'_, '_, Self::UserParam>,
    ) -> ModularShieldConsoleData {
        let generators = c
            .generators
            .iter()
            .enumerate()
            .filter_map(|(i, &e)| {
                let (g, unpowered) = gens.get(e).ok()?;
                Some(LinkedGenerator {
                    name: g.display_name.clone(),
                    id: i as u32 + 1,
                    max_strength: g.max_strength,
                    current_strength: g.stored_strength,
                    active: g.active,
                    recovering: g.recovering || g.initiating_until.is_some(),
                    current_regeneration: g.current_regeneration,
                    inactive: unpowered.is_some(),
                })
            })
            .collect();
        ModularShieldConsoleData { generators }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<ConsoleAct, UiReject> {
        let id = params.number(ID)?;
        if !(1.0..=1024.0).contains(&id) {
            return Err(UiReject::BadParams("id: not a generator".into()));
        }
        let index = id as usize - 1;
        match action {
            TOGGLE_SHIELDS => Ok(ConsoleAct::Toggle(index)),
            RENAME => {
                let name: String = params.text(NAME)?.chars().take(64).collect();
                Ok(ConsoleAct::Rename(index, name))
            }
            _ => Err(UiReject::UnknownAction),
        }
    }
}

fn console_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<ShieldConsoleWindow>>>) {
    let pending: Vec<(Entity, ConsoleAct)> = acts
        .read(world.resource::<Messages<UiActed<ShieldConsoleWindow>>>())
        .map(|a| (a.source, a.act.clone()))
        .collect();
    for (console, act) in pending {
        let Some(c) = world.get::<ShieldConsole>(console).cloned() else {
            continue;
        };
        let index = match &act {
            ConsoleAct::Toggle(i) | ConsoleAct::Rename(i, _) => *i,
        };
        let Some(g) = c
            .generators
            .get(index)
            .copied()
            .filter(|&g| gen_of(world, g).is_some())
        else {
            continue;
        };
        match act {
            ConsoleAct::Toggle(_) => toggle(world, g),
            ConsoleAct::Rename(_, name) => {
                if let Some(mut s) = world.get_mut::<ShieldGenerator>(g) {
                    s.display_name = name;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circle_and_ring_follow_circle_range_turfs() {
        // Radius 1: the centre and the four neighbours plus diagonals (1 + 1 = 2 <= 1.5? no).
        assert_eq!(circle(1.0).len(), 5);
        // Radius 3, ring outside radius 2.
        let ring: Vec<_> = circle(3.0)
            .into_iter()
            .filter(|o| !circle(2.0).contains(o))
            .collect();
        assert!(ring.contains(&(3, 0)) && ring.contains(&(2, 2)) && !ring.contains(&(1, 1)));
        assert!(perimeter(10.0).contains(&(10, 0)) && perimeter(10.0).contains(&(0, -10)));
    }

    #[test]
    fn regeneration_follows_state_and_radius() {
        let mut g = ShieldGenerator::new(false);
        g.max_regeneration = 10.0;
        assert_eq!(regeneration(&g), 2.5, "recovering");
        g.recovering = false;
        assert_eq!(regeneration(&g), 10.0);
        g.active = true;
        g.radius = 3.0;
        g.max_radius = 3.0;
        // 10 / (0.5 + 2) * 0.5 (interior penalty).
        assert_eq!(regeneration(&g), 2.0);
        g.exterior_only = true;
        assert_eq!(regeneration(&g), 4.0);
    }
}
