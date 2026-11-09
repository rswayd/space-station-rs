//! Conveyor belts and switches (`/obj/machinery/conveyor`, `/obj/machinery/conveyor_switch`,
//! `code/modules/recycling/conveyor.dm`).
//!
//! A belt moves what is on it (`/datum/component/convey`) one tile per `speed` seconds in
//! `movedir` while it has power and its `last_command` is not 0; a switch with the same `id`
//! sets that command for every belt and switch of the `id`. Wrench rotates, screwdriver inverts
//! (right click flips the belt), crowbar (4 s) takes it up; a belt stack extends a belt, or is
//! laid on a floor facing the user; a switch assembly links nearby stacks and is placed on a
//! floor.
//!
//! Cost: only running belts ([`Belts::active`]) are visited, every 0.05 s: a tile lookup each
//! and per thing on it a timer check. An idle round costs nothing per tick.
// shortcut: no sounds, no multitool wire mode (`wire_mode`: powers a belt from its cable), no
// screentips or examine text, no `/datum/wires/conveyor` on switches (one dud wire whose pulse
// flips the lever), no USB port or circuit component, switch speed by multitool cycles five
// presets instead of `tgui_input_number`, no sprite rotation of what rides a corner belt
// (`special_rotate`).

use std::collections::HashMap;

use bevy::prelude::*;
use ss13_mapload::engmisc::{
    CONVEYOR_ROOT, CONVEYOR_STACK, CONVEYOR_SWITCH_ITEM, CONVEYOR_SWITCH_ROOT,
};
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmAtom, DmConveyor, DmItem, Turf};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Dir, Facing, TilePos};
use ss13_world::SpatialIndex;

use super::RecyclingJob;
use crate::construction::{Job, path_of, spawn_obj, spawn_stack};
use crate::disposals::pipes::{self, EAST, NORTH, SOUTH, WEST};
use crate::health::Health;
use crate::power::Unpowered;
use crate::tools::{Actor, ToolKind, tell, use_stack, use_tool};

/// `CONVEYOR_FORWARD`, `CONVEYOR_BACKWARDS`, `CONVEYOR_OFF`.
const FORWARD: i8 = 1;
const BACKWARDS: i8 = -1;
const OFF: i8 = 0;
/// `crowbar_act`: `4 SECONDS`.
const PRY_DS: f32 = 40.0;
/// `view()`: how far a switch assembly sees belts and stacks.
const VIEW_RANGE: u32 = 7;
/// Conveyors move on `SSconveyors` (`wait = 1`: every tick, 0.05 s).
const BELT_TICK_SECS: f64 = 0.05;
/// Ticks one frame may catch up after a stall.
const MAX_CATCH_UP: u32 = 3;
/// What a multitool cycles a switch through (seconds per tile).
const SPEED_PRESETS: [f32; 5] = [0.2, 0.5, 1.0, 2.0, 5.0];

/// `/obj/machinery/conveyor`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Conveyor {
    pub id: String,
    /// `dir`, BYOND bits (a diagonal for corner belts).
    pub dir: u8,
    pub inverted: bool,
    pub flipped: bool,
    /// `operating`: what the belt does now (0 without power).
    pub operating: i8,
    /// `last_command`: what a switch last told it.
    pub last_command: i8,
    pub speed: f32,
}

impl Conveyor {
    /// `update_move_direction`: (`forwards`, `backwards`), corner belts included.
    pub fn directions(&self) -> (u8, u8) {
        let (mut forwards, mut backwards) = match self.dir {
            NORTH => (NORTH, SOUTH),
            SOUTH => (SOUTH, NORTH),
            EAST => (EAST, WEST),
            WEST => (WEST, EAST),
            d if d == NORTH | EAST => (EAST, SOUTH),
            d if d == NORTH | WEST => (NORTH, EAST),
            d if d == SOUTH | EAST => (SOUTH, WEST),
            d if d == SOUTH | WEST => (WEST, NORTH),
            _ => (SOUTH, NORTH),
        };
        if self.inverted {
            std::mem::swap(&mut forwards, &mut backwards);
        }
        if self.flipped {
            std::mem::swap(&mut forwards, &mut backwards);
        }
        (forwards, backwards)
    }

