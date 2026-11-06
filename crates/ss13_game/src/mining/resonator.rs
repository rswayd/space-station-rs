//! The resonator (server), after tg `/obj/item/resonator` and
//! `/obj/effect/temp_visual/resonance` (`code/modules/mining/equipment/resonator.dm`).
//!
//! - Used in hand it switches modes: automatic (fields burst after 2 s) and manual (they wait
//!   up to 60 s for a hit); the upgraded resonator also sets matrices that burst when something
//!   steps in.
//! - Hitting a tile (`pre_attack`) bursts the field already there (at `quick_burst_mod` damage)
//!   or, below `fieldlimit` fields, sets a new one there (`CLICK_CD_MELEE`). On a mob the
//!   attack follows.
//! - A burst drills a mineral wall on its tile, does 20 brute (×3 at low pressure,
//!   `lavaland_equipment_pressure_check`) to every living thing there, bursts the fields
//!   around it and, unless it fails (`failure_prob`), spreads new fields into the mineral walls
//!   around it, each more likely to fail by the resonator's `adding_failure`.
// tg's spread fields take the default `adding_failure` of 50 instead of the resonator's (30
// for the upgraded one): the resonator's is used here, as the var intends.
// shortcut: no `resonance` movespeed modifier on the hit (10 s slowdown), no crush visual and
// no growing animation of the automatic field; upgrade with movespeed modifiers.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_world::{Moved, SpatialIndex};

use crate::health::{DamageType, Health};
use crate::sound::{PlaySound, playsound};
use crate::tools::tell;

pub const RESONATOR: &str = "/obj/item/resonator";
pub const UPGRADED: &str = "/obj/item/resonator/upgraded";
const FIELD: &str = "/obj/effect/temp_visual/resonance";
const FIRE_SOUND: &str = "sound/items/weapons/resonator_fire.ogg";
const BLAST_SOUND: &str = "sound/items/weapons/resonator_blast.ogg";
/// `resonance_damage`.
pub const RESONANCE_DAMAGE: f32 = 20.0;
/// `duration` of a manual field and a matrix; an automatic one's.
const DURATION: f64 = 60.0;
const AUTO_DURATION: f64 = 2.0;

/// `RESONATOR_MODE_*`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Auto,
    Manual,
    Matrix,
}

/// A resonator's `mode`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Resonator {
    pub mode: Mode,
}

/// A resonance field on `pos`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ResonanceField {
    pub pos: TilePos,
    pub creator: Option<Entity>,
    pub resonator: Option<Entity>,
    pub mode: Mode,
    pub burst_at: f64,
    pub damage_multiplier: f32,
    pub failure_prob: f32,
    pub adding_failure: f32,
}

/// `fieldlimit`, `quick_burst_mod`, `adding_failure` of a resonator type.
fn stats(path: &str) -> (usize, f32, f32) {
    if is_under(path, UPGRADED) {
        (6, 1.0, 30.0)
    } else {
        (4, 0.8, 50.0)
    }
}

pub(super) struct ResonatorPlugin;

impl Plugin for ResonatorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (matrix_steps, timed_bursts).chain());
    }
}

fn now(world: &World) -> f64 {
    world
        .get_resource::<Time>()
        .map_or(0.0, |t| t.elapsed_secs_f64())
}

fn resonator_path(world: &World, e: Entity) -> Option<String> {
    world
        .get::<DmItem>(e)
        .map(|i| i.path.clone())
        .filter(|p| is_under(p, RESONATOR))
}

/// `attack_self`: the next mode. True when `item` is a resonator.
pub fn attack_self(world: &mut World, client: Entity, item: Entity) -> bool {
    let Some(path) = resonator_path(world, item) else {
        return false;
    };
    let mode = world
        .get::<Resonator>(item)
        .copied()
        .unwrap_or_default()
        .mode;
    let (next, text) = match mode {
        Mode::Auto => (
            Mode::Manual,
            "You set the resonator's fields to detonate only after you hit one with it.",
        ),
        Mode::Manual if is_under(&path, UPGRADED) => (
            Mode::Matrix,
            "You set the resonator's fields to work as matrix traps.",
        ),
        _ => (
            Mode::Auto,
            "You set the resonator's fields to automatically detonate after 2 seconds.",
        ),
    };
    world.entity_mut(item).insert(Resonator { mode: next });
    tell(world, client, Span::Info, text);
    true
}

