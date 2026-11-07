//! Field generators and containment fields (server): tg
//! `code/modules/power/singularity/field_generator.dm` and `containment_field.dm`.
//!
//! - A [`FieldGenerator`] is wrenched (2 s) and welded (2 s, a lit welder) to the floor, then
//!   turned on by hand. It warms up one step every 5 s (`+a1`..`+a3`), and at the third it
//!   looks up to 8 tiles along each cardinal for another active generator through free tiles
//!   (`setup_field`) and fills the gap with [`ContainmentField`]s; 0.5 s later it is online.
//!   Once charging it cannot be turned off by hand.
//! - An online generator pays `round((2 + fields) / 2)` power every machine tick (tg `process`
//!   on `SSmachines`), from its own store first, then from the generators it is linked to
//!   (`draw_power`); out of power it shuts down, drops every field of its loop and cools down
//!   one step every 5 s. Emitter beams charge it by their damage, up to 250
//!   (`bullet_act`); the `+p1`..`+p6` overlay shows `floor(6 * power / 250)`.
//! - A containment field is dense to whatever walks into it: a living mob is shocked
//!   (`yeet_shock`: `min(rand(30, 40), rand(30, 40))` through `electrocute_act` and 10 s
//!   paralysis for a carbon, burn for the rest), then the field sparks and throws it away
//!   200 tiles at speed 4 (`bump_field`), once per 0.5 s. A generator with fields does the same
//!   to what bumps it. Fields block air.
//!
//! Cost: only generators and fields exist in the queries; fields are made and dropped on
//! events (start, shutdown), the per-frame pass is over generators with a timer pending.
// shortcut: no floor shields (`shield_floor`), no singularity blocking (no singularity yet),
// no explosion blocking, no generator `can_atmos_pass` switch, fields are dense to thrown items
// and gun projectiles (tg lets them through; emitter beams pass, `super::emitter`), no
// megafauna or simple-animal rip-outs, fields are made in all four directions at once (tg
// staggers them 0.1 s apart).

use bevy::ecs::message::MessageCursor;
use bevy::ecs::query::QueryState;
use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmAtom, DmClothing, DmObj, Turf};
use ss13_shared::appearance::Overlay;
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::{Appearance, Dir, Facing, IconStateId, TilePos};
use ss13_world::{Bumped, Density, SpatialIndex};

use crate::click::ClickResolved;
use crate::construction::Job;
use crate::health::{DamageType, Health, Stat};
use crate::inventory::Inventory;
use crate::obj_damage::{Anchored, set_anchored};
use crate::tools::{Actor, ToolKind, tell, use_tool};

pub const FIELD_GENERATOR: &str = "/obj/machinery/field/generator";
pub const FIELD_GENERATOR_STARTS_ON: &str = "/obj/machinery/field/generator/starts_on";
pub const FIELD_GENERATOR_MAGIC: &str = "/obj/machinery/field/generator/starts_on/magic";
pub const CONTAINMENT_FIELD: &str = "/obj/machinery/field/containment";
/// `bump_field`'s `playsound(src, 'sound/effects/gravhit.ogg', 50, TRUE)`.
pub const GRAVHIT: &str = ss13_mapload::sound::files::DOORS[2];

/// tg `field_generator_max_power`.
pub const MAX_POWER: f64 = 250.0;
/// `var/power = 20`.
pub const START_POWER: f64 = 20.0;
/// The warm-up and cool-down step (`addtimer(..., 5 SECONDS)`).
pub const WARM_STEP_SECS: f64 = 5.0;
/// `addtimer(VARSET_CALLBACK(src, active, FG_ONLINE), 0.5 SECONDS)`.
const ONLINE_DELAY_SECS: f64 = 0.5;
/// `for(var/dist in 0 to 7)`: how far `setup_field` looks for the other generator.
pub const FIELD_REACH: u32 = 8;
/// `draw_power`'s `failsafe >= 8`.
const DRAW_FAILSAFE: u32 = 8;
/// `addtimer(CALLBACK(src, PROC_REF(clear_shock)), 0.5 SECONDS)`.
const SHOCK_COOLDOWN_SECS: f64 = 0.5;
/// `user.Paralyze(10 SECONDS)` of a carbon in `yeet_shock`.
const SHOCK_PARALYZE_SECS: f32 = 10.0;
/// `throw_at(target, 200, 4)`.
const THROW_RANGE: u32 = 200;
const THROW_SPEED: f32 = 4.0;
/// `default_unfasten_wrench` and `welder.use_tool(src, user, 20)`, deciseconds.
const TOOL_DS: f32 = 20.0;