    /// `movedir`.
    pub fn movedir(&self) -> u8 {
        let (forwards, backwards) = self.directions();
        if self.operating == FORWARD {
            forwards
        } else {
            backwards
        }
    }

    /// `update_icon_state`.
    fn state(&self) -> String {
        let shown = if self.inverted {
            -self.operating
        } else {
            self.operating
        };
        let flip = if self.flipped { "-flipped" } else { "" };
        format!("conveyor{shown}{flip}")
    }
}

/// `/obj/machinery/conveyor_switch`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct ConveyorSwitch {
    pub id: String,
    pub position: i8,
    pub oneway: bool,
    pub invert_icon: bool,
    pub conveyor_speed: f32,
}

impl ConveyorSwitch {
    /// `update_position`.
    fn pulled(&mut self, direction: i8) {
        self.position = if self.position != OFF {
            OFF
        } else if self.oneway {
            FORWARD
        } else {
            direction
        };
    }

    /// `update_icon_state`.
    fn state(&self) -> &'static str {
        match (self.position, self.invert_icon) {
            (0, _) => "switch-off",
            (p, false) if p < 0 => "switch-rev",
            (p, true) if p < 0 => "switch-fwd",
            (_, false) => "switch-fwd",
            (_, true) => "switch-rev",
        }
    }
}

/// `/obj/item/stack/conveyor::id`: the switch a laid belt obeys.
#[derive(Component, Clone, Debug, PartialEq, Eq, Default)]
pub struct BeltStackId(pub String);

/// `/obj/item/conveyor_switch_construct::id` (`"[rand()]"`).
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct SwitchAssembly(pub String);

/// `GLOB.conveyors_by_id` and the belts that are switched on.
#[derive(Resource, Debug, Default)]
pub struct Belts {
    pub by_id: HashMap<String, Vec<Entity>>,
    pub active: Vec<Entity>,
}

/// `/datum/component/convey`: when the thing's next conveyor step is due.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Conveying {
    pub next_at: f64,
}

#[derive(Resource, Default)]
pub struct BeltClock(f64);

/// Belts and switches get their live state as they appear (map load or a later spawn).
pub fn init_new(mut commands: Commands, added: Query<(Entity, &DmConveyor), Added<DmConveyor>>) {
    for (e, dm) in &added {
        let dm = dm.clone();
        commands.queue(move |world: &mut World| add_state(world, e, &dm));
    }
}

fn add_state(world: &mut World, e: Entity, dm: &DmConveyor) {
    if world.get_entity(e).is_err() {
        return;
    }
    if dm.is_switch {
        let invert_icon = dm.invert_icon
            || (is_under(
                &path_of(world, e).unwrap_or_default(),
                "/obj/machinery/conveyor_switch/oneway",
            ) && matches!(
                world.get::<DmAtom>(e).map(|a| a.dir),
                Some(NORTH) | Some(WEST)
            ));
        world.entity_mut(e).insert(ConveyorSwitch {
            id: dm.id.clone(),
            position: OFF,
            oneway: dm.oneway,
            invert_icon,
            conveyor_speed: dm.conveyor_speed,
        });
        refresh_switch(world, e);
    } else {
        let dir = world
            .get::<DmAtom>(e)
            .map_or(SOUTH, |a| if a.dir == 0 { SOUTH } else { a.dir });
        let last_command = if dm.auto_on { FORWARD } else { OFF };
        world.entity_mut(e).insert(Conveyor {
            id: dm.id.clone(),
            dir,
            inverted: dm.inverted,
            flipped: dm.flipped,
            operating: last_command,
            last_command,
            speed: dm.speed,
        });
        refresh_belt(world, e);
        if dm.auto_on {
            world.resource_mut::<Belts>().active.push(e);
        }
    }
    world
        .resource_mut::<Belts>()
        .by_id
        .entry(dm.id.clone())
        .or_default()
        .push(e);
}

