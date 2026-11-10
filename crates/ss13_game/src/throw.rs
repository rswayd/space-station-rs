//! Throwing (server), after tg `living_item_handling.dm` (`toggle_throw_mode`, `throw_item`),
//! `atoms_movable.dm` (`throw_at`, `throw_impact`, `hitby`), `subsystem/throwing.dm`
//! (`/datum/thrownthing`) and `living_defense.dm`/`carbon_defense.dm` (`hitby`, catching).
//!
//! - R (or the HUD throw button) toggles [`ThrowMode`]; the next click of the mob throws the
//!   active hand's item at what was clicked (any entity the client sees, no reach) and turns
//!   throw mode off, with `CLICK_CD_THROW`.
//! - The item flies like a `thrownthing`: `throw_speed` tiles per decisecond turned into steps per
//!   tick with tg's formula, along tg's Bresenham line ([`Trajectory`]), at most `throw_range`
//!   tiles, and lands on the clicked tile at the latest. Every step is a normal tile step (its
//!   `TilePos` replicates, so clients see it fly).
//! - It hits the first dense thing in its way (or the clicked movable when it reaches its tile,
//!   lying mobs included): a living mob in throw mode with an empty active hand catches it,
//!   otherwise takes `throwforce` as the item's damage type ([`crate::health::apply_damage`]) and,
//!   standing, is pushed one tile; an unanchored closet is pushed. The item stops where it was.
//! - With an empty active hand the mob held in an aggressive (or harder) grab is thrown instead
//!   (the grab ends; a neck grab throws one speed harder). A thrown carbon that crashes into
//!   something dense is hurt (tg `carbon/throw_impact`): into a wall paralyzed 2 s and 10 brute
//!   to a random bodypart (more when thrown faster than it throws itself), into a structure or a
//!   non-carbon only when fast; into another carbon both fall for 2 s with 10 brute each.
// shortcut: no thrower momentum, no spin animation, no armour on items, no space drift after
// landing, no `check_block` of the carbon crashed into, no brawling knockdown blocks, no
// hulk/dwarf throw power; add with armour, zero gravity and martial blocks.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::{DmCloset, DmItem, DmName};
use ss13_net::server::IntentReceived;
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::protocol::ClientIntent;
use ss13_shared::{Dir, Facing, TilePos};
use ss13_world::{SpatialIndex, WorldSystems};

use crate::click::{ResolveClicks, ThrowClick};
use crate::combat::{CombatRng, NextMove};
use crate::health::{BodyPosition, DamageType, Health, HealthSystems, Stat, apply_damage_zone};
use crate::inventory::{CarriedBy, Inventory};
use crate::pull::{dir_between, step_once, step_passing, visible_message};
use crate::server::{PlayerMob, Players};
use crate::zone::BodyZone;

/// `CLICK_CD_THROW`, seconds.
pub const CLICK_CD_THROW: f64 = 0.8;
/// BYOND `world.tick_lag` of tg (20 ticks per second), deciseconds; `SSthrowing` fires every tick.
const TICK_LAG_DS: f64 = 0.5;
/// `MAX_TICKS_TO_MAKE_UP`.
const MAX_TICKS_TO_MAKE_UP: f64 = 3.0;

/// tg `throw_mode` of a living mob (absent = off).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ThrowMode(pub bool);

/// tg `/datum/thrownthing` path math: where the next step goes and how many steps a tick takes.
#[derive(Clone, Debug, PartialEq)]
pub struct Trajectory {
    pub target_turf: TilePos,
    /// `get_dir` from the start to the target, kept after passing it.
    pub init_dir: Option<Dir>,
    pub maxrange: u32,
    /// Tiles per decisecond (`throw_speed`).
    pub speed: f32,
    pub dist_travelled: u32,
    /// `world.time` of the throw, deciseconds.
    pub start_ds: f64,
    dist_x: i32,
    dist_y: i32,
    dx: Dir,
    dy: Dir,
    diagonal_error: f32,
    pure_diagonal: bool,
}

impl Trajectory {
    /// tg `throw_at`; `None` for a speed of 0 or less (nothing is thrown).
    pub fn new(
        from: TilePos,
        target: TilePos,
        range: u32,
        speed: f32,
        now_ds: f64,
    ) -> Option<Self> {
        if speed <= 0.0 {
            return None;
        }
        let mut dist_x = (target.x - from.x).abs();
        let mut dist_y = (target.y - from.y).abs();
        let mut dx = if target.x > from.x {
            Dir::East
        } else {
            Dir::West
        };
        let mut dy = if target.y > from.y {
            Dir::North
        } else {
            Dir::South
        };
        let pure_diagonal = dist_x == dist_y;
        if !pure_diagonal && dist_x <= dist_y {
            std::mem::swap(&mut dist_x, &mut dist_y);
            std::mem::swap(&mut dx, &mut dy);
        }
        Some(Self {
            target_turf: target,
            init_dir: dir_between(from, target),
            maxrange: range,
            speed,
            dist_travelled: 0,
            start_ds: now_ds,
            dist_x,
            dist_y,
            dx,
            dy,
            diagonal_error: dist_x as f32 / 2.0 - dist_y as f32,
            pure_diagonal,
        })
    }