/// tg `FG_OFFLINE`, `FG_CHARGING`, `FG_ONLINE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FgActive {
    Offline,
    Charging,
    Online,
}

/// tg `FG_UNSECURED`, `FG_SECURED`, `FG_WELDED`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FgState {
    Unsecured,
    Secured,
    Welded,
}

/// The generator's overlay states in its own icon.
#[derive(Clone, Debug, Default, PartialEq)]
struct GenSprites {
    /// `+a1`..`+a3`.
    warming: [Option<IconStateId>; 3],
    /// `+on`.
    on: Option<IconStateId>,
    /// `+p1`..`+p6`.
    power: [Option<IconStateId>; 6],
}

/// tg `/obj/machinery/field/generator`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct FieldGenerator {
    pub active: FgActive,
    pub power: f64,
    pub state: FgState,
    /// 0..=3.
    pub warming_up: u8,
    /// The `+p` overlay shown, `floor(6 * power / 250)`.
    pub power_level: u8,
    /// When the next warm-up (charging) or cool-down (offline) step comes.
    pub next_step_at: Option<f64>,
    /// When a generator that started its fields comes online.
    pub online_at: Option<f64>,
    pub fields: Vec<Entity>,
    pub connected: Vec<Entity>,
    /// `instantenous`: warms up and cools down at once (`starts_on/magic`).
    pub instant: bool,
    /// `has_shocked` until then.
    pub shocked_until: f64,
    sprites: GenSprites,
}

impl FieldGenerator {
    pub fn new(state: FgState) -> Self {
        Self {
            active: FgActive::Offline,
            power: START_POWER,
            state,
            warming_up: 0,
            power_level: 0,
            next_step_at: None,
            online_at: None,
            fields: Vec::new(),
            connected: Vec::new(),
            instant: false,
            shocked_until: f64::NEG_INFINITY,
            sprites: GenSprites::default(),
        }
    }

    /// `check_power_level`: DM's one-argument `round` floors.
    pub fn check_power_level(&mut self) {
        self.power_level = (6.0 * self.power / MAX_POWER).floor().clamp(0.0, 6.0) as u8;
    }

    /// `bullet_act` of a non-`BULLET` projectile: its damage charges the generator.
    pub fn absorb(&mut self, damage: f64) {
        self.power = (self.power + damage).min(MAX_POWER);
        self.check_power_level();
    }

    /// One warm-up or cool-down timer step at `now` (`warm_up`, `cool_down`). True when the
    /// warm-up finished and the fields start.
    pub fn step(&mut self, now: f64) -> bool {
        let Some(at) = self.next_step_at.filter(|&at| now >= at) else {
            return false;
        };
        match self.active {
            FgActive::Charging => {
                self.warming_up += 1;
                if self.warming_up >= 3 {
                    self.next_step_at = None;
                    return true;
                }
                self.next_step_at = Some(at + WARM_STEP_SECS);
            }
            FgActive::Offline => {
                self.warming_up = self.warming_up.saturating_sub(1);
                self.next_step_at = (self.warming_up > 0).then_some(at + WARM_STEP_SECS);
            }
            FgActive::Online => self.next_step_at = None,
        }
        false
    }

    /// `turn_on`. True when the fields start at once (`instantenous`).
    pub fn turn_on(&mut self, now: f64) -> bool {
        if self.instant {
            self.active = FgActive::Online;
            self.warming_up = 3;
            self.next_step_at = None;
            return true;
        }
        self.active = FgActive::Charging;
        self.next_step_at = Some(now + WARM_STEP_SECS);
        false
    }

    /// `turn_off` without the cleanup of the fields ([`turn_off`] does both).
    fn switch_off(&mut self, now: f64) {
        self.active = FgActive::Offline;
        self.online_at = None;
        if self.instant {
            self.warming_up = 0;
            self.next_step_at = None;
        } else {
            self.next_step_at = Some(now + WARM_STEP_SECS);
        }
    }
}

/// tg `/obj/machinery/field/containment` between two generators.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ContainmentField {
    pub gens: [Entity; 2],
    /// `has_shocked` until then.
    pub shocked_until: f64,
}

/// `round(power_draw * 0.5, 1)`: DM rounds halves up.
pub fn power_draw(fields: usize) -> f64 {
    ((2 + fields) as f64 * 0.5 + 0.5).floor()
}