fn set_state(world: &mut World, e: Entity, state: &str) {
    let id = world.get::<DmAtom>(e).and_then(|a| {
        world
            .get_resource::<IconIndex>()
            .and_then(|i| i.get(&a.icon, state))
    });
    if let Some(id) = id
        && let Some(mut look) = world.get_mut::<Appearance>(e)
        && look.icon_state != id
    {
        look.icon_state = id;
    }
}

fn refresh_belt(world: &mut World, belt: Entity) {
    if let Some(state) = world.get::<Conveyor>(belt).map(Conveyor::state) {
        set_state(world, belt, &state);
    }
}

fn refresh_switch(world: &mut World, switch: Entity) {
    if let Some(state) = world
        .get::<ConveyorSwitch>(switch)
        .map(ConveyorSwitch::state)
    {
        set_state(world, switch, state);
    }
}

/// Whether the belt on `tile` moves `e`: a living mob or a loose object (`start_conveying`'s
/// `unconveyables`, `should_move`'s `anchored`), not one in a locker or in the air.
fn conveyable(world: &World, e: Entity, pos: TilePos) -> bool {
    let Ok(entity) = world.get_entity(e) else {
        return false;
    };
    if entity.contains::<Turf>()
        || entity.contains::<Conveyor>()
        || entity.contains::<crate::closets::InCloset>()
        || entity.contains::<crate::throw::Thrown>()
        || entity.contains::<crate::medical::surgery::Buckled>()
    {
        return false;
    }
    if entity.get::<DmItem>().is_some_and(|i| i.anchored) {
        return false;
    }
    if entity.contains::<Health>() {
        // `MOVETYPES_NOT_TOUCHING_GROUND` unless knocked out.
        if crate::multiz::flies(world, e)
            && entity.get::<crate::health::Stat>() == Some(&crate::health::Stat::Conscious)
        {
            return false;
        }
    } else {
        let Some(path) = path_of(world, e) else {
            return false;
        };
        if is_under(&path, "/obj/effect")
            || !(is_under(&path, "/obj/item") || !crate::obj_damage::anchored(world, e))
        {
            return false;
        }
    }
    !crate::spacemove::weightless_at(world, e, pos)
}

fn convey_step(world: &mut World, e: Entity, dir: Dir) {
    // `MOVEMENT_LOOP_NO_DIR_UPDATE`: the step leaves the thing facing as it was.
    let facing = world.get::<Facing>(e).copied();
    let _ = crate::pull::step_passing(world, e, dir, |_, _| false);
    if let Some(facing) = facing
        && let Some(mut current) = world.get_mut::<Facing>(e)
    {
        current.set_if_neq(facing);
    }
}

