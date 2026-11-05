//! Tripping from running too fast (server), BandaStation `movespeed_limit`
//! (`modular_bandastation/movespeed_limit/code/movement.dm`, `check_movespeed`).
//!
//! After every move of a human whose move delay (`cached_multiplicative_slowdown`) is under
//! 0.85 deciseconds: a 50% chance to fall (2 s knockdown); under 0.3 the fall may also hurt
//! both legs by 40 brute, the likelier the faster.
// shortcut: no `status_speed_boost` exemption (no status effect grants that modifier yet).

use bevy::ecs::entity::EntityHashSet;
use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_shared::chat::Span;
use ss13_world::{Moved, Mover, WorldSystems};

use crate::combat::CombatRng;
use crate::health::{DamageType, HealthSystems};
use crate::human::HumanLook;
use crate::server::PlayerMob;
use crate::tools::tell;
use crate::zone::BodyZone;

/// `GLOB.legs_trip_threshold`, `GLOB.legs_break_threshold`, deciseconds per tile.
const TRIP_THRESHOLD: f32 = 0.85;
const BREAK_THRESHOLD: f32 = 0.3;
const TRIP_CHANCE: f32 = 50.0;
const KNOCKDOWN: f32 = 2.0;
const LEG_BRUTE: f32 = 40.0;

pub struct StumblePlugin;

impl Plugin for StumblePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            stumble.after(WorldSystems::Movement).before(HealthSystems),
        );
    }
}

/// What a move at `delay_ds` does, given the two rolls: `None`, a fall, or a fall on hurt legs.
pub fn trip(delay_ds: f32, trip_roll: bool, break_roll: impl FnOnce(f32) -> bool) -> Option<bool> {
    if delay_ds >= TRIP_THRESHOLD || !trip_roll {
        return None;
    }
    let excess = BREAK_THRESHOLD - delay_ds;
    Some(delay_ds < BREAK_THRESHOLD && break_roll(excess / BREAK_THRESHOLD * 100.0))
}

// Cost: per tick, one lookup per moved thing.
fn stumble(
    world: &mut World,
    mut cursor: Local<MessageCursor<Moved>>,
    mut seen: Local<EntityHashSet>,
) {
    let moved: Vec<Entity> = cursor
        .read(world.resource::<Messages<Moved>>())
        .map(|m| m.entity)
        .collect();
    seen.clear();
    for mob in moved {
        // A diagonal move is one `Move` (two `Moved`).
        if !seen.insert(mob) || world.get::<HumanLook>(mob).is_none() {
            continue;
        }
        let Some(delay_ds) = world.get::<Mover>(mob).map(|m| m.delay * 10.0) else {
            continue;
        };
        if delay_ds >= TRIP_THRESHOLD {
            continue;
        }
        let roll = {
            let mut rng = world.get_resource_or_init::<CombatRng>();
            let fall = rng.chance(TRIP_CHANCE);
            trip(delay_ds, fall, |chance| rng.chance(chance))
        };
        let Some(broke) = roll else {
            continue;
        };
        if broke {
            crate::medical::hurt_limb(world, mob, BodyZone::LLeg, DamageType::Brute, LEG_BRUTE);
            crate::medical::hurt_limb(world, mob, BodyZone::RLeg, DamageType::Brute, LEG_BRUTE);
        }
        crate::status_effects::knockdown(world, mob, KNOCKDOWN, 0.0);
        if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
            let text = if broke {
                "Вы повредили ноги, споткнувшись от слишком быстрого бега!"
            } else {
                "Вы оступились и упали от слишком быстрого бега!"
            };
            tell(world, client, Span::Danger, text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trips_only_below_the_threshold() {
        assert_eq!(trip(0.85, true, |_| true), None);
        assert_eq!(trip(0.5, false, |_| true), None);
        assert_eq!(trip(0.5, true, |_| true), Some(false));
        // At 0.15 ds the legs break with (0.3 - 0.15) / 0.3 = 50%.
        let mut asked = 0.0;
        assert_eq!(
            trip(0.15, true, |c| {
                asked = c;
                true
            }),
            Some(true)
        );
        assert!((asked - 50.0).abs() < 1e-3);
    }
}