/// tg `draw_power(draw, failsafe, other_generator, last)` over the generators' power stores:
/// own power first, then the first linked generator other than `last` is asked.
pub fn draw_power(
    stores: &mut [(Entity, f64, Vec<Entity>)],
    me: Entity,
    mut draw: f64,
    failsafe: u32,
    other: Option<Entity>,
    last: Option<Entity>,
) -> bool {
    if other == Some(me) || failsafe >= DRAW_FAILSAFE {
        return false;
    }
    let failsafe = failsafe + 1;
    let Some(i) = stores.iter().position(|s| s.0 == me) else {
        return false;
    };
    if stores[i].1 >= draw {
        stores[i].1 -= draw;
        return true;
    }
    draw -= stores[i].1;
    stores[i].1 = 0.0;
    let linked = stores[i].2.clone();
    for fg in linked {
        if Some(fg) == last {
            continue;
        }
        return match other {
            Some(other) => draw_power(stores, fg, draw, failsafe, Some(other), Some(me)),
            None => draw_power(stores, fg, draw, failsafe, Some(me), Some(me)),
        };
    }
    false
}

/// A timed generator tool action ([`Job::FieldGenerator`]), with the state it started from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldGenJob {
    /// `default_unfasten_wrench` from this anchored state.
    Wrench { anchored: bool },
    /// `welder_act` from `FG_SECURED` (weld) or `FG_WELDED` (cut free).
    Weld { welded: bool },
}

pub struct FieldGeneratorPlugin;

impl Plugin for FieldGeneratorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            ss13_world::MapInit,
            init_generators.after(super::init_power),
        )
        .add_systems(
            Update,
            (
                toggle_on_click
                    .after(ss13_world::WorldSystems::Movement)
                    .after(crate::click::ResolveClicks),
                run_generators.after(super::PowerSystems),
                field_bumps.after(ss13_world::WorldSystems::Movement),
            ),
        );
    }
}

use crate::tools::now;

fn init_generators(world: &mut World) {
    let mut query =
        world.query_filtered::<(Entity, &DmObj, Option<&DmAtom>), With<ss13_world::Mapped>>();
    let found: Vec<(Entity, String, Option<String>)> = query
        .iter(world)
        .filter(|(_, obj, _)| is_under(&obj.path, FIELD_GENERATOR))
        .map(|(e, obj, atom)| (e, obj.path.clone(), atom.map(|a| a.icon.clone())))
        .collect();
    let t = now(world);
    for (e, path, icon) in found {
        let sprites = icon.map(|i| sprites(world, &i)).unwrap_or_default();
        let starts_on = is_under(&path, FIELD_GENERATOR_STARTS_ON);
        // `/obj/machinery/field/generator/anchored` and `starts_on` are anchored.
        let anchored = world.get::<Anchored>(e).is_some_and(|a| a.0) || starts_on;
        let state = if starts_on {
            FgState::Welded
        } else if anchored {
            FgState::Secured
        } else {
            FgState::Unsecured
        };
        if anchored {
            set_anchored(world, e, true);
        }
        let mut fg = FieldGenerator::new(state);
        fg.sprites = sprites;
        if is_under(&path, FIELD_GENERATOR_MAGIC) {
            fg.instant = true;
            // `power_level = 6`: the highest overlay.
            fg.power_level = 6;
        }
        world.entity_mut(e).insert(fg);
        refresh_look(world, e);
        // `starts_on/post_machine_initialize`: `turn_on`.
        if starts_on {
            turn_on(world, e, t);
        }
    }
}

fn sprites(world: &World, icon: &str) -> GenSprites {
    let Some(index) = world.get_resource::<IconIndex>() else {
        return GenSprites::default();
    };
    GenSprites {
        warming: [1, 2, 3].map(|i| index.get(icon, &format!("+a{i}"))),
        on: index.get(icon, "+on"),
        power: [1, 2, 3, 4, 5, 6].map(|i| index.get(icon, &format!("+p{i}"))),
    }
}

/// `update_overlays`: `+a[warming_up]`, `+on` with fields, `+p[power_level]`.
fn refresh_look(world: &mut World, fg: Entity) {
    let Some(g) = world.get::<FieldGenerator>(fg) else {
        return;
    };
    let s = &g.sprites;
    let warming = g
        .warming_up
        .checked_sub(1)
        .and_then(|i| s.warming.get(usize::from(i)).copied().flatten());
    let on = s.on.filter(|_| !g.fields.is_empty());
    let power = g
        .power_level
        .checked_sub(1)
        .and_then(|i| s.power.get(usize::from(i)).copied().flatten());
    let overlays: Vec<Overlay> = [warming, on, power]
        .into_iter()
        .flatten()
        .map(Overlay::new)
        .collect();
    if let Some(mut appearance) = world.get_mut::<Appearance>(fg)
        && appearance.overlays != overlays
    {
        appearance.overlays = overlays;
    }
}