/// Belts' power, then a step of everything riding a running belt that is due.
pub fn tick(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs_f64();
    let steps = {
        let mut clock = world.resource_mut::<BeltClock>();
        clock.0 += dt;
        let n = ((clock.0 / BELT_TICK_SECS) as u32).min(MAX_CATCH_UP);
        clock.0 = (clock.0 - f64::from(n) * BELT_TICK_SECS).min(BELT_TICK_SECS);
        n
    };
    if steps == 0 {
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    let active: Vec<Entity> = {
        let mut belts = world.resource_mut::<Belts>();

        std::mem::take(&mut belts.active)
    };
    let mut keep = Vec::with_capacity(active.len());
    for belt in active {
        let Some(b) = world.get::<Conveyor>(belt).cloned() else {
            continue;
        };
        if b.last_command == OFF {
            continue;
        }
        keep.push(belt);
        let Some(&pos) = world.get::<TilePos>(belt) else {
            continue;
        };
        let wanted = if world.get::<Unpowered>(belt).is_some() {
            OFF
        } else {
            b.last_command
        };
        if b.operating != wanted {
            world.get_mut::<Conveyor>(belt).expect("read").operating = wanted;
            refresh_belt(world, belt);
        }
        if wanted == OFF {
            continue;
        }
        let (speed, dir) = (f64::from(b.speed), belt_dir(&b, wanted));
        let on_tile: Vec<Entity> = world.resource::<SpatialIndex>().on_tile(pos).to_vec();
        for e in on_tile {
            if !conveyable(world, e, pos) {
                continue;
            }
            let Some(next_at) = world.get::<Conveying>(e).map(|c| c.next_at) else {
                // `start_delay = speed`.
                world.entity_mut(e).insert(Conveying {
                    next_at: now + speed,
                });
                continue;
            };
            if now < next_at {
                continue;
            }
            world.get_mut::<Conveying>(e).expect("read").next_at = now + speed;
            if let Some(dir) = dir {
                convey_step(world, e, dir);
            }
        }
    }
    world.resource_mut::<Belts>().active = keep;
    let mut q = world.query_filtered::<(Entity, Option<&TilePos>), With<Conveying>>();
    let stale: Vec<Entity> = q
        .iter(world)
        .filter(|&(_, pos)| {
            pos.is_none_or(|&p| {
                !world
                    .resource::<SpatialIndex>()
                    .on_tile(p)
                    .iter()
                    .any(|&b| world.get::<Conveyor>(b).is_some_and(|c| c.operating != OFF))
            })
        })
        .map(|(e, _)| e)
        .collect();
    for e in stale {
        world.entity_mut(e).remove::<Conveying>();
    }
}

fn belt_dir(belt: &Conveyor, operating: i8) -> Option<Dir> {
    let mut b = belt.clone();
    b.operating = operating;
    Dir::from_bits(b.movedir())
}

/// tg `/obj/machinery/conveyor_switch/attack_hand(_secondary)`: `on_user_activation`.
pub fn switch_hand(world: &mut World, _mob: Entity, target: Entity, right: bool) -> bool {
    let Some(mut sw) = world.get::<ConveyorSwitch>(target).cloned() else {
        return false;
    };
    sw.pulled(if right { BACKWARDS } else { FORWARD });
    world.entity_mut(target).insert(sw.clone());
    refresh_switch(world, target);
    apply_switch(world, &sw);
    true
}

/// `update_linked_conveyors`, `update_linked_switches`.
fn apply_switch(world: &mut World, sw: &ConveyorSwitch) {
    let linked = world
        .resource::<Belts>()
        .by_id
        .get(&sw.id)
        .cloned()
        .unwrap_or_default();
    let mut live = Vec::with_capacity(linked.len());
    for e in linked {
        if let Some(mut belt) = world.get_mut::<Conveyor>(e) {
            belt.last_command = sw.position;
            belt.operating = sw.position;
            belt.speed = sw.conveyor_speed;
            live.push(e);
            refresh_belt(world, e);
            if sw.position != OFF {
                let mut belts = world.resource_mut::<Belts>();
                if !belts.active.contains(&e) {
                    belts.active.push(e);
                }
            }
        } else if let Some(mut other) = world.get_mut::<ConveyorSwitch>(e) {
            other.invert_icon = sw.invert_icon;
            other.position = sw.position;
            other.conveyor_speed = sw.conveyor_speed;
            live.push(e);
            refresh_switch(world, e);
        }
    }
    world
        .resource_mut::<Belts>()
        .by_id
        .insert(sw.id.clone(), live);
}

/// Tools on a belt or switch. True when the click was theirs.
pub(super) fn tool_act(
    world: &mut World,
    actor: Actor,
    kind: ToolKind,
    target: Entity,
    right: bool,
) -> bool {
    if world.get::<Conveyor>(target).is_some() {
        return belt_tool(world, actor, kind, target, right);
    }
    if world.get::<ConveyorSwitch>(target).is_some() {
        return switch_tool(world, actor, kind, target);
    }
    false
}

fn belt_tool(world: &mut World, actor: Actor, kind: ToolKind, target: Entity, right: bool) -> bool {
    let client = actor.client;
    match kind {
        ToolKind::Crowbar => {
            if use_tool(
                world,
                actor,
                target,
                PRY_DS,
                0,
                Job::Recycling(RecyclingJob::PryBelt),
            ) {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Вы с трудом поддеваете ленту...",
                );
            }
            true
        }
        ToolKind::Wrench => {
            let angle = if right { 45 } else { -45 };
            let dir = pipes::turn(world.get::<Conveyor>(target).expect("checked").dir, angle);
            if dir != 0 {
                world.get_mut::<Conveyor>(target).expect("checked").dir = dir;
                if let Some(dir) = Dir::from_bits(dir) {
                    set_facing(world, target, dir);
                }
            }
            tell(world, client, Span::Notice, "Вы поворачиваете ленту.");
            true
        }
        ToolKind::Screwdriver => {
            let mut belt = world.get_mut::<Conveyor>(target).expect("checked");
            let text = if right {
                belt.flipped = !belt.flipped;
                if belt.flipped {
                    "Вы переворачиваете полотно ленты."
                } else {
                    "Вы возвращаете полотно ленты."
                }
            } else {
                belt.inverted = !belt.inverted;
                if belt.inverted {
                    "Лента теперь идёт в обратную сторону."
                } else {
                    "Лента снова идёт вперёд."
                }
            };
            refresh_belt(world, target);
            tell(world, client, Span::Notice, text);
            true
        }
        // Wire mode is not ported; the tool is not put on the belt either.
        ToolKind::Multitool => true,
        _ => false,
    }
}