    /// tg `tilestomove` at `now_ds`.
    pub fn tiles_this_tick(&self, now_ds: f64) -> u32 {
        let travelled = if self.dist_travelled > 0 {
            f64::from(self.dist_travelled)
        } else {
            -1.0
        };
        let speed = f64::from(self.speed);
        let owed = ((now_ds + TICK_LAG_DS - self.start_ds) * speed - travelled)
            .min(speed * MAX_TICKS_TO_MAKE_UP);
        (owed * TICK_LAG_DS).ceil().max(0.0) as u32
    }

    /// The throw is over (`dist_travelled >= maxrange || loc == target_turf`).
    pub fn finished(&self, pos: TilePos) -> bool {
        self.dist_travelled >= self.maxrange || pos == self.target_turf
    }

    /// The direction of the next step from `pos` (`None`: nowhere to go); advances the
    /// Bresenham error like tg does on every step attempt.
    pub fn next_dir(&mut self, pos: TilePos) -> Option<Dir> {
        let homing = self.dist_travelled <= self.dist_x.max(self.dist_y) as u32;
        let mut dir = if homing {
            dir_between(pos, self.target_turf)
        } else {
            self.init_dir
        };
        if !self.pure_diagonal {
            let remaining = self.dist_x.max(self.dist_y) - self.dist_travelled as i32;
            if self.diagonal_error >= 0.0 && remaining != 1 {
                dir = Some(self.dx);
            }
            self.diagonal_error += if self.diagonal_error < 0.0 {
                self.dist_x as f32 / 2.0
            } else {
                -(self.dist_y as f32)
            };
        }
        dir
    }
}

/// On an item in flight.
#[derive(Component, Clone, Debug)]
pub struct Thrown {
    pub path: Trajectory,
    pub thrower: Option<Entity>,
    /// The movable that was clicked (tg `initial_target`), hit when the item reaches its tile.
    pub target: Option<Entity>,
    /// Direction of the last step (tg `dir` of the item, used by `hitby` pushes).
    pub last_dir: Option<Dir>,
}

pub struct ThrowServerPlugin;

impl Plugin for ThrowServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (toggle_throw_mode, throw_clicks, fly)
                .chain()
                .after(WorldSystems::Movement)
                .after(ResolveClicks)
                .before(HealthSystems),
        );
    }
}

/// tg `toggle_throw_mode`: not from crit or worse.
fn toggle_throw_mode(
    mut intents: MessageReader<IntentReceived>,
    players: Res<Players>,
    mut mobs: Query<(&PlayerMob, &Stat, Option<&mut ThrowMode>)>,
    mut commands: Commands,
) {
    for intent in intents.read() {
        if !matches!(intent.intent, ClientIntent::ToggleThrowMode) {
            continue;
        }
        let Some(mob) = players.mob(intent.client) else {
            continue;
        };
        let Ok((player, stat, mode)) = mobs.get_mut(mob) else {
            continue;
        };
        if player.client != intent.client || *stat >= Stat::SoftCrit {
            continue;
        }
        match mode {
            Some(mut mode) => mode.0 = !mode.0,
            None => {
                commands.entity(mob).insert(ThrowMode(true));
            }
        }
    }
}

fn name_of(world: &World, entity: Entity) -> String {
    if let Some(name) = world.get::<Name>(entity) {
        return name.as_str().to_owned();
    }
    world
        .get::<DmName>(entity)
        .map_or_else(|| "что-то".to_owned(), |n| n.name.clone())
}

/// BandaStation's throw verbs (`ru_verbs.toml`): third person and second person, tg picks one of
/// five (`yeet` with 0.5%).
const THROW_VERBS: [(&str, &str); 5] = [
    ("бросает", "бросаете"),
    ("метает", "метаете"),
    ("швыряет", "швыряете"),
    ("кидает", "кидаете"),
    ("запускает", "запускаете"),
];
const YEET: (&str, &str) = ("жёстко кидает", "жёстко кидаете");

