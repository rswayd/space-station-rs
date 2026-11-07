//! Atmospheric shield generators (server): tg `/obj/machinery/atmos_shield_gen` and
//! `/obj/effect/atmos_shield` (`code/game/machinery/atmos_field_gen.dm`).
//!
//! - An anchored generator toggles with an empty hand (refused while locked, but for
//!   silicons): off, or waiting for power. A right click or an ID with `external airlocks` or
//!   `engine_equip` locks it. `/active` maps in waiting and locked.
//! - Every machine tick (`process_early`): a powered waiting generator becomes active; an active
//!   one that lost power waits again and its fields go. An active generator without fields looks
//!   along its facing for `max_range` tiles (stopping at a closed turf) for a powered, anchored
//!   generator facing back with no master; found, that one is its slave and a field spans the
//!   line between them (both ends included), else it waits. Fields block air
//!   ([`ss13_atmos::AtmosPass::No`]) and nothing else; the master uses `active_power_usage` per
//!   field every tick. A slave whose master went inactive or away turns off.
//! - `RefreshParts`: `max_range = laser tier + 2`, `active_power_usage = 1 kW / capacitor tier`.
//! - Tools: a screwdriver refuses on a locked, shut generator; a crowbar and a wrench refuse
//!   while on, a wrench also while locked; otherwise `crate::machine_frame` takes it apart.
// shortcut: no wires (`/datum/wires/atmosshieldgen`), rotation, smoothing, light or emissive
// overlays, `TRAIT_FIREDOOR_STOP`, singularity or closed-turf destruction of fields.

use bevy::prelude::*;
use ss13_mapload::{DmItem, DmObj};
use ss13_shared::chat::Span;
use ss13_shared::{Dir, Facing, TilePos};
use ss13_world::SpatialIndex;

use crate::construction::{PanelOpen, path_of, tile_changed, turf_at};
use crate::obj_damage::{Anchored, is_under};
use crate::power::{MachineTicks, Unpowered};
use crate::stock_parts::PartKind;
use crate::tools::{Actor, ToolKind, tell, tool_of, use_tool};

pub const GENERATOR: &str = "/obj/machinery/atmos_shield_gen";
const ACTIVE_GENERATOR: &str = "/obj/machinery/atmos_shield_gen/active";
pub const FIELD: &str = "/obj/effect/atmos_shield";
/// `req_one_access`.
const ACCESS: [&str; 2] = ["external airlocks", "engine_equip"];
/// `BASE_MACHINE_ACTIVE_CONSUMPTION`, watts.
const BASE_ACTIVE_POWER: f64 = 1000.0;
/// `default_unfasten_wrench`, deciseconds.
const WRENCH_DS: f32 = 20.0;
const RATCHET: &str = "sound/items/tools/ratchet.ogg";
const NO_POWER: &str = "sound/machines/cryo_warning.ogg";

/// `GENERATOR_INACTIVE`, `GENERATOR_WANTPOWER`, `GENERATOR_ACTIVE`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GenState {
    #[default]
    Off,
    WantPower,
    Active,
}

/// A generator's live state.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct AtmosShieldGen {
    pub on: GenState,
    pub locked: bool,
    /// Its fields (the master's).
    pub fields: Vec<Entity>,
    /// The generator making the fields, on a slave.
    pub master: Option<Entity>,
}

/// A field tile and the generator that made it.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtmosShield {
    pub owner: Entity,
}

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // `attack_hand` (toggle) and `attack_hand_secondary` (lock).
    app.add_click_handler(ClickPhase::EmptyHand, 1979, |w, c| {
        interact(w, c.client, c.mob, c.target, c.modifiers.right)
    });
    // `item_interaction` (ID cards), the tool refusals and `wrench_act` (before
    // `crate::machine_frame` takes it apart).
    app.add_click_handler(ClickPhase::HeldOnTarget, 1589, |w, c| {
        c.held
            .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target))
    });
    app.add_systems(
        ss13_world::MapInit,
        setup_mapped.after(crate::power::init_power),
    )
    .add_systems(
        Update,
        machine_tick
            .after(crate::power::PowerSystems)
            .run_if(any_with_component::<AtmosShieldGen>),
    );
}