fn turn_on(world: &mut World, fg: Entity, t: f64) {
    let Some(start) = world
        .get_mut::<FieldGenerator>(fg)
        .map(|mut g| g.turn_on(t))
    else {
        return;
    };
    if start {
        start_fields(world, fg, t);
    }
    refresh_look(world, fg);
}

/// `turn_off`: offline, the loop's fields dropped (`cleanup`), cooling down.
pub fn turn_off(world: &mut World, fg: Entity) {
    let t = now(world);
    if let Some(mut g) = world.get_mut::<FieldGenerator>(fg) {
        g.switch_off(t);
    }
    cleanup(world, fg);
}

/// `start_fields`: a generator not welded down shuts off; otherwise fields toward every
/// active generator in reach, online 0.5 s later.
fn start_fields(world: &mut World, fg: Entity, t: f64) {
    let Some(g) = world.get::<FieldGenerator>(fg) else {
        return;
    };
    let anchored = world.get::<Anchored>(fg).is_some_and(|a| a.0);
    if g.state != FgState::Welded || !anchored {
        turn_off(world, fg);
        return;
    }
    for dir in [Dir::North, Dir::South, Dir::East, Dir::West] {
        setup_field(world, fg, dir);
    }
    if let Some(mut g) = world.get_mut::<FieldGenerator>(fg)
        && g.active == FgActive::Charging
    {
        g.online_at = Some(t + ONLINE_DELAY_SECS);
    }
    refresh_look(world, fg);
}

fn generator_on(world: &World, pos: TilePos) -> Option<Entity> {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| world.get::<FieldGenerator>(e).is_some())
}

fn field_on(world: &World, pos: TilePos) -> Option<Entity> {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| world.get::<ContainmentField>(e).is_some())
}

fn is_mob(world: &World, e: Entity) -> bool {
    world.get::<Health>(e).is_some() || world.get::<ss13_world::Mover>(e).is_some()
}

/// `setup_field(NSEW)`: the free run toward another active generator (walls and dense
/// non-mobs stop it), filled with fields; the two generators link up.
fn setup_field(world: &mut World, fg: Entity, dir: Dir) -> bool {
    let Some(&start) = world.get::<TilePos>(fg) else {
        return false;
    };
    let (dx, dy) = dir.offset();
    let mut cur = start;
    let mut found = None;
    let mut steps: i32 = 0;
    for _ in 0..FIELD_REACH {
        cur = TilePos::new(cur.x + dx, cur.y + dy, cur.z);
        let index = world.resource::<SpatialIndex>();
        if !index.in_bounds(cur) {
            return false;
        }
        let dense = index.dense_on_tile(cur);
        if dense.iter().any(|&e| world.get::<Turf>(e).is_some()) {
            return false;
        }
        if let Some(other) = generator_on(world, cur) {
            steps -= 1;
            if world
                .get::<FieldGenerator>(other)
                .is_some_and(|o| o.active == FgActive::Offline)
            {
                return false;
            }
            found = Some(other);
            break;
        }
        if dense.iter().any(|&e| e != fg && !is_mob(world, e)) {
            return false;
        }
        steps += 1;
    }
    let Some(other) = found else {
        return false;
    };
    let mut cur = start;
    for _ in 0..=steps {
        cur = TilePos::new(cur.x + dx, cur.y + dy, cur.z);
        if field_on(world, cur).is_some() {
            continue;
        }
        let field = spawn_field(world, cur, [fg, other], dir);
        for g in [fg, other] {
            if let Some(mut g) = world.get_mut::<FieldGenerator>(g) {
                g.fields.push(field);
            }
        }
        // `created_field.on_entered(src, shocked_mob)` for the living already there.
        let living: Vec<Entity> = world
            .resource::<SpatialIndex>()
            .on_tile(cur)
            .iter()
            .copied()
            .filter(|&e| world.get::<Health>(e).is_some())
            .collect();
        for mob in living {
            yeet_shock(world, field, mob);
        }
    }
    for (a, b) in [(fg, other), (other, fg)] {
        if let Some(mut g) = world.get_mut::<FieldGenerator>(a)
            && !g.connected.contains(&b)
        {
            g.connected.push(b);
        }
    }
    refresh_look(world, other);
    true
}

/// A new containment field on `pos` (dense here, so walking into it bumps it).
fn spawn_field(world: &mut World, pos: TilePos, gens: [Entity; 2], dir: Dir) -> Entity {
    let field = crate::construction::spawn_obj(world, CONTAINMENT_FIELD, pos)
        .unwrap_or_else(|| world.spawn((Name::new("containment field"), pos)).id());
    world.entity_mut(field).insert((
        ContainmentField {
            gens,
            shocked_until: f64::NEG_INFINITY,
        },
        Density,
        Facing(dir),
        ss13_atmos::AtmosBlocker(ss13_atmos::AtmosPass::No),
    ));
    field
}