fn set_facing(world: &mut World, e: Entity, dir: Dir) {
    match world.get_mut::<Facing>(e) {
        Some(mut facing) => {
            facing.set_if_neq(Facing(dir));
        }
        None => {
            world.entity_mut(e).insert(Facing(dir));
        }
    }
}

fn switch_tool(world: &mut World, actor: Actor, kind: ToolKind, target: Entity) -> bool {
    let client = actor.client;
    match kind {
        ToolKind::Crowbar => {
            let (Some(sw), Some(&pos)) = (
                world.get::<ConveyorSwitch>(target).cloned(),
                world.get::<TilePos>(target),
            ) else {
                return true;
            };
            let linked = world.resource::<Belts>().by_id.get(&sw.id).cloned();
            world.despawn(target);
            if let Some(mut list) = linked {
                list.retain(|&e| e != target);
                world
                    .resource_mut::<Belts>()
                    .by_id
                    .insert(sw.id.clone(), list);
            }
            if let Some(item) = spawn_obj(world, CONVEYOR_SWITCH_ITEM, pos) {
                world.entity_mut(item).insert(SwitchAssembly(sw.id));
            }
            tell(world, client, Span::Notice, "Вы снимаете переключатель.");
            true
        }
        ToolKind::Screwdriver => {
            let mut sw = world.get_mut::<ConveyorSwitch>(target).expect("checked");
            sw.oneway = !sw.oneway;
            let text = if sw.oneway {
                "Переключатель теперь односторонний."
            } else {
                "Переключатель теперь обычный."
            };
            tell(world, client, Span::Notice, text);
            true
        }
        ToolKind::Wrench => {
            let mut sw = world.get_mut::<ConveyorSwitch>(target).expect("checked");
            sw.invert_icon = !sw.invert_icon;
            refresh_switch(world, target);
            tell(world, client, Span::Notice, "Вы переставляете рычаг.");
            true
        }
        ToolKind::Multitool => {
            let mut sw = world
                .get::<ConveyorSwitch>(target)
                .expect("checked")
                .clone();
            let next = SPEED_PRESETS
                .iter()
                .position(|&s| (s - sw.conveyor_speed).abs() < 0.01)
                .map_or(0, |i| (i + 1) % SPEED_PRESETS.len());
            sw.conveyor_speed = SPEED_PRESETS[next];
            world.entity_mut(target).insert(sw.clone());
            apply_switch(world, &sw);
            tell(
                world,
                client,
                Span::Notice,
                format!("Время между движениями: {} с.", sw.conveyor_speed),
            );
            true
        }
        _ => false,
    }
}