fn field_at(world: &mut World, pos: TilePos) -> Option<Entity> {
    world
        .query::<(Entity, &ResonanceField)>()
        .iter(world)
        .find(|(_, f)| f.pos == pos)
        .map(|(e, _)| e)
}

/// `pre_attack` -> `create_resonance` on the clicked thing's tile. True when the click ends
/// here (not on a living mob, which the resonator then hits).
pub fn item_interaction(world: &mut World, mob: Entity, held: Entity, target: Entity) -> bool {
    let Some(path) = resonator_path(world, held) else {
        return false;
    };
    // `check_allowed_items(not_inside = TRUE)`: a thing on a tile, not carried or stored.
    if world.get::<crate::inventory::CarriedBy>(target).is_some()
        || world.get::<crate::storage::Stored>(target).is_some()
    {
        return false;
    }
    let Some(&pos) = world.get::<TilePos>(target) else {
        return false;
    };
    create_resonance(world, mob, held, &path, pos);
    world.get::<Health>(target).is_none()
}

/// `create_resonance`.
pub fn create_resonance(
    world: &mut World,
    user: Entity,
    resonator: Entity,
    path: &str,
    pos: TilePos,
) {
    let (limit, quick_burst, adding_failure) = stats(path);
    if let Some(field) = field_at(world, pos) {
        if let Some(mut f) = world.get_mut::<ResonanceField>(field) {
            f.damage_multiplier = quick_burst;
        }
        burst(world, field);
        return;
    }
    let count = world
        .query::<&ResonanceField>()
        .iter(world)
        .filter(|f| f.resonator == Some(resonator))
        .count();
    if count >= limit {
        return;
    }
    let mode = world
        .get::<Resonator>(resonator)
        .copied()
        .unwrap_or_default()
        .mode;
    spawn_field(
        world,
        pos,
        Some(user),
        Some(resonator),
        mode,
        0.0,
        adding_failure,
    );
    let t = now(world);
    if let Some(mut next) = world.get_mut::<crate::combat::NextMove>(user) {
        next.0 = t + crate::combat::CLICK_CD_MELEE;
    }
}

/// `/obj/effect/temp_visual/resonance/Initialize`.
fn spawn_field(
    world: &mut World,
    pos: TilePos,
    creator: Option<Entity>,
    resonator: Option<Entity>,
    mode: Mode,
    failure_prob: f32,
    adding_failure: f32,
) -> Entity {
    let duration = if mode == Mode::Auto {
        AUTO_DURATION
    } else {
        DURATION
    };
    let field = ResonanceField {
        pos,
        creator,
        resonator,
        mode,
        burst_at: now(world) + duration,
        damage_multiplier: 1.0,
        failure_prob,
        adding_failure,
    };
    let e = crate::construction::spawn_obj(world, FIELD, pos)
        .unwrap_or_else(|| world.spawn_empty().id());
    world.entity_mut(e).insert(field);
    playsound(world, PlaySound::new(pos, FIRE_SOUND, 50.0, true));
    e
}

/// `COMSIG_ATOM_ENTERED` of a matrix: whatever steps in bursts it.
fn matrix_steps(world: &mut World, mut cursor: Local<MessageCursor<Moved>>) {
    let moved: Vec<TilePos> = cursor
        .read(world.resource::<Messages<Moved>>())
        .map(|m| m.to)
        .collect();
    if moved.is_empty() {
        return;
    }
    let matrices: Vec<Entity> = world
        .query::<(Entity, &ResonanceField)>()
        .iter(world)
        .filter(|(_, f)| f.mode == Mode::Matrix && moved.contains(&f.pos))
        .map(|(e, _)| e)
        .collect();
    for field in matrices {
        burst(world, field);
    }
}

fn timed_bursts(world: &mut World) {
    let t = now(world);
    let due: Vec<Entity> = world
        .query::<(Entity, &ResonanceField)>()
        .iter(world)
        .filter(|(_, f)| f.burst_at <= t)
        .map(|(e, _)| e)
        .collect();
    for field in due {
        burst(world, field);
    }
}

fn is_mineral(world: &World, pos: TilePos) -> bool {
    crate::construction::turf_at(world, pos)
        .is_some_and(|t| world.get::<ss13_mapload::DmMineral>(t).is_some())
}