/// tg `ClickOn` with `throw_mode` -> `throw_item`, checked at the moment of effect.
fn throw_clicks(world: &mut World, mut clicks: Local<MessageCursor<ThrowClick>>) {
    let clicks: Vec<ThrowClick> = clicks
        .read(world.resource::<Messages<ThrowClick>>())
        .copied()
        .collect();
    let now = world.resource::<Time>().elapsed_secs_f64();
    for click in clicks {
        let mob = click.mob;
        let Ok(entity) = world.get_entity(mob) else {
            continue;
        };
        if entity.get::<Stat>() != Some(&Stat::Conscious)
            || entity.get::<ThrowMode>() != Some(&ThrowMode(true))
            || entity.get::<NextMove>().is_none_or(|n| now < n.0)
            || world.get::<TilePos>(click.target) != Some(&click.target_pos)
        {
            continue;
        }
        let Some(&from) = entity.get::<TilePos>() else {
            continue;
        };
        let held = entity.get::<Inventory>().and_then(Inventory::active_item);
        if let (Some(mut facing), Some(dir)) = (
            world.get_mut::<Facing>(mob),
            dir_between(from, click.target_pos),
        ) {
            facing.set_if_neq(Facing(dir));
        }
        // `throw_item` turns throw mode off first, whatever happens next.
        world.entity_mut(mob).insert(ThrowMode(false));
        if held.is_none() {
            throw_grabbed(world, mob, from, click, now);
            continue;
        }
        let Some(item) = held.filter(|&i| !crate::inventory::no_drop(world, i)) else {
            continue;
        };
        let Some(dm) = world.get::<DmItem>(item).cloned() else {
            continue;
        };
        let speed = dm.throw_speed.max(1.0);
        let Some(path) = Trajectory::new(
            from,
            click.target_pos,
            dm.throw_range.max(0.0) as u32,
            speed,
            now * 10.0,
        ) else {
            continue;
        };
        let yeet = world.resource_mut::<CombatRng>().chance(0.5);
        let pick = world.resource_mut::<CombatRng>().range(0, 4) as usize;
        let (verb, own_verb) = if yeet { YEET } else { THROW_VERBS[pick] };
        let (user, thing) = (name_of(world, mob), name_of(world, item));
        let chat = visible_message(
            world,
            from,
            ChatChannel::Combat,
            (Span::Danger, format!("{user} {verb} {thing}!")),
            &[(mob, Span::Danger, format!("Вы {own_verb} {thing}!"))],
        );
        for message in chat {
            world.write_message(message);
        }
        world.entity_mut(mob).insert(NextMove(now + CLICK_CD_THROW));
        // `on_thrown` -> `dropItemToGround`, then `throw_at` from the mob's tile.
        let target =
            (world.get::<ss13_mapload::Turf>(click.target).is_none()).then_some(click.target);
        world.entity_mut(item).remove::<CarriedBy>().insert((
            from,
            Thrown {
                path,
                thrower: Some(mob),
                target,
                last_dir: None,
            },
        ));
        // tg `quickstart`: the first tick runs at once.
        crate::legcuffs::on_thrown(world, item, from);
        fly_one(world, item, now * 10.0);
    }
}

/// `throw_mode` default `throw_range`, `throw_speed` of a mob (`/atom/movable`).
pub const MOB_THROW_RANGE: u32 = 7;
pub const MOB_THROW_SPEED: f32 = 2.0;
/// `CARBON_MAX_IMPACT_SPEED_BONUS`.
pub const CARBON_MAX_IMPACT_SPEED_BONUS: f32 = 5.0;

/// On a mob in flight: how much faster than its own `throw_speed` it was thrown (tg
/// `extra_speed` of `carbon/throw_impact`, 0 when it threw itself).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct ThrownMob {
    pub extra_speed: f32,
}