/// A belt stack or switch assembly on a floor, a belt stack on a belt, any item on a belt.
pub(super) fn item_interaction(
    world: &mut World,
    actor: Actor,
    held: Entity,
    target: Entity,
    pos: TilePos,
) -> bool {
    let path = path_of(world, held).unwrap_or_default();
    let stack = is_under(&path, CONVEYOR_STACK);
    let client = actor.client;
    if world.get::<Conveyor>(target).is_some() {
        if stack {
            extend_belt(world, actor, held, target);
            return true;
        }
        // `item_interaction`: out of combat mode the item is put on the belt.
        let combat = world
            .get::<crate::combat::CombatMode>(actor.mob)
            .is_some_and(|c| c.0);
        if combat || crate::inventory::no_drop(world, held) {
            return false;
        }
        crate::inventory::put_down(world, held, pos);
        return true;
    }
    let floor = world.get::<Turf>(target).is_some()
        && is_under(
            &path_of(world, target).unwrap_or_default(),
            "/turf/open/floor",
        );
    if !floor {
        return false;
    }
    if stack {
        let Some(&user) = world.get::<TilePos>(actor.mob) else {
            return true;
        };
        if user == pos {
            tell(
                world,
                client,
                Span::Warning,
                "Нельзя положить ленту под себя!",
            );
            return true;
        }
        let Some(dir) = crate::pull::dir_between(pos, user) else {
            return true;
        };
        let id = world
            .get::<BeltStackId>(held)
            .map(|b| b.0.clone())
            .unwrap_or_default();
        lay_belt(world, pos, dir as u8, &id);
        use_stack(world, held, 1);
        return true;
    }
    if is_under(&path, CONVEYOR_SWITCH_ITEM) {
        let id = assembly_id(world, held);
        let origin = world.get::<TilePos>(actor.mob).copied().unwrap_or(pos);
        let found = world
            .resource::<SpatialIndex>()
            .in_radius(origin, VIEW_RANGE)
            .any(|e| world.get::<Conveyor>(e).is_some_and(|b| b.id == id));
        if !found {
            tell(
                world,
                client,
                Span::Notice,
                "Переключатель не обнаружил связанных лент поблизости.",
            );
            return true;
        }
        if let Some(sw) = spawn_obj(world, CONVEYOR_SWITCH_ROOT, pos) {
            world.entity_mut(sw).insert(DmConveyor {
                id,
                is_switch: true,
                conveyor_speed: 0.2,
                ..default()
            });
        }
        world.despawn(held);
        return true;
    }
    false
}

/// `/obj/machinery/conveyor/item_interaction` with a belt stack: a new belt where this one
/// points.
fn extend_belt(world: &mut World, actor: Actor, held: Entity, target: Entity) {
    let Some(b) = world.get::<Conveyor>(target).cloned() else {
        return;
    };
    let Some(&pos) = world.get::<TilePos>(target) else {
        return;
    };
    let forwards = b.directions().0;
    let Some(dir) = Dir::from_bits(forwards) else {
        return;
    };
    let next = pos.step(dir);
    let index = world.resource::<SpatialIndex>();
    if !index.in_bounds(next)
        || crate::construction::turf_at(world, next)
            .and_then(|t| path_of(world, t))
            .is_some_and(|p| is_under(&p, "/turf/closed"))
    {
        return;
    }
    if index
        .on_tile(next)
        .iter()
        .any(|&e| world.get::<Conveyor>(e).is_some())
    {
        tell(
            world,
            actor.client,
            Span::Warning,
            "Нельзя положить ленту поверх другой ленты.",
        );
        return;
    }
    lay_belt(world, next, forwards, &b.id);
    use_stack(world, held, 1);
}