/// `cleanup`: every field of the generator gone, and of every generator linked to it.
fn cleanup(world: &mut World, fg: Entity) {
    let mut todo = vec![fg];
    let mut done = Vec::new();
    while let Some(g) = todo.pop() {
        if done.contains(&g) {
            continue;
        }
        done.push(g);
        let Some((fields, linked)) = world.get_mut::<FieldGenerator>(g).map(|mut state| {
            (
                std::mem::take(&mut state.fields),
                std::mem::take(&mut state.connected),
            )
        }) else {
            continue;
        };
        for field in fields {
            drop_field(world, field);
        }
        for other in linked {
            if let Some(mut o) = world.get_mut::<FieldGenerator>(other) {
                o.connected.retain(|&c| c != g);
            }
            todo.push(other);
        }
        refresh_look(world, g);
    }
}

/// `containment/Destroy`: off both generators' lists, gone.
fn drop_field(world: &mut World, field: Entity) {
    let Some(f) = world.get::<ContainmentField>(field).copied() else {
        return;
    };
    for g in f.gens {
        if let Some(mut g) = world.get_mut::<FieldGenerator>(g) {
            g.fields.retain(|&x| x != field);
        }
    }
    if let Ok(e) = world.get_entity_mut(field) {
        e.despawn();
    }
}

/// `interact`: welded generators turn on by hand; once charging they cannot be turned off.
fn toggle_on_click(world: &mut World, mut clicks: Local<MessageCursor<ClickResolved>>) {
    let pending: Vec<ClickResolved> = clicks
        .read(world.resource::<Messages<ClickResolved>>())
        .copied()
        .collect();
    for click in pending {
        let Some((state, active)) = world
            .get::<FieldGenerator>(click.target)
            .map(|g| (g.state, g.active))
        else {
            continue;
        };
        let empty = world
            .get::<Inventory>(click.mob)
            .is_some_and(|i| i.active_item().is_none());
        if !empty || click.modifiers.right {
            continue;
        }
        let name = crate::security::name_of(world, click.target);
        if state != FgState::Welded {
            tell(
                world,
                click.client,
                Span::Warning,
                format!("{name} нужно сначала надёжно закрепить на полу!"),
            );
            continue;
        }
        if active != FgActive::Offline {
            tell(
                world,
                click.client,
                Span::Warning,
                format!("Вы не можете выключить {name}, пока он работает!"),
            );
            continue;
        }
        let user = crate::security::name_of(world, click.mob);
        crate::security::show(
            world,
            click.target_pos,
            7,
            ChatChannel::Local,
            (Span::Notice, format!("{user} включает {name}.")),
            &[(click.mob, Span::Notice, format!("Вы включаете {name}."))],
        );
        info!(target: "engine", "field generator {:?} activated by {:?}", click.target, click.mob);
        let t = now(world);
        turn_on(world, click.target, t);
    }
}