fn setup_mapped(world: &mut World) {
    let mut q = world.query_filtered::<(Entity, &DmObj), With<ss13_world::Mapped>>();
    let found: Vec<Entity> = q
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, GENERATOR))
        .map(|(e, _)| e)
        .collect();
    for e in found {
        setup(world, e);
    }
}

/// `Initialize`: `/active` waits for power, locked. Also for generators built from a frame
/// (`crate::machine_frame::setup_built`).
pub fn setup(world: &mut World, e: Entity) {
    let active = path_of(world, e).is_some_and(|p| is_under(&p, ACTIVE_GENERATOR));
    let state = if active {
        AtmosShieldGen {
            on: GenState::WantPower,
            locked: true,
            ..default()
        }
    } else {
        AtmosShieldGen::default()
    };
    world.entity_mut(e).insert(state);
}

fn state_of(world: &World, e: Entity) -> Option<&AtmosShieldGen> {
    world.get::<AtmosShieldGen>(e)
}

fn powered(world: &World, e: Entity) -> bool {
    world.get::<Unpowered>(e).is_none()
}

use crate::obj_damage::anchored;

fn facing(world: &World, e: Entity) -> Dir {
    world.get::<Facing>(e).map_or(Dir::South, |f| f.0)
}

/// `RefreshParts`: (`max_range`, `active_power_usage` watts).
pub fn ratings(world: &World, e: Entity) -> (u32, f64) {
    let r = crate::stock_parts::ratings(world, e);
    let laser = r.first(PartKind::MicroLaser).unwrap_or(1);
    let capacitor = r.first(PartKind::Capacitor).unwrap_or(1).max(1);
    (laser + 2, BASE_ACTIVE_POWER / f64::from(capacitor))
}

/// tg `allowed(user)` with `req_one_access`.
fn allowed(world: &World, mob: Entity, card: Option<Entity>) -> bool {
    if crate::silicon::interact::has_silicon_access(world, mob) {
        return true;
    }
    let card = card.or_else(|| {
        world
            .get::<crate::inventory::Inventory>(mob)
            .and_then(|i| i.id_card(world))
    });
    let access = card
        .and_then(|c| world.get::<crate::inventory::IdCard>(c))
        .map(|c| c.access.clone())
        .unwrap_or_default();
    let one: Vec<String> = ACCESS.iter().map(|a| (*a).to_owned()).collect();
    crate::inventory::check_access(&[], &one, &access)
}

fn toggle_lock(world: &mut World, client: Entity, e: Entity) {
    let locked = world.get_mut::<AtmosShieldGen>(e).map(|mut g| {
        g.locked = !g.locked;
        g.locked
    });
    let text = if locked == Some(true) {
        "locked!"
    } else {
        "unlocked!"
    };
    tell(world, client, Span::Notice, text);
}

/// An empty hand: toggle (left), lock (right). True when it was ours.
pub fn interact(world: &mut World, client: Entity, mob: Entity, e: Entity, right: bool) -> bool {
    let Some(g) = state_of(world, e).cloned() else {
        return false;
    };
    if right {
        if allowed(world, mob, None) {
            toggle_lock(world, client, e);
        } else {
            tell(world, client, Span::Warning, "no access!");
        }
        return true;
    }
    if !anchored(world, e) {
        tell(world, client, Span::Warning, "not anchored!");
        return true;
    }
    if g.locked && !crate::silicon::interact::has_silicon_access(world, mob) {
        tell(world, client, Span::Warning, "locked!");
        return true;
    }
    toggle(world, e);
    let on = state_of(world, e).is_some_and(|g| g.on != GenState::Off);
    tell(
        world,
        client,
        Span::Notice,
        if on { "turned on" } else { "turned off" },
    );
    true
}

/// `toggle`: off, or waiting for power.
pub fn toggle(world: &mut World, e: Entity) {
    if state_of(world, e).is_some_and(|g| g.on != GenState::Off) {
        turn_off(world, e, false);
    } else if let Some(mut g) = world.get_mut::<AtmosShieldGen>(e) {
        g.on = GenState::WantPower;
    }
}

