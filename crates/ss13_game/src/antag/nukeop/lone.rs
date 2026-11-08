//! The lone operative (server) after tg `/datum/round_event_control/operative` (BandaStation:
//! `min_players = 30`), `/datum/antagonist/nukeop/lone`, `/datum/team/nuclear/loneop` and the
//! disk's `/datum/component/keep_me_secure`.
//!
//! - **The disk's comfort** ([`disk_security`], every 2 s like `SSobj`): the disk is secure
//!   when fewer than 10 players live, or when it moved since the last check and a player holds
//!   it ([`is_secured`]). Secure, `prob(weight)` takes one off the event's weight; left alone
//!   for over 500 s, each check has `(unmoved deciseconds - 5000) × 0.0001` % to add one
//!   ([`unsecured_chance`]). The weight starts at 0, so a guarded disk never calls anyone.
//! - **The event** ([`make`]): once, from 20 minutes and 30 players, picked by that weight. The
//!   first ghost opted in to "Lone Operative" spawns at a carp spawn in space as the lone
//!   operative job with the lone operative's full kit, an uplink of 25 TC for lone operatives,
//!   its own team (a syndicate name, the self-destruct terminal's code) and "nuke the station".
//!   Nobody willing or no carp spawn: the event ends without a spawn.
// shortcut: being pulled does not count as held; the disk's bedsheet "sleeps soundly" message
// and the admin weight messages are not modelled; the body is the player's character, not a
// random one (`randomize_human_appearance`).

use bevy::prelude::*;
use ss13_shared::TilePos;

use super::server::{NukeDisk, OpKind, add_op, spawn_lone_team, willing_ghost};
use crate::events::controller::{EventRng, EventState, Events, RoundEvent};
use crate::inventory::CarriedBy;
use crate::server::PlayerMob;

/// `/datum/round_event_control/operative` `name`.
pub const EVENT_NAME: &str = "Lone Operative";
/// The disk's `keep_me_secure` `min_pop_limit`.
pub const MIN_POP_LIMIT: u32 = 10;
/// `last_move < world.time - 500 SECONDS`.
pub const UNMOVED_SECS: f64 = 500.0;
/// `SSobj` wait.
const PROCESS_SECS: f64 = 2.0;

/// `keep_me_secure`'s `last_secured_location` and `last_move`, on the disk.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct DiskSecurity {
    pub last_secured: Option<TilePos>,
    /// Game seconds.
    pub last_move: f64,
}

/// `is_secured()` over its inputs: the living players, whether the disk is where it was last
/// secure, and whether a player holds it.
pub fn is_secured(living: u32, unmoved: bool, held_by_player: bool) -> bool {
    if living < MIN_POP_LIMIT {
        return true;
    }
    !unmoved && held_by_player
}

/// `unsecured_process`' chance, percent: `(world.time - 500 SECONDS - last_move) × 0.0001` in
/// deciseconds, once the disk has been left alone for over 500 s.
pub fn unsecured_chance(now: f64, last_move: f64) -> f64 {
    if last_move < now - UNMOVED_SECS {
        (now - UNMOVED_SECS - last_move) * 10.0 * 0.0001
    } else {
        0.0
    }
}

/// `get(parent, /mob)` with a client: the player carrying the disk, directly or in a bag.
fn player_holder(world: &World, disk: Entity) -> Option<Entity> {
    let mut e = disk;
    for _ in 0..8 {
        e = world
            .get::<CarriedBy>(e)
            .map(|c| c.0)
            .or_else(|| world.get::<crate::storage::StoredIn>(e).map(|s| s.0))?;
        if world.get::<PlayerMob>(e).is_some() {
            return Some(e);
        }
    }
    None
}

/// The lone operative event control's index while it may still occur.
fn loneop_control(world: &World) -> Option<usize> {
    let events = world.get_resource::<Events>()?;
    let i = events.index(EVENT_NAME)?;
    let c = &events.controls[i];
    (c.occurrences < c.max_occurrences).then_some(i)
}

/// `keep_me_secure/process` with the disk's `secured_process` / `unsecured_process`.
pub fn disk_security(world: &mut World, mut next: Local<f64>) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now < *next {
        return;
    }
    *next = now + PROCESS_SECS;
    let mut disks = world.query_filtered::<Entity, With<NukeDisk>>();
    let disks: Vec<Entity> = disks.iter(world).collect();
    if disks.is_empty() {
        return;
    }
    let living = crate::events::controller::active_players(world);
    for disk in disks {
        let existing = world.get::<DiskSecurity>(disk).copied();
        let state = match existing {
            Some(s) => s,
            None => {
                let fresh = DiskSecurity {
                    last_secured: None,
                    last_move: now,
                };
                world.entity_mut(disk).insert(fresh);
                fresh
            }
        };
        let turf = crate::modpc::pda::turf_of(world, disk);
        let unmoved = state.last_secured.is_some() && state.last_secured == turf;
        let held = player_holder(world, disk).is_some();
        let control = loneop_control(world);
        if is_secured(living, unmoved, held) {
            world.entity_mut(disk).insert(DiskSecurity {
                last_secured: turf,
                last_move: now,
            });
            let Some(i) = control else {
                continue;
            };
            let weight = world.resource::<Events>().controls[i].weight;
            let roll = world
                .get_resource_mut::<EventRng>()
                .is_some_and(|mut r| r.0.prob(weight as f32));
            if roll {
                let mut events = world.resource_mut::<Events>();
                events.controls[i].weight = (weight - 1).max(0);
                debug!("the disk is secured: lone operative weight {}", weight - 1);
            }
        } else {
            let Some(i) = control else {
                continue;
            };
            let chance = unsecured_chance(now, state.last_move);
            if chance <= 0.0 {
                continue;
            }
            let roll = world
                .get_resource_mut::<EventRng>()
                .is_some_and(|mut r| r.0.prob(chance as f32));
            if roll {
                let mut events = world.resource_mut::<Events>();
                events.controls[i].weight += 1;
                info!(
                    "the disk was left unsecured at {turf:?}: lone operative weight {}",
                    events.controls[i].weight
                );
            }
        }
    }
}