/// tg `throw_item` with an empty hand: the mob pulled in an aggressive grab or harder, not
/// buckled, is let go and thrown (`neckgrab_throw` adds one to the speed).
fn throw_grabbed(world: &mut World, mob: Entity, from: TilePos, click: ThrowClick, now: f64) {
    let Some(target) = world.get::<crate::pull::Pulling>(mob).map(|p| p.0) else {
        return;
    };
    let grab = world
        .get::<crate::pull::GrabState>(mob)
        .copied()
        .unwrap_or_default();
    if grab < crate::pull::GrabState::Aggressive
        || world.get::<Health>(target).is_none()
        || world
            .get::<crate::medical::surgery::Buckled>(target)
            .is_some()
    {
        return;
    }
    world.entity_mut(mob).remove::<crate::pull::Pulling>();
    let Some(&start) = world.get::<TilePos>(target) else {
        return;
    };
    let neck = grab >= crate::pull::GrabState::Neck;
    let speed = MOB_THROW_SPEED + f32::from(u8::from(neck));
    let Some(path) = Trajectory::new(start, click.target_pos, MOB_THROW_RANGE, speed, now * 10.0)
    else {
        return;
    };
    let yeet = world.resource_mut::<CombatRng>().chance(0.5);
    let pick = world.resource_mut::<CombatRng>().range(0, 4) as usize;
    let (verb, own_verb) = if yeet { YEET } else { THROW_VERBS[pick] };
    let (user, thing) = (name_of(world, mob), name_of(world, target));
    let power = if neck { " с силой" } else { "" };
    let chat = visible_message(
        world,
        from,
        ChatChannel::Combat,
        (Span::Danger, format!("{user}{power} {verb} {thing}!")),
        &[(mob, Span::Danger, format!("Вы{power} {own_verb} {thing}!"))],
    );
    for message in chat {
        world.write_message(message);
    }
    world.entity_mut(mob).insert(NextMove(now + CLICK_CD_THROW));
    let aimed = (world.get::<ss13_mapload::Turf>(click.target).is_none() && click.target != target)
        .then_some(click.target);
    world.entity_mut(target).insert((
        Thrown {
            path,
            thrower: Some(mob),
            target: aimed,
            last_dir: None,
        },
        ThrownMob {
            extra_speed: (speed - MOB_THROW_SPEED).clamp(0.0, CARBON_MAX_IMPACT_SPEED_BONUS),
        },
    ));
    fly_one(world, target, now * 10.0);
}

/// tg `iscarbon`: a living mob with hands that is not a silicon or a basic mob.
pub(crate) fn is_carbon(world: &World, mob: Entity) -> bool {
    world.get::<Health>(mob).is_some()
        && world.get::<Inventory>(mob).is_some()
        && world.get::<crate::silicon::Silicon>(mob).is_none()
        && world.get::<crate::mobs::BasicMob>(mob).is_none()
}

/// tg `take_bodypart_damage(amount, check_armor = TRUE)`: brute to a random bodypart.
fn bodypart_damage(world: &mut World, mob: Entity, amount: f32) {
    let zone = match world.resource_mut::<CombatRng>().range(0, 5) {
        0 => BodyZone::Head,
        1 => BodyZone::Chest,
        2 => BodyZone::LArm,
        3 => BodyZone::RArm,
        4 => BodyZone::LLeg,
        _ => BodyZone::RLeg,
    };
    crate::bots::hurt_zone(world, mob, zone, DamageType::Brute, amount);
}

/// What a thrown carbon crashed into, for [`impact_damage`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Crash {
    /// A dense turf (a wall).
    Wall,
    /// An `/obj/structure`.
    Structure,
    /// Another carbon.
    Carbon,
    /// Anything else dense (machines, other mobs).
    Other,
}

/// tg `carbon/throw_impact` numbers: (seconds paralyzed, brute) of the thrown carbon, and the
/// same for a carbon it crashed into.
pub fn impact_damage(crash: Crash, extra_speed: f32) -> ((f32, f32), Option<(f32, f32)>) {
    let fast = extra_speed > 0.0;
    match crash {
        Crash::Wall => ((2.0, 10.0 + 5.0 * extra_speed), None),
        Crash::Structure if fast => ((1.0, 5.0 + 5.0 * extra_speed), None),
        Crash::Other if fast => ((0.0, 5.0 * extra_speed), None),
        Crash::Carbon => (
            (2.0, 10.0 + 5.0 * extra_speed),
            Some((2.0, 10.0 + 5.0 * extra_speed)),
        ),
        Crash::Structure | Crash::Other => ((0.0, 0.0), None),
    }
}

/// tg `carbon/throw_impact` of a thrown mob that hit `hit` (`None`: it landed).
fn mob_impact(world: &mut World, mob: Entity, hit: Option<Entity>) {
    let extra = world.get::<ThrownMob>(mob).map_or(0.0, |t| t.extra_speed);
    world.entity_mut(mob).remove::<ThrownMob>();
    let Some(hit) = hit else {
        return;
    };
    if !is_carbon(world, mob) || hit == mob {
        return;
    }
    let crash = if world.get::<ss13_mapload::Turf>(hit).is_some() {
        Crash::Wall
    } else if is_carbon(world, hit) {
        Crash::Carbon
    } else if crate::construction::path_of(world, hit)
        .is_some_and(|p| ss13_mapload::fidelity::is_under(&p, "/obj/structure"))
    {
        Crash::Structure
    } else {
        Crash::Other
    };
    // `if(hurt && hit_atom.density)`; the carbon branch does not ask.
    let dense = world.get::<ss13_world::Density>(hit).is_some()
        || world.get::<ss13_mapload::Turf>(hit).is_some();
    if crash != Crash::Carbon && !dense {
        return;
    }
    let ((stun, damage), victim) = impact_damage(crash, extra);
    if stun > 0.0 {
        crate::status_effects::apply(world, mob, crate::status_effects::Incap::Paralyze, stun);
    }
    if damage > 0.0 {
        bodypart_damage(world, mob, damage);
    }
    if let Some((victim_stun, victim_damage)) = victim {
        crate::status_effects::apply(
            world,
            hit,
            crate::status_effects::Incap::Paralyze,
            victim_stun,
        );
        bodypart_damage(world, hit, victim_damage);
    }
    let fast = if extra > 0.0 {
        " с большой скоростью"
    } else {
        ""
    };
    let (who, what) = (name_of(world, mob), name_of(world, hit));
    let at = world.get::<TilePos>(mob).copied().unwrap_or_default();
    let (others, own) = if crash == Crash::Carbon {
        (
            format!("{who} врезается в {what}{fast}, сбивая с ног их обоих!"),
            format!("Вы врезаетесь в {what}{fast}!"),
        )
    } else {
        (
            format!("{who} врезается в {what}{fast}!"),
            format!("Вы врезаетесь в {what}{fast}!"),
        )
    };
    let chat = visible_message(
        world,
        at,
        ChatChannel::Combat,
        (Span::Danger, others),
        &[(mob, Span::BoldWarning, own)],
    );
    for message in chat {
        world.write_message(message);
    }
}