/// `new /obj/machinery/conveyor(loc, new_dir, new_id)`: the state comes in [`init_new`].
fn lay_belt(world: &mut World, pos: TilePos, dir: u8, id: &str) -> Option<Entity> {
    let belt = spawn_obj(world, CONVEYOR_ROOT, pos)?;
    if let Some(mut atom) = world.get_mut::<DmAtom>(belt) {
        atom.dir = dir;
    }
    if let Some(d) = Dir::from_bits(dir) {
        set_facing(world, belt, d);
    }
    if let Some(mut dm) = world.get_mut::<DmConveyor>(belt) {
        dm.id = id.to_owned();
    }
    Some(belt)
}

/// The assembly's `id`, made on first use (`Initialize`: `"[rand()]"`).
fn assembly_id(world: &mut World, assembly: Entity) -> String {
    if let Some(a) = world.get::<SwitchAssembly>(assembly) {
        return a.0.clone();
    }
    let id = format!(
        "{}",
        world
            .resource_mut::<crate::combat::CombatRng>()
            .range(0, u32::MAX - 1)
    );
    world
        .entity_mut(assembly)
        .insert(SwitchAssembly(id.clone()));
    id
}

/// `/obj/item/conveyor_switch_construct/attack_self`: every stack in view links to it.
pub fn attack_self(world: &mut World, client: Entity, item: Entity) -> bool {
    if !path_of(world, item).is_some_and(|p| is_under(&p, CONVEYOR_SWITCH_ITEM)) {
        return false;
    }
    let id = assembly_id(world, item);
    let holder_pos = world
        .get::<crate::inventory::CarriedBy>(item)
        .and_then(|c| world.get::<TilePos>(c.0).copied())
        .or_else(|| world.get::<TilePos>(item).copied());
    if let Some(pos) = holder_pos {
        let stacks: Vec<Entity> = world
            .resource::<SpatialIndex>()
            .in_radius(pos, VIEW_RANGE)
            .filter(|&e| path_of(world, e).is_some_and(|p| is_under(&p, CONVEYOR_STACK)))
            .collect();
        for stack in stacks {
            world.entity_mut(stack).insert(BeltStackId(id.clone()));
        }
    }
    tell(
        world,
        client,
        Span::Notice,
        "Вы привязали все сборки лент поблизости к этому переключателю.",
    );
    true
}

/// A switch assembly in hand on a belt stack in the other (`stack/item_interaction`).
pub(super) fn link_stack(world: &mut World, client: Entity, held: Entity, target: Entity) -> bool {
    if !path_of(world, held).is_some_and(|p| is_under(&p, CONVEYOR_SWITCH_ITEM))
        || !path_of(world, target).is_some_and(|p| is_under(&p, CONVEYOR_STACK))
    {
        return false;
    }
    let id = assembly_id(world, held);
    world.entity_mut(target).insert(BeltStackId(id));
    tell(
        world,
        client,
        Span::Notice,
        "Вы привязываете сборку ленты к переключателю.",
    );
    true
}