/// The held item on a generator. True when it was ours.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    e: Entity,
) -> bool {
    let Some(g) = state_of(world, e).cloned() else {
        return false;
    };
    let is_card = world
        .get::<DmItem>(held)
        .is_some_and(|i| is_under(&i.path, "/obj/item/card/id"));
    if is_card {
        // `istype(tool, /obj/item/card/id) && check_access(tool)`: else on to the attack.
        if !allowed(world, mob, Some(held)) {
            return false;
        }
        toggle_lock(world, client, e);
        return true;
    }
    let Some((kind, _)) = tool_of(world, held) else {
        return false;
    };
    let on = g.on != GenState::Off;
    let panel = world.get::<PanelOpen>(e).is_some_and(|p| p.0);
    match kind {
        ToolKind::Screwdriver if !panel && g.locked => {
            tell(world, client, Span::Warning, "locked!");
            true
        }
        ToolKind::Crowbar | ToolKind::Wrench if on => {
            tell(world, client, Span::Warning, "turn off first!");
            true
        }
        ToolKind::Wrench if g.locked => {
            tell(world, client, Span::Warning, "unlock first!");
            true
        }
        ToolKind::Wrench => {
            let anchored = anchored(world, e);
            if let Some(&pos) = world.get::<TilePos>(e) {
                crate::sound::playsound(
                    world,
                    crate::sound::PlaySound::new(pos, RATCHET, 50.0, true),
                );
            }
            let actor = Actor {
                client,
                mob,
                item: Some(held),
            };
            use_tool(
                world,
                actor,
                e,
                WRENCH_DS,
                0,
                crate::construction::Job::Unfasten { anchored },
            );
            true
        }
        _ => false,
    }
}

/// `turn_off(power_failure)`.
pub fn turn_off(world: &mut World, e: Entity, power_failure: bool) {
    let Some(g) = state_of(world, e).cloned() else {
        return;
    };
    if g.on == GenState::Off {
        return;
    }
    if power_failure && let Some(&pos) = world.get::<TilePos>(e) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, NO_POWER, 65.0, false),
        );
    }
    if let Some(mut s) = world.get_mut::<AtmosShieldGen>(e) {
        s.on = if power_failure {
            GenState::WantPower
        } else {
            GenState::Off
        };
    }
    if let Some(master) = g.master {
        turn_off(world, master, power_failure);
    }
    // A slave ends inactive even on a power failure (`change_master(null)`).
    change_master(world, e, None);
    if !power_failure {
        drop_fields(world, e);
    }
}

/// `change_master`: a new master makes it active, none makes it inactive.
fn change_master(world: &mut World, e: Entity, master: Option<Entity>) {
    let Some(mut g) = world.get_mut::<AtmosShieldGen>(e) else {
        return;
    };
    if g.master == master {
        return;
    }
    g.master = master;
    g.on = if master.is_some() {
        GenState::Active
    } else {
        GenState::Off
    };
}

fn drop_fields(world: &mut World, e: Entity) {
    let fields = world
        .get_mut::<AtmosShieldGen>(e)
        .map(|mut g| std::mem::take(&mut g.fields))
        .unwrap_or_default();
    for f in fields {
        let Some(&pos) = world.get::<TilePos>(f) else {
            continue;
        };
        world.despawn(f);
        tile_changed(world, pos);
    }
}

/// `Destroy` of a generator taken apart or destroyed (from `crate::obj_damage::deconstruct`):
/// its fields go, its master turns off.
pub fn on_destroy(world: &mut World, e: Entity) {
    let Some(g) = state_of(world, e).cloned() else {
        return;
    };
    drop_fields(world, e);
    if let Some(master) = g.master {
        turn_off(world, master, false);
    }
    // The master of slaves pointing here (`master_deleted`).
    let mut q = world.query::<(Entity, &AtmosShieldGen)>();
    let slaves: Vec<Entity> = q
        .iter(world)
        .filter(|(_, s)| s.master == Some(e))
        .map(|(s, _)| s)
        .collect();
    for s in slaves {
        if let Some(mut g) = world.get_mut::<AtmosShieldGen>(s) {
            g.master = None;
        }
        turn_off(world, s, false);
    }
}