/// Timers (warm-up, cool-down, online, the shock cooldowns) every frame; `calc_power` once per
/// machine tick.
fn run_generators(
    world: &mut World,
    mut gens: Local<Option<QueryState<(Entity, &'static FieldGenerator)>>>,
    mut last_tick: Local<u64>,
) {
    let t = now(world);
    let gens = gens.get_or_insert_with(|| world.query());
    let due: Vec<Entity> = gens
        .iter(world)
        .filter(|(_, g)| {
            g.next_step_at.is_some_and(|at| t >= at) || g.online_at.is_some_and(|at| t >= at)
        })
        .map(|(e, _)| e)
        .collect();
    for fg in due {
        let Some(start) = world.get_mut::<FieldGenerator>(fg).map(|mut g| {
            if g.online_at.is_some_and(|at| t >= at) {
                g.online_at = None;
                // The `VARSET_CALLBACK` of a generator turned off since does nothing.
                if g.active == FgActive::Charging {
                    g.active = FgActive::Online;
                }
            }
            g.step(t)
        }) else {
            continue;
        };
        // `warm_up` of a generator turned off meanwhile returns early (`if(!active)`).
        if start {
            start_fields(world, fg, t);
        }
        refresh_look(world, fg);
    }
    let ticks = world.resource::<super::MachineTicks>().0;
    if ticks == *last_tick {
        return;
    }
    *last_tick = ticks;
    calc_power(world);
}

/// `process` -> `calc_power` of every online generator, in entity order.
fn calc_power(world: &mut World) {
    let mut gens = world.query::<(Entity, &FieldGenerator)>();
    let mut stores: Vec<(Entity, f64, Vec<Entity>)> = gens
        .iter(world)
        .map(|(e, g)| (e, g.power, g.connected.clone()))
        .collect();
    let online: Vec<(Entity, usize)> = gens
        .iter(world)
        .filter(|(_, g)| g.active == FgActive::Online && !g.instant)
        .map(|(e, g)| (e, g.fields.len()))
        .collect();
    let mut failed = Vec::new();
    for (fg, fields) in online {
        if !draw_power(&mut stores, fg, power_draw(fields), 0, None, None) {
            failed.push(fg);
        }
    }
    for (e, power, _) in stores {
        if let Some(mut g) = world.get_mut::<FieldGenerator>(e)
            && g.power != power
        {
            g.power = power;
            g.check_power_level();
        }
    }
    for fg in failed {
        // A generator its loop already shut down this tick still announces it, as in tg.
        if let Some(&pos) = world.get::<TilePos>(fg) {
            let name = crate::security::name_of(world, fg);
            crate::security::show(
                world,
                pos,
                7,
                ChatChannel::Local,
                (Span::Danger, format!("{name} отключается!")),
                &[],
            );
        }
        info!(target: "engine", "field generator {fg:?} ran out of power and DEACTIVATED");
        turn_off(world, fg);
        if let Some(mut g) = world.get_mut::<FieldGenerator>(fg) {
            g.power = 0.0;
            g.check_power_level();
        }
        refresh_look(world, fg);
    }
    let mut all = world.query_filtered::<Entity, With<FieldGenerator>>();
    let all: Vec<Entity> = all.iter(world).collect();
    for fg in all {
        refresh_look(world, fg);
    }
}

/// `bullet_act` of a beam that hit `target`: a field generator takes its damage as power.
/// True when `target` is one.
pub fn bullet_act(world: &mut World, target: Entity, damage: f32) -> bool {
    if world
        .get_mut::<FieldGenerator>(target)
        .map(|mut g| g.absorb(f64::from(damage)))
        .is_none()
    {
        return false;
    }
    refresh_look(world, target);
    true
}

/// `Bumped`: a field (or a generator with fields) shocks a living mob and repels it, or
/// repels a machine, structure or mech, at most once per 0.5 s.
fn field_bumps(world: &mut World, mut bumps: Local<MessageCursor<Bumped>>) {
    let bumped: Vec<(Entity, Entity)> = bumps
        .read(world.resource::<Messages<Bumped>>())
        .map(|b| (b.entity, b.obstacle))
        .collect();
    let t = now(world);
    for (mover, obstacle) in bumped {
        let shocked_until = if let Some(f) = world.get::<ContainmentField>(obstacle) {
            f.shocked_until
        } else if let Some(g) = world
            .get::<FieldGenerator>(obstacle)
            .filter(|g| !g.fields.is_empty())
        {
            g.shocked_until
        } else {
            continue;
        };
        if t < shocked_until {
            continue;
        }
        if world.get::<Health>(mover).is_some() {
            yeet_shock(world, obstacle, mover);
        } else if world.get::<crate::mecha::Mech>(mover).is_some()
            || world.get::<DmObj>(mover).is_some_and(|o| {
                is_under(&o.path, "/obj/machinery") || is_under(&o.path, "/obj/structure")
            })
        {
            bump_field(world, obstacle, mover);
        }
    }
}

/// The gloves' `siemens_coefficient` (`electrocute_act` without `SHOCK_NOGLOVES`).
fn glove_siemens(world: &World, mob: Entity) -> f32 {
    world
        .get::<Inventory>(mob)
        .and_then(|i| i.gloves)
        .and_then(|g| world.get::<DmClothing>(g))
        .map_or(1.0, |c| c.siemens_coefficient)
}

/// `yeet_shock`: a field whose generators are gone just disappears.
fn yeet_shock(world: &mut World, source: Entity, mob: Entity) {
    if let Some(f) = world.get::<ContainmentField>(source).copied()
        && f.gens
            .iter()
            .any(|&g| world.get::<FieldGenerator>(g).is_none())
    {
        drop_field(world, source);
        return;
    }
    let damage = {
        let mut rng = world.resource_mut::<super::EngineRng>();
        let (a, b) = (rng.0.range(30, 40), rng.0.range(30, 40));
        a.min(b) as f32
    };
    if world.get::<Inventory>(mob).is_some() {
        crate::status_effects::apply(
            world,
            mob,
            crate::status_effects::Incap::Paralyze,
            SHOCK_PARALYZE_SECS,
        );
        let siemens = glove_siemens(world, mob);
        crate::wires::electrocute_act(world, mob, damage, source, siemens);
    } else if let Ok(mut entity) = world.get_entity_mut(mob)
        && let Ok((mut health, mut stat)) = entity.get_components_mut::<(&mut Health, &mut Stat)>()
    {
        // `take_overall_damage(burn = shock_damage)` of the rest (silicons).
        health.adjust(DamageType::Burn, damage);
        crate::health::update_stat(&health, &mut stat);
    }
    bump_field(world, source, mob);
}

/// `bump_field`: sparks, the warning, `gravhit`, and the thing thrown away from the field.
fn bump_field(world: &mut World, source: Entity, thing: Entity) {
    let t = now(world);
    let field = world
        .get::<ContainmentField>(source)
        .map(|f| f.shocked_until);
    let fg = world.get::<FieldGenerator>(source).map(|g| g.shocked_until);
    if field.or(fg).is_some_and(|until| t < until) {
        return;
    }
    if let Some(mut f) = world.get_mut::<ContainmentField>(source) {
        f.shocked_until = t + SHOCK_COOLDOWN_SECS;
    }
    if let Some(mut g) = world.get_mut::<FieldGenerator>(source) {
        g.shocked_until = t + SHOCK_COOLDOWN_SECS;
    }
    let (Some(&at), Some(&from)) = (world.get::<TilePos>(thing), world.get::<TilePos>(source))
    else {
        return;
    };
    crate::emp::do_sparks(world, at);
    if world.get::<Health>(thing).is_some()
        && let Some(client) = world
            .get::<crate::server::PlayerMob>(thing)
            .map(|p| p.client)
    {
        tell(
            world,
            client,
            Span::Danger,
            "Поле отбрасывает вас с огромной силой!",
        );
    }
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(from, GRAVHIT, 50.0, true),
    );
    // `get_edge_target_turf(thing, get_dir(src, get_step_away(thing, src)))`: away from the
    // field, far enough that the range ends the flight first.
    let Some(away) = crate::mecha::core::dir_between(from, at) else {
        return;
    };
    let (dx, dy) = away.offset();
    let reach = THROW_RANGE as i32 + 1;
    let target = TilePos::new(at.x + dx * reach, at.y + dy * reach, at.z);
    let now_ds = t * 10.0;
    if let Some(path) = crate::throw::Trajectory::new(at, target, THROW_RANGE, THROW_SPEED, now_ds)
    {
        world.entity_mut(thing).insert(crate::throw::Thrown {
            path,
            thrower: None,
            target: None,
            last_dir: None,
        });
    }
}