/// tg `throw_at(target, range, speed)` of an item on a tile with no thrower (machines throwing
/// out what they made). False when it does not fly.
pub fn throw_at(world: &mut World, item: Entity, target: TilePos, range: u32, speed: f32) -> bool {
    let Some(&from) = world.get::<TilePos>(item) else {
        return false;
    };
    let now_ds = world.resource::<Time>().elapsed_secs_f64() * 10.0;
    let Some(path) = Trajectory::new(from, target, range, speed, now_ds) else {
        return false;
    };
    world.entity_mut(item).insert(Thrown {
        path,
        thrower: None,
        target: None,
        last_dir: None,
    });
    crate::legcuffs::on_thrown(world, item, from);
    fly_one(world, item, now_ds);
    true
}

/// `SSthrowing.fire`: every item in flight takes this tick's steps.
// Cost: per tick, one pass over items in flight only.
fn fly(world: &mut World, mut flying: Local<Vec<Entity>>) {
    let mut query = world.query_filtered::<Entity, With<Thrown>>();
    flying.extend(query.iter(world));
    let now_ds = world.resource::<Time>().elapsed_secs_f64() * 10.0;
    for item in flying.drain(..) {
        fly_one(world, item, now_ds);
    }
}

/// tg `thrownthing.tick` for one item.
fn fly_one(world: &mut World, item: Entity, now_ds: f64) {
    let Some(mut thrown) = world.get::<Thrown>(item).cloned() else {
        return;
    };
    // Picked up or deleted mid-air: the throw ends.
    let Some(mut pos) = world.get::<TilePos>(item).copied() else {
        world.entity_mut(item).remove::<Thrown>();
        return;
    };
    if thrown.path.dist_travelled > 0
        && let Some(obstacle) = obstacle_here(world, &thrown, item, pos)
    {
        finish(world, item, &thrown, Some(obstacle));
        return;
    }
    for _ in 0..thrown.path.tiles_this_tick(now_ds) {
        if thrown.path.finished(pos) {
            finish(world, item, &thrown, None);
            return;
        }
        let Some(dir) = thrown.path.next_dir(pos) else {
            finish(world, item, &thrown, None);
            return;
        };
        if let Err(blockers) = move_item(world, item, dir, thrown.thrower) {
            let hit = pick_hit(world, &blockers, thrown.target);
            finish(world, item, &thrown, hit);
            return;
        }
        thrown.last_dir = Some(dir);
        thrown.path.dist_travelled += 1;
        pos = world.get::<TilePos>(item).copied().unwrap_or(pos);
        if let Some(target) = thrown.target
            && world.get::<TilePos>(target) == Some(&pos)
        {
            finish(world, item, &thrown, Some(target));
            return;
        }
    }
    world.entity_mut(item).insert(thrown);
}

/// tg's check for things that moved onto the item's tile while it waited: the clicked movable,
/// or anything dense that is not a border object (not the thrower).
fn obstacle_here(world: &World, thrown: &Thrown, item: Entity, pos: TilePos) -> Option<Entity> {
    let index = world.resource::<SpatialIndex>();
    if let Some(target) = thrown.target
        && index.on_tile(pos).contains(&target)
    {
        return Some(target);
    }
    index
        .dense_on_tile(pos)
        .iter()
        .copied()
        .find(|&e| e != item && Some(e) != thrown.thrower && !over_it(world, e))
}