fn closed_turf(world: &World, pos: TilePos) -> bool {
    turf_at(world, pos)
        .and_then(|t| path_of(world, t))
        .is_some_and(|p| is_under(&p, "/turf/closed"))
}

fn spawn_field(world: &mut World, pos: TilePos, owner: Entity) -> Entity {
    let field = crate::construction::spawn_obj(world, FIELD, pos)
        .unwrap_or_else(|| world.spawn((Name::new("atmos shield"), pos)).id());
    world.entity_mut(field).insert((
        AtmosShield { owner },
        Anchored(true),
        ss13_atmos::AtmosBlocker(ss13_atmos::AtmosPass::No),
    ));
    tile_changed(world, pos);
    field
}

/// `process_early` of one generator.
pub fn process_early(world: &mut World, e: Entity) {
    // An unanchored generator is off (`wrench_act`: `if(... && !anchored) turn_off()`).
    if !anchored(world, e) && state_of(world, e).is_some_and(|g| g.on != GenState::Off) {
        turn_off(world, e, false);
    }
    let Some(g) = state_of(world, e).cloned() else {
        return;
    };
    let is_powered = powered(world, e);
    if g.on == GenState::Active && !is_powered {
        turn_off(world, e, true);
    } else if g.on == GenState::WantPower
        && is_powered
        && let Some(mut s) = world.get_mut::<AtmosShieldGen>(e)
    {
        s.on = GenState::Active;
    }
    let Some(g) = state_of(world, e).cloned() else {
        return;
    };
    if g.on != GenState::Active {
        drop_fields(world, e);
        return;
    }
    if let Some(master) = g.master {
        if state_of(world, master).is_none_or(|m| m.on != GenState::Active) {
            // Detached first: a slave dropped by its master must not turn that master from
            // waiting-for-power to off on the way out, or power coming back never re-forms the pair.
            change_master(world, e, None);
            turn_off(world, e, false);
        }
        return;
    }
    let (max_range, watts) = ratings(world, e);
    if !g.fields.is_empty() {
        let joules = watts * g.fields.len() as f64;
        crate::power::machine_use_energy(world, e, joules, false);
        return;
    }
    let (Some(&start), dir) = (world.get::<TilePos>(e), facing(world, e)) else {
        return;
    };
    let mut cur = start;
    let mut slave = None;
    let mut line = Vec::new();
    for _ in 0..max_range {
        if closed_turf(world, cur) {
            return;
        }
        line.push(cur);
        slave = world
            .resource::<SpatialIndex>()
            .on_tile(cur)
            .iter()
            .copied()
            .find(|&o| {
                o != e
                    && state_of(world, o).is_some_and(|s| s.master.is_none())
                    && powered(world, o)
                    && anchored(world, o)
                    && facing(world, o) == dir.reverse()
            });
        if slave.is_some() {
            break;
        }
        cur = cur.step(dir);
    }
    let Some(slave) = slave else {
        if let Some(mut s) = world.get_mut::<AtmosShieldGen>(e) {
            s.on = GenState::WantPower;
        }
        return;
    };
    change_master(world, slave, Some(e));
    let fields: Vec<Entity> = line
        .into_iter()
        .map(|pos| spawn_field(world, pos, e))
        .collect();
    if let Some(mut s) = world.get_mut::<AtmosShieldGen>(e) {
        s.fields = fields;
    }
}

fn machine_tick(world: &mut World, mut last_tick: Local<u64>) {
    let ticks = world.resource::<MachineTicks>().0;
    if ticks == *last_tick {
        return;
    }
    *last_tick = ticks;
    let mut q = world.query::<(Entity, &AtmosShieldGen)>();
    let gens: Vec<Entity> = q.iter(world).map(|(e, _)| e).collect();
    for e in gens {
        if world.get_entity(e).is_ok() {
            process_early(world, e);
        }
    }
}