/// `wrench_act` / `welder_act`. True when `target` is a field generator.
pub fn tool_act(world: &mut World, actor: Actor, kind: ToolKind, target: Entity) -> bool {
    let Some(g) = world.get::<FieldGenerator>(target).cloned() else {
        return false;
    };
    let anchored = world.get::<Anchored>(target).is_some_and(|a| a.0);
    let client = actor.client;
    let name = crate::security::name_of(world, target);
    match kind {
        ToolKind::Wrench => {
            // `can_be_unfasten_wrench`.
            if g.active != FgActive::Offline {
                tell(
                    world,
                    client,
                    Span::Warning,
                    format!("Сначала выключите {name}!"),
                );
            } else if g.state == FgState::Welded {
                tell(
                    world,
                    client,
                    Span::Warning,
                    format!("{name} приварен к полу!"),
                );
            } else {
                use_tool(
                    world,
                    actor,
                    target,
                    TOOL_DS,
                    0,
                    Job::FieldGenerator(FieldGenJob::Wrench { anchored }),
                );
            }
        }
        ToolKind::Welder => {
            if g.active != FgActive::Offline {
                tell(
                    world,
                    client,
                    Span::Warning,
                    format!("{name} должен быть выключен!"),
                );
                return true;
            }
            let (welded, own) = match g.state {
                FgState::Unsecured => {
                    tell(
                        world,
                        client,
                        Span::Warning,
                        format!("{name} нужно прикрутить к полу!"),
                    );
                    return true;
                }
                FgState::Secured => (false, format!("Вы начинаете приваривать {name} к полу...")),
                FgState::Welded => (true, format!("Вы начинаете отрезать {name} от пола...")),
            };
            if let Some(item) = actor.item
                && !crate::tools::tool_usable(world, item, 1, 0.0)
            {
                let text = crate::tools::unusable_message(world, item, 1);
                tell(world, client, Span::Warning, text);
                return true;
            }
            if use_tool(
                world,
                actor,
                target,
                TOOL_DS,
                0,
                Job::FieldGenerator(FieldGenJob::Weld { welded }),
            ) {
                tell(world, client, Span::Notice, own);
            }
        }
        _ => return false,
    }
    true
}