/// tg `thrownthing.tick`: a thrown thing flies over `LETPASSTHROW` (tables, racks, trays).
fn over_it(world: &World, e: Entity) -> bool {
    crate::inventory::lets_pass(world, e, crate::inventory::LETPASSTHROW)
}

/// tg `Move` of the item: a diagonal step tries its vertical half, then the horizontal one, and
/// the first refusal ends the flight (tg `Bump` -> `finalize`).
fn move_item(
    world: &mut World,
    item: Entity,
    dir: Dir,
    thrower: Option<Entity>,
) -> Result<(), Vec<Entity>> {
    let step = |world: &mut World, d| {
        step_passing(world, item, d, |w, e| Some(e) == thrower || over_it(w, e))
    };
    let (dx, dy) = dir.offset();
    if dx == 0 || dy == 0 {
        return step(world, dir);
    }
    let v = if dy > 0 { Dir::North } else { Dir::South };
    let h = if dx > 0 { Dir::East } else { Dir::West };
    let first = match step(world, v) {
        Ok(()) => h,
        Err(_) => {
            step(world, h)?;
            v
        }
    };
    step(world, first)
}

/// Of the things that refused the item: the clicked one, else a living mob, else the first.
fn pick_hit(world: &World, blockers: &[Entity], target: Option<Entity>) -> Option<Entity> {
    blockers
        .iter()
        .copied()
        .find(|&e| Some(e) == target)
        .or_else(|| {
            blockers
                .iter()
                .copied()
                .find(|&e| world.get::<Health>(e).is_some())
        })
        .or_else(|| blockers.first().copied())
}

/// tg `finalize` -> `throw_impact` -> `hitby` of what was hit (`None`: it lands).
fn finish(world: &mut World, item: Entity, thrown: &Thrown, hit: Option<Entity>) {
    world.entity_mut(item).remove::<Thrown>();
    if world.get::<Health>(item).is_some() {
        mob_impact(world, item, hit);
        return;
    }
    if crate::spells::conjure::packet_thrown(world, item, hit) {
        return;
    }
    let Some(hit) = hit else {
        // Toys, plates and pizza boxes (`crate::toys::thrown`).
        if crate::toys::thrown(world, item, None) {
            return;
        }
        // `/obj/item/light/throw_impact`: a bulb shatters where it lands.
        crate::power::lighting::thrown(world, item);
        // `frozen/shatter_on_landed`.
        if crate::effects::frozen::shatter(world, item) {
            return;
        }
        // Plants with liquid contents squash where they land.
        if crate::botany::produce::thrown(world, item, None) {
            return;
        }
        crate::chem::soda_thrown(world, item);
        if world.get_entity(item).is_ok() {
            crate::bar::glass_thrown(world, item, None);
        }
        return;
    };
    if world.get::<Health>(hit).is_some() {
        hit_living(world, item, thrown, hit);
        crate::holodeck::dodgeball_hit(world, item, hit);
        // `bola/throw_impact`: not caught.
        if world.get_entity(item).is_ok() && world.get::<CarriedBy>(item).is_none() {
            crate::legcuffs::thrown(world, item, hit);
        }
        if world.get_entity(item).is_err() {
            return;
        }
        if world.get::<CarriedBy>(item).is_none() && crate::toys::thrown(world, item, Some(hit)) {
            return;
        }
        // `frozen/shatter_on_throw`: not caught.
        if world.get_entity(item).is_ok() && crate::effects::frozen::shatter(world, item) {
            return;
        }
        // `COMSIG_MOVABLE_IMPACT` of a plant: prickles, squashing when not caught.
        if world.get_entity(item).is_ok() && crate::botany::produce::thrown(world, item, Some(hit))
        {
            return;
        }
        // Not caught.
        if world.get::<CarriedBy>(item).is_none() && world.get_entity(item).is_ok() {
            crate::chem::soda_thrown(world, item);
            if world.get_entity(item).is_ok() {
                crate::bar::glass_thrown(world, item, Some(hit));
            }
        }
    } else if crate::effects::frozen::shatter(world, item) {
        // `COMSIG_MOVABLE_IMPACT` comes before the hit thing's `hitby`.
        return;
    } else if crate::disposals::hitby(world, item, hit) {
    } else if world.get::<DmCloset>(hit).is_some_and(|c| !c.wall_mounted) {
        push(world, hit, thrown);
    }
    if world.get::<Health>(hit).is_none()
        && world.get_entity(item).is_ok()
        && world.get::<TilePos>(item).is_some()
        && crate::toys::thrown(world, item, Some(hit))
    {
        return;
    }
    if world.get::<Health>(hit).is_none()
        && world.get_entity(item).is_ok()
        && crate::botany::produce::thrown(world, item, Some(hit))
    {
        return;
    }
    if world.get::<Health>(hit).is_none() && world.get_entity(item).is_ok() {
        crate::chem::soda_thrown(world, item);
        if world.get_entity(item).is_ok() {
            crate::bar::glass_thrown(world, item, Some(hit));
        }
    }
    // `/obj/item/light/throw_impact`: a bulb not caught shatters on what it hit.
    crate::power::lighting::thrown(world, item);
}