/// `crowbar_act` after its `do_after`.
pub(super) fn complete_pry(world: &mut World, client: Entity, belt: Entity) {
    let (Some(b), Some(&pos)) = (
        world.get::<Conveyor>(belt).cloned(),
        world.get::<TilePos>(belt),
    ) else {
        return;
    };
    world.despawn(belt);
    let linked = world.resource_mut::<Belts>().by_id.remove(&b.id);
    if let Some(mut list) = linked {
        list.retain(|&e| e != belt);
        world
            .resource_mut::<Belts>()
            .by_id
            .insert(b.id.clone(), list);
    }
    if let Some(stack) = spawn_stack(world, CONVEYOR_STACK, pos, 1)
        && !b.id.is_empty()
    {
        world.entity_mut(stack).insert(BeltStackId(b.id));
    }
    tell(world, client, Span::Notice, "Вы снимаете ленту.");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn belt(dir: u8) -> Conveyor {
        Conveyor {
            id: String::new(),
            dir,
            inverted: false,
            flipped: false,
            operating: FORWARD,
            last_command: FORWARD,
            speed: 0.2,
        }
    }

    #[test]
    fn corner_belts_and_inversion_follow_update_move_direction() {
        assert_eq!(belt(NORTH).directions(), (NORTH, SOUTH));
        assert_eq!(belt(NORTH | EAST).directions(), (EAST, SOUTH));
        assert_eq!(belt(SOUTH | WEST).directions(), (WEST, NORTH));
        let mut b = belt(EAST);
        b.inverted = true;
        assert_eq!(b.directions(), (WEST, EAST));
        b.flipped = true;
        assert_eq!(b.directions(), (EAST, WEST), "inverted and flipped cancel");
        b.inverted = false;
        b.operating = BACKWARDS;
        assert_eq!(b.movedir(), EAST, "backwards of a flipped belt");
    }

    #[test]
    fn a_switch_cycles_like_update_position() {
        let mut sw = ConveyorSwitch {
            id: String::new(),
            position: OFF,
            oneway: false,
            invert_icon: false,
            conveyor_speed: 0.2,
        };
        sw.pulled(BACKWARDS);
        assert_eq!(sw.position, BACKWARDS);
        assert_eq!(sw.state(), "switch-rev");
        sw.pulled(FORWARD);
        assert_eq!(sw.position, OFF);
        sw.oneway = true;
        sw.pulled(BACKWARDS);
        assert_eq!(sw.position, FORWARD, "one way ignores the direction");
    }

    #[test]
    fn a_running_belt_carries_an_item_after_its_delay() {
        use std::time::Duration;
        let mut app = App::new();
        app.add_plugins(ss13_world::WorldPlugin);
        let world = app.world_mut();
        world.insert_resource(SpatialIndex::auto_levels(20, 20));
        world.init_resource::<Time>();
        world.init_resource::<ss13_mapload::Areas>();
        world.init_resource::<Belts>();
        world.init_resource::<BeltClock>();
        let on_belt = TilePos::new(5, 5, 1);
        let running = world.spawn((on_belt, belt(NORTH))).id();
        world.resource_mut::<Belts>().active.push(running);
        let item = world
            .spawn((
                on_belt,
                DmItem {
                    path: "/obj/item/toy".into(),
                    ..default()
                },
            ))
            .id();
        let bolted = world
            .spawn((
                on_belt,
                DmItem {
                    path: "/obj/item/toy".into(),
                    anchored: true,
                    ..default()
                },
            ))
            .id();
        let advance = |world: &mut World, secs: f64| {
            world
                .resource_mut::<Time>()
                .advance_by(Duration::from_secs_f64(secs));
            tick(world);
        };
        advance(world, 0.1);
        assert_eq!(
            world.get::<TilePos>(item),
            Some(&on_belt),
            "the start delay"
        );
        advance(world, 0.25);
        assert_eq!(world.get::<TilePos>(item), Some(&TilePos::new(5, 6, 1)));
        assert_eq!(world.get::<TilePos>(bolted), Some(&on_belt));
        advance(world, 0.1);
        assert!(
            world.get::<Conveying>(item).is_none(),
            "off the belt the ride ends"
        );
    }

    #[test]
    fn icon_states_match_update_icon_state() {
        let mut b = belt(NORTH);
        assert_eq!(b.state(), "conveyor1");
        b.inverted = true;
        assert_eq!(b.state(), "conveyor-1");
        b.operating = OFF;
        assert_eq!(b.state(), "conveyor0");
        b.flipped = true;
        assert_eq!(b.state(), "conveyor0-flipped");
    }
}