/// `orange(1, pos)` on its level.
fn around(pos: TilePos) -> impl Iterator<Item = TilePos> {
    (-1i32..=1)
        .flat_map(move |dy| (-1i32..=1).map(move |dx| (dx, dy)))
        .filter(|&(dx, dy)| (dx, dy) != (0, 0))
        .map(move |(dx, dy)| TilePos {
            x: pos.x + dx,
            y: pos.y + dy,
            z: pos.z,
        })
}

/// The damage of a burst on `pos` (`check_pressure`).
pub fn burst_damage(low_pressure: bool, multiplier: f32) -> f32 {
    let base = if low_pressure {
        RESONANCE_DAMAGE * 3.0
    } else {
        RESONANCE_DAMAGE
    };
    base * multiplier
}

/// `burst`. The field is gone before the fields around it burst, so a chain never comes back
/// to it (`rupturing`).
pub fn burst(world: &mut World, field: Entity) {
    let Some(f) = world.get::<ResonanceField>(field).copied() else {
        return;
    };
    world.despawn(field);
    let pos = f.pos;
    if is_mineral(world, pos) {
        super::rocks::gets_drilled(world, pos, f.creator);
    }
    let damage = burst_damage(
        !super::rocks::pressure_over(world, pos),
        f.damage_multiplier,
    );
    playsound(world, PlaySound::new(pos, BLAST_SOUND, 50.0, true));
    let living: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| world.get::<Health>(e).is_some())
        .collect();
    for mob in living {
        if let Some(client) = world.get::<crate::server::PlayerMob>(mob).map(|p| p.client) {
            tell(
                world,
                client,
                Span::Danger,
                "The resonance field ruptured with you in it!",
            );
        }
        crate::mobs::lavaland::hurt(world, mob, DamageType::Brute, damage, None);
    }
    for near in around(pos).collect::<Vec<_>>() {
        if let Some(other) = field_at(world, near) {
            burst(world, other);
        }
    }
    let resonator_alive = f.resonator.is_some_and(|r| world.get_entity(r).is_ok());
    let spreads = !world
        .resource_mut::<crate::combat::CombatRng>()
        .chance(f.failure_prob);
    if !spreads || !resonator_alive {
        return;
    }
    let mode = f
        .resonator
        .and_then(|r| world.get::<Resonator>(r).copied())
        .unwrap_or_default()
        .mode;
    for near in around(pos).collect::<Vec<_>>() {
        if is_mineral(world, near) && field_at(world, near).is_none() {
            spawn_field(
                world,
                near,
                f.creator,
                f.resonator,
                mode,
                f.failure_prob + f.adding_failure,
                f.adding_failure,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_pressure_triples_and_quick_burst_softens() {
        assert_eq!(burst_damage(false, 1.0), 20.0);
        assert_eq!(burst_damage(true, 1.0), 60.0);
        assert!((burst_damage(true, 0.8) - 48.0).abs() < 1e-4);
    }

    #[test]
    fn upgraded_resonator_has_more_fields_and_spreads_further() {
        assert_eq!(stats(RESONATOR), (4, 0.8, 50.0));
        assert_eq!(stats(UPGRADED), (6, 1.0, 30.0));
    }

    #[test]
    fn around_is_the_eight_neighbours() {
        let pos = TilePos { x: 5, y: 5, z: 1 };
        let tiles: Vec<_> = around(pos).collect();
        assert_eq!(tiles.len(), 8);
        assert!(!tiles.contains(&pos));
        assert!(tiles.contains(&TilePos { x: 4, y: 6, z: 1 }));
    }

    #[test]
    fn modes_cycle_with_matrix_only_when_upgraded() {
        let mut world = World::new();
        world.init_resource::<Messages<ss13_net::chat::server::SendChat>>();
        let client = world.spawn_empty().id();
        let plain = world
            .spawn(DmItem {
                path: RESONATOR.to_owned(),
                ..default()
            })
            .id();
        let upgraded = world
            .spawn(DmItem {
                path: UPGRADED.to_owned(),
                ..default()
            })
            .id();
        let mode = |w: &World, e| w.get::<Resonator>(e).unwrap().mode;
        for (item, expected) in [
            (plain, [Mode::Manual, Mode::Auto, Mode::Manual]),
            (upgraded, [Mode::Manual, Mode::Matrix, Mode::Auto]),
        ] {
            for m in expected {
                assert!(attack_self(&mut world, client, item));
                assert_eq!(mode(&world, item), m);
            }
        }
    }
}