/// The end of a [`FieldGenJob`], re-checked against the state it started from.
pub fn complete(
    world: &mut World,
    client: Entity,
    target: Entity,
    job: FieldGenJob,
    consume: impl FnOnce(&mut World) -> bool,
) {
    let Some(g) = world.get::<FieldGenerator>(target).cloned() else {
        return;
    };
    if g.active != FgActive::Offline {
        return;
    }
    let anchored = world.get::<Anchored>(target).is_some_and(|a| a.0);
    let name = crate::security::name_of(world, target);
    match job {
        FieldGenJob::Wrench { anchored: from } => {
            if anchored != from || g.state == FgState::Welded || !consume(world) {
                return;
            }
            set_anchored(world, target, !from);
            // `set_anchored`: `state = anchorvalue ? FG_SECURED : FG_UNSECURED`.
            if let Some(mut g) = world.get_mut::<FieldGenerator>(target) {
                g.state = if from {
                    FgState::Unsecured
                } else {
                    FgState::Secured
                };
            }
            let text = if from {
                format!("Вы откручиваете {name} от пола.")
            } else {
                format!("Вы прикручиваете {name} к полу.")
            };
            tell(world, client, Span::Notice, text);
        }
        FieldGenJob::Weld { welded: from } => {
            let expected = if from {
                FgState::Welded
            } else {
                FgState::Secured
            };
            if g.state != expected || !consume(world) {
                return;
            }
            if let Some(mut g) = world.get_mut::<FieldGenerator>(target) {
                g.state = if from {
                    FgState::Secured
                } else {
                    FgState::Welded
                };
            }
            let text = if from {
                format!("Вы отрезаете {name} от пола.")
            } else {
                format!("Вы привариваете {name} к полу.")
            };
            tell(world, client, Span::Notice, text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warms_up_in_three_five_second_steps_then_cools_down() {
        let mut g = FieldGenerator::new(FgState::Welded);
        assert!(!g.turn_on(0.0));
        assert_eq!(g.active, FgActive::Charging);
        assert!(!g.step(4.9));
        assert!(!g.step(5.0));
        assert!(!g.step(10.0));
        assert_eq!(g.warming_up, 2);
        assert!(g.step(15.0), "the third step starts the fields");
        assert_eq!((g.warming_up, g.next_step_at), (3, None));
        g.active = FgActive::Online;
        g.switch_off(20.0);
        assert_eq!(g.active, FgActive::Offline);
        for t in [25.0, 30.0, 35.0] {
            assert!(!g.step(t));
        }
        assert_eq!((g.warming_up, g.next_step_at), (0, None));
        // Instant generators warm and cool at once.
        let mut magic = FieldGenerator::new(FgState::Welded);
        magic.instant = true;
        assert!(magic.turn_on(0.0));
        assert_eq!((magic.active, magic.warming_up), (FgActive::Online, 3));
    }

    #[test]
    fn beams_charge_up_to_the_cap_and_the_overlay_floors() {
        let mut g = FieldGenerator::new(FgState::Welded);
        g.check_power_level();
        // floor(6 * 20 / 250) = 0.
        assert_eq!(g.power_level, 0);
        g.absorb(30.0);
        assert_eq!((g.power, g.power_level), (50.0, 1));
        for _ in 0..10 {
            g.absorb(30.0);
        }
        assert_eq!((g.power, g.power_level), (MAX_POWER, 6));
    }

    #[test]
    fn the_draw_is_half_the_load_rounded_up_and_comes_from_the_loop() {
        assert_eq!(power_draw(0), 1.0);
        assert_eq!(power_draw(1), 2.0);
        assert_eq!(power_draw(6), 4.0);
        let (a, b) = (
            Entity::from_raw_u32(1).unwrap(),
            Entity::from_raw_u32(2).unwrap(),
        );
        let mut stores = vec![(a, 1.0, vec![b]), (b, 10.0, vec![a])];
        // `a` pays 1 itself and asks `b` for the other 2.
        assert!(draw_power(&mut stores, a, 3.0, 0, None, None));
        assert_eq!((stores[0].1, stores[1].1), (0.0, 8.0));
        // Neither has enough: `b` asks `a`, which does not ask back.
        let mut stores = vec![(a, 0.0, vec![b]), (b, 1.0, vec![a])];
        assert!(!draw_power(&mut stores, b, 5.0, 0, None, None));
        assert_eq!(stores[1].1, 0.0);
    }
}