/// `/atom/movable/hitby`: an unanchored thing hit hard enough steps along the item's direction.
fn push(world: &mut World, hit: Entity, thrown: &Thrown) {
    if let Some(dir) = thrown.last_dir.or(thrown.path.init_dir) {
        let _ = step_once(world, hit, dir, None);
    }
}

/// `/mob/living/carbon/hitby` (catch) and `/mob/living/hitby` (damage, push).
fn hit_living(world: &mut World, item: Entity, thrown: &Thrown, mob: Entity) {
    let Some(dm) = world.get::<DmItem>(item).cloned() else {
        return;
    };
    let Some(at) = world.get::<TilePos>(mob).copied() else {
        return;
    };
    let (victim, thing) = (name_of(world, mob), name_of(world, item));
    // tg `can_catch_item`: throw mode, hands free to act, the active hand empty.
    let catcher = world
        .get::<Inventory>(mob)
        .filter(|inv| inv.active_item().is_none());
    let catches = catcher.is_some()
        && world.get::<ThrowMode>(mob) == Some(&ThrowMode(true))
        && world.get::<Stat>(mob) == Some(&Stat::Conscious)
        && dm.pickable()
        && !crate::legcuffs::uncatchable(world, item);
    if catches {
        let hand = catcher.map_or(0, |inv| inv.active);
        crate::inventory::pick_up(world, mob, item, hand);
        let chat = visible_message(
            world,
            at,
            ChatChannel::Combat,
            (Span::Warning, format!("{victim} ловит {thing}!")),
            &[(
                mob,
                Span::BoldWarning,
                format!("Вы ловите {thing} в воздухе!"),
            )],
        );
        for message in chat {
            world.write_message(message);
        }
        return;
    }
    // `hitby` -> `check_block(THROWN_PROJECTILE_ATTACK)`, not of one's own throw: the worn
    // reactive armor, then the held items. Blocked: no catch, no damage, no push.
    if thrown.thrower != Some(mob) {
        let kind = DamageType::from_dm(&dm.damtype).unwrap_or(DamageType::Brute);
        if crate::clothing::reactive::hit_reaction(world, mob, Some(item), &thing) {
            return;
        }
        let attack = crate::weapons::block::AttackType::Thrown;
        if let Some(blocker) = crate::weapons::block::held_block(world, mob, dm.throwforce, attack)
        {
            crate::weapons::block::on_block(world, mob, blocker, &thing, dm.throwforce, kind);
            return;
        }
    }
    let chat = visible_message(
        world,
        at,
        ChatChannel::Combat,
        (Span::Danger, format!("{thing} врезается в {victim}!")),
        &[(mob, Span::BoldWarning, format!("В вас врезается {thing}!"))],
    );
    for message in chat {
        world.write_message(message);
    }
    if dm.throwforce <= 0.0 {
        return;
    }
    let kind = DamageType::from_dm(&dm.damtype).unwrap_or(DamageType::Brute);
    if let (Some(mut health), Some(&stat)) =
        (world.get::<Health>(mob).cloned(), world.get::<Stat>(mob))
    {
        let mut new_stat = stat;
        // tg `hitby`: `get_random_valid_zone(BODY_ZONE_CHEST, 65)`.
        let zone = crate::medical::random_valid_zone(
            &mut world.resource_mut::<CombatRng>(),
            BodyZone::Chest,
            65.0,
        );
        let before = health.part(zone).damage();
        let multiplier = world
            .get::<crate::medical::wounds::Limbs>(mob)
            .map_or(1.0, |l| l.limb(zone).damage_multiplier());
        let damage = dm.throwforce * multiplier;
        apply_damage_zone(&mut health, &mut new_stat, zone, kind, damage);
        world.entity_mut(mob).insert(health);
        if new_stat != stat {
            world.entity_mut(mob).insert(new_stat);
        }
        // tg `hitby` -> `apply_damage(wound_bonus, exposed_wound_bonus, sharpness)`.
        let wounding = world
            .get::<ss13_mapload::DmWounding>(item)
            .cloned()
            .unwrap_or_default();
        crate::medical::wounds::after_hit(
            world,
            mob,
            zone,
            crate::medical::wounds::Hit {
                kind,
                damage,
                sharpness: wounding.sharpness,
                wound_bonus: wounding.wound_bonus,
                exposed_wound_bonus: wounding.exposed_wound_bonus,
                part_damage_before: before,
            },
        );
    }
    // "it's significantly harder to push someone ... if they are down on the floor".
    let lying = world.get::<BodyPosition>(mob).is_some_and(|b| b.lying());
    if !lying {
        push(world, mob, thrown);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn at(x: i32, y: i32) -> TilePos {
        TilePos::new(x, y, 1)
    }

    /// Walks a trajectory on an empty map, one step per call, until it is finished.
    fn walk(mut path: Trajectory, mut pos: TilePos) -> Vec<TilePos> {
        let mut tiles = vec![];
        while !path.finished(pos) {
            let dir = path.next_dir(pos).expect("a direction");
            pos = pos.step(dir);
            path.dist_travelled += 1;
            tiles.push(pos);
            assert!(tiles.len() < 50);
        }
        tiles
    }

    #[test]
    fn lands_on_the_target_or_at_range() {
        let from = at(5, 5);
        // Straight east, 3 tiles: lands on the clicked tile.
        let path = Trajectory::new(from, at(8, 5), 7, 2.0, 0.0).unwrap();
        assert_eq!(walk(path, from), [at(6, 5), at(7, 5), at(8, 5)]);
        // A target beyond range: stops after `throw_range` tiles.
        let path = Trajectory::new(from, at(20, 5), 7, 2.0, 0.0).unwrap();
        let tiles = walk(path, from);
        assert_eq!(tiles.len(), 7);
        assert_eq!(tiles.last(), Some(&at(12, 5)));
        // Pure diagonal.
        let path = Trajectory::new(from, at(8, 8), 7, 2.0, 0.0).unwrap();
        assert_eq!(walk(path, from), [at(6, 6), at(7, 7), at(8, 8)]);
        // Own tile: already there.
        let path = Trajectory::new(from, from, 7, 2.0, 0.0).unwrap();
        assert!(walk(path, from).is_empty());
        // No speed: no throw (tg `throw_at` returns).
        assert!(Trajectory::new(from, at(8, 5), 7, 0.0, 0.0).is_none());
    }

    #[test]
    fn slanted_throws_follow_the_tg_line() {
        // 4 east, 2 north: tg's error term mixes straight and diagonal steps and ends on target.
        let from = at(0, 0);
        let path = Trajectory::new(from, at(4, 2), 7, 2.0, 0.0).unwrap();
        let tiles = walk(path, from);
        assert_eq!(tiles.last(), Some(&at(4, 2)));
        assert!(tiles.len() <= 4, "{tiles:?}");
        for pair in tiles.windows(2) {
            assert_eq!(pair[0].chebyshev(pair[1]), Some(1), "{tiles:?}");
        }
    }

    #[test]
    fn crashes_hurt_like_carbon_throw_impact() {
        // A wall always hurts; a structure or a machine only a carbon thrown faster than it
        // throws itself (a neck grab throw: one faster).
        assert_eq!(impact_damage(Crash::Wall, 0.0), ((2.0, 10.0), None));
        assert_eq!(impact_damage(Crash::Wall, 1.0), ((2.0, 15.0), None));
        assert_eq!(impact_damage(Crash::Structure, 0.0), ((0.0, 0.0), None));
        assert_eq!(impact_damage(Crash::Structure, 1.0), ((1.0, 10.0), None));
        assert_eq!(impact_damage(Crash::Other, 0.0), ((0.0, 0.0), None));
        assert_eq!(impact_damage(Crash::Other, 2.0), ((0.0, 10.0), None));
        // Into another carbon: both go down.
        assert_eq!(
            impact_damage(Crash::Carbon, 0.0),
            ((2.0, 10.0), Some((2.0, 10.0)))
        );
    }

    #[test]
    fn steps_per_tick_follow_tg_speed() {
        // Speed 2 (a toolbox): one tile per tick, starting at once.
        let mut path = Trajectory::new(at(0, 0), at(30, 0), 30, 2.0, 100.0).unwrap();
        let mut total = 0;
        for tick in 0..6 {
            let n = path.tiles_this_tick(100.0 + f64::from(tick) * TICK_LAG_DS);
            path.dist_travelled += n;
            total += n;
            assert_eq!(n, 1, "tick {tick}");
        }
        assert_eq!(total, 6);
        // Speed 4: about two tiles per tick (tg's rounding makes the second tick one).
        let mut path = Trajectory::new(at(0, 0), at(30, 0), 30, 4.0, 0.0).unwrap();
        let mut steps = vec![];
        for tick in 0..6 {
            let n = path.tiles_this_tick(f64::from(tick) * TICK_LAG_DS);
            path.dist_travelled += n;
            steps.push(n);
        }
        assert_eq!(steps, [2, 1, 2, 2, 2, 2]);
    }
}
