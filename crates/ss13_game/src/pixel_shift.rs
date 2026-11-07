//! Pixel shift (server), BandaStation `modular_bandastation/pixel_shift`
//! (`/datum/component/pixel_shift`, `/datum/keybinding/mob/pixel_shift`).
//!
//! While the pixel shift key is held the client sends `ClientIntent::PixelShift(dir)` instead
//! of steps; each moves the mob's sprite one pixel that way (cardinals only, at most
//! [`MAX_SHIFT`] from its base offset) unless it is restrained, immobilized, pulled, lying,
//! buckled, inside something or not conscious. Moving, being pulled or falling over resets it
//! ([`PixelShift`] is removed; `crate::health::lying_appearance` draws the offset on top of the
//! mob's own). Past [`PASSABLE_THRESHOLD`] the mob lets others into its tile from the sides it
//! leaves free (`passthroughable`, [`passable_from`]).
// shortcut: `passable_from` is the rule only; mob density is not consulted by the movement code
// per direction yet, so walking through a shifted mob waits for a per-direction `CanAllowThrough`
// hook in `ss13_world`. Basic mobs (no `BodyPosition`) do not shift. The layer shift verbs are not
// ported (no verb panel).

use bevy::prelude::*;
use ss13_net::server::IntentReceived;
use ss13_shared::protocol::ClientIntent;
use ss13_shared::{Appearance, Dir, TilePos};
use ss13_world::WorldSystems;

use crate::health::{BodyPosition, Stat};
use crate::server::Players;

/// `maximum_pixel_shift`, `passable_shift_threshold`.
pub const MAX_SHIFT: i16 = 12;
pub const PASSABLE_THRESHOLD: i16 = 8;

/// `/datum/component/pixel_shift`: the shift `[x, y]` in pixels (tg `pixel_w`, `pixel_z`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PixelShift(pub [i16; 2]);

impl PixelShift {
    /// One pixel `dir` (cardinals), capped at [`MAX_SHIFT`]. False when nothing changed.
    pub fn shift(&mut self, dir: Dir) -> bool {
        let (axis, delta) = match dir {
            Dir::North => (1, 1),
            Dir::South => (1, -1),
            Dir::East => (0, 1),
            Dir::West => (0, -1),
            _ => return false,
        };
        let next = (self.0[axis] + delta).clamp(-MAX_SHIFT, MAX_SHIFT);
        let changed = next != self.0[axis];
        self.0[axis] = next;
        changed
    }
}

/// `passthroughable`: the BYOND dir bits from which others may enter the shifted mob's tile.
pub fn passable_from(shift: [i16; 2]) -> u8 {
    let (n, s, e, w) = (1u8, 2u8, 4u8, 8u8);
    let mut bits = 0;
    if shift[1] > PASSABLE_THRESHOLD {
        bits |= e | s | w;
    } else if shift[1] < -PASSABLE_THRESHOLD {
        bits |= n | e | w;
    }
    if shift[0] > PASSABLE_THRESHOLD {
        bits |= n | s | w;
    } else if shift[0] < -PASSABLE_THRESHOLD {
        bits |= n | e | s;
    }
    bits
}

pub struct PixelShiftPlugin;

impl Plugin for PixelShiftPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (unshift, shift_intents)
                .chain()
                .after(WorldSystems::Movement),
        );
    }
}

/// The checks of `pixel_shift` plus the port's: a body that stands, not buckled, not inside.
fn can_shift(world: &World, mob: Entity) -> bool {
    let traits = crate::status_effects::incapacitation(world, mob);
    let entity = world.entity(mob);
    entity.get::<Stat>() == Some(&Stat::Conscious)
        && entity.get::<BodyPosition>().is_some_and(|p| !p.lying())
        && !traits.restrained
        && !traits.immobilized
        && !traits.floored
        && !entity.contains::<crate::pull::PulledBy>()
        && !entity.contains::<crate::medical::surgery::Buckled>()
        && !entity.contains::<crate::closets::InCloset>()
        && !entity.contains::<crate::vehicles::Rider>()
}