/// `/datum/round_event/ghost_role/operative`.
pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(LoneOperative)
}

struct LoneOperative;

impl RoundEvent for LoneOperative {
    fn start(&mut self, world: &mut World, _: &mut EventState) {
        spawn_lone_operative(world);
    }
}

/// `spawn_role()`: the lone operative's body, or `None` (`NOT_ENOUGH_PLAYERS`, `MAP_ERROR`).
pub fn spawn_lone_operative(world: &mut World) -> Option<Entity> {
    let Some((_, client)) = willing_ghost(world, super::ROLE_LONE_OPERATIVE) else {
        info!("lone operative cannot be spawned due to lack of players signing up");
        return None;
    };
    // `find_space_spawn`: a random carp spawn.
    let spawns = world
        .get_resource::<crate::server::SpawnPoints>()
        .map(|s| s.carp.clone())
        .unwrap_or_default();
    if spawns.is_empty() {
        info!("lone operative cannot be spawned due to a map error");
        return None;
    }
    let mut rng = super::server::take_rng(world);
    let pos = spawns[rng.range(0, spawns.len() as i64 - 1) as usize];
    super::server::put_rng(world, rng);
    let mob = crate::server::spawn_mob(world, client, pos, String::new());
    crate::noncrew_jobs::set_assigned_role(world, mob, super::server::LONE_OPERATIVE_JOB);
    spawn_lone_team(world);
    let mut rng = super::server::take_rng(world);
    add_op(world, mob, OpKind::Lone, true, 0, None, &mut rng);
    super::server::put_rng(world, rng);
    info!("{mob} has been made into lone operative by an event at {pos:?}");
    Some(mob)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_disk_is_secure_only_moving_in_a_players_hands() {
        // Low population: always secure.
        assert!(is_secured(9, true, false));
        assert!(is_secured(10, false, true));
        assert!(!is_secured(10, true, true));
        assert!(!is_secured(30, false, false));
    }

    fn loneop_world() -> World {
        use crate::events::controller::EventControl;
        let mut world = World::new();
        world.init_resource::<Time>();
        world.insert_resource(EventRng(ss13_mapload::spawner::Rng::new(3)));
        let mut events = Events::default();
        events.controls = vec![EventControl {
            name: EVENT_NAME,
            weight: 0,
            earliest_start: 1200.0,
            min_players: 30,
            max_occurrences: 1,
            occurrences: 0,
            fakeable: false,
            admin_disabled: false,
            wizard: false,
            holiday: None,
            available: None,
            make,
        }];
        world.insert_resource(events);
        // Enough living players for the disk to need guarding.
        for _ in 0..MIN_POP_LIMIT {
            world.spawn((
                PlayerMob {
                    client: Entity::PLACEHOLDER,
                },
                crate::health::Health::default(),
                crate::health::Stat::Conscious,
            ));
        }
        world
    }

    #[test]
    fn a_neglected_disk_calls_the_lone_operative() {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = loneop_world();
        let disk = world.spawn((NukeDisk, TilePos::new(5, 5, 1))).id();
        world.run_system_once(disk_security).unwrap();
        assert_eq!(world.get::<DiskSecurity>(disk).unwrap().last_move, 0.0);
        // Left on the floor for 10000 s: 9.5% per check.
        world
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs(10_000));
        for _ in 0..200 {
            world.run_system_once(disk_security).unwrap();
        }
        let weight = world.resource::<Events>().controls[0].weight;
        assert!((5..60).contains(&weight), "{weight}");
        // Once it happened, nothing changes the weight any more.
        world.resource_mut::<Events>().controls[0].occurrences = 1;
        world.run_system_once(disk_security).unwrap();
        assert_eq!(world.resource::<Events>().controls[0].weight, weight);
    }

    #[test]
    fn neglect_grows_the_chance_after_five_hundred_seconds() {
        assert_eq!(unsecured_chance(500.0, 0.0), 0.0);
        // 1000 s unmoved: 5000 ds over the limit, 0.5% per check.
        assert!((unsecured_chance(1000.0, 0.0) - 0.5).abs() < 1e-9);
        assert!((unsecured_chance(2500.0, 500.0) - 1.5).abs() < 1e-9);
    }
}