fn shift_intents(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<IntentReceived>>,
) {
    let shifts: Vec<(Entity, Dir)> = cursor
        .read(world.resource::<bevy::ecs::message::Messages<IntentReceived>>())
        .filter_map(|i| match i.intent {
            ClientIntent::PixelShift(dir) => Some((i.client, dir)),
            _ => None,
        })
        .collect();
    for (client, dir) in shifts {
        let Some(mob) = world.resource::<Players>().mob(client) else {
            continue;
        };
        if world.get_entity(mob).is_err() || !can_shift(world, mob) {
            continue;
        }
        let mut shift = world.get::<PixelShift>(mob).copied().unwrap_or_default();
        if shift.shift(dir) {
            world.entity_mut(mob).insert(shift);
        }
    }
}

/// `unpixel_shift` on `COMSIG_MOVABLE_MOVED`, a pull, `TRAIT_FLOORED`, or anything else that
/// makes the shift invalid.
fn unshift(world: &mut World, mut last: Local<Vec<(Entity, TilePos)>>) {
    let mut query = world.query_filtered::<(Entity, &TilePos), With<PixelShift>>();
    let now: Vec<(Entity, TilePos)> = query.iter(world).map(|(e, &p)| (e, p)).collect();
    for &(mob, pos) in &now {
        let moved = last
            .iter()
            .find(|(e, _)| *e == mob)
            .is_some_and(|&(_, was)| was != pos);
        if moved || !can_shift(world, mob) {
            clear(world, mob);
        }
    }
    *last = now
        .into_iter()
        .filter(|&(e, _)| world.get::<PixelShift>(e).is_some())
        .collect();
}

/// Removes the shift and has the look redrawn without it.
pub fn clear(world: &mut World, mob: Entity) {
    let Ok(mut entity) = world.get_entity_mut(mob) else {
        return;
    };
    if entity.take::<PixelShift>().is_some()
        && let Some(mut look) = entity.get_mut::<Appearance>()
    {
        look.set_changed();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::message::Messages;

    #[test]
    fn intents_shift_a_standing_mob_and_a_step_resets_it() {
        let mut world = World::new();
        world.init_resource::<Players>();
        world.init_resource::<Messages<IntentReceived>>();
        let client = world.spawn_empty().id();
        let mob = world
            .spawn((
                TilePos::new(5, 5, 1),
                Stat::Conscious,
                BodyPosition::default(),
            ))
            .id();
        world.resource_mut::<Players>().set_mob(client, mob);
        let mut schedule = Schedule::default();
        schedule.add_systems((unshift, shift_intents).chain());
        let send = |world: &mut World, dir: Dir| {
            world.write_message(IntentReceived {
                client,
                intent: ClientIntent::PixelShift(dir),
            });
        };
        send(&mut world, Dir::East);
        send(&mut world, Dir::East);
        send(&mut world, Dir::South);
        schedule.run(&mut world);
        assert_eq!(world.get::<PixelShift>(mob), Some(&PixelShift([2, -1])));
        // Unconscious: refused.
        world.entity_mut(mob).insert(Stat::SoftCrit);
        send(&mut world, Dir::East);
        schedule.run(&mut world);
        assert!(world.get::<PixelShift>(mob).is_none(), "crit resets it");
        world.entity_mut(mob).insert(Stat::Conscious);
        send(&mut world, Dir::North);
        schedule.run(&mut world);
        assert_eq!(world.get::<PixelShift>(mob), Some(&PixelShift([0, 1])));
        schedule.run(&mut world);
        // A step resets it.
        world.entity_mut(mob).insert(TilePos::new(6, 5, 1));
        schedule.run(&mut world);
        assert!(world.get::<PixelShift>(mob).is_none());
    }

    #[test]
    fn shifts_are_capped_cardinal_and_open_the_tile_past_the_threshold() {
        let mut shift = PixelShift::default();
        for _ in 0..20 {
            shift.shift(Dir::North);
        }
        assert_eq!(shift.0, [0, MAX_SHIFT]);
        assert!(!shift.shift(Dir::North), "capped");
        assert!(!shift.shift(Dir::NorthEast), "diagonals do nothing");
        assert!(shift.shift(Dir::West));
        assert_eq!(shift.0, [-1, MAX_SHIFT]);
        // Shifted north past 8 px: others come in from the east, south and west.
        assert_eq!(passable_from(shift.0), 2 | 4 | 8);
        assert_eq!(passable_from([0, PASSABLE_THRESHOLD]), 0);
        assert_eq!(passable_from([-9, 0]), 1 | 4 | 2);
    }
}
