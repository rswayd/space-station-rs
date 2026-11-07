//! Necropolis tendrils and chests (server), after tg `/mob/living/basic/mining/tendril`
//! (`code/modules/mob/living/basic/lavaland/tendril/tendril.dm`) and
//! `code/modules/mining/lavaland/necropolis_chests.dm` with the `mining_loot` spawners
//! (`code/game/objects/effects/spawners/random/mining_loot.dm`).
//!
//! - A tendril ([`Tendril`]) clears the rock within 4.5 tiles of it on `Initialize` and turns
//!   the asteroid floor there into necropolis floor (`prob(100 / sqrt(max(1, dist)))`),
//!   remembering each floor's type. It regenerates 20 brute a second after 30 s without damage
//!   (`/datum/component/regenerator`).
//! - Its death (`death`, `death_drops`) plays `tendril_destroyed`, leaves a tendril chest on its
//!   tile and turns the necropolis floors back after `round(5 - dist, 0.5)` seconds, only those
//!   still necropolis floor.
//! - Chests are plain crates, opened by hand (tg has no lock or key on them). The first opening
//!   (`PopulateContents`) of a tendril chest rolls one item of `mining_loot` (a demonic chest of
//!   `mining_loot/demonic`, the rare modkit entry rolls again from `pka_mod`), a puzzling chest
//!   one of three relics. The megafauna's chests are filled when they drop
//!   (`crate::mobs::megafauna`).
// shortcut: the tendril fights with nothing but its melee (no tentacle lash, chasing or cross
// spikes, no AI tree yet: it only stands), no heartbeat loop, GPS signal, achievements,
// emissive overlay, regeneration outline or `block_mining_mob_respawns` (the cave population
// does not respawn here); add each with its mechanic.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;

use crate::combat::CombatRng;
use crate::construction::{baseturfs_of, change_turf, path_of, scrape_away, turf_at};
use crate::health::{DamageType, Health, Stat};
use crate::mobs::BasicMob;
use crate::sound::{PlaySound, playsound};

pub const TENDRIL: &str = "/mob/living/basic/mining/tendril";
pub const NECROPOLIS_CHEST: &str = "/obj/structure/closet/crate/necropolis";
pub const TENDRIL_CHEST: &str = "/obj/structure/closet/crate/necropolis/tendril";
const DEMONIC_CHEST: &str = "/obj/structure/closet/crate/necropolis/tendril/demonic";
const PUZZLE_CHEST: &str = "/obj/structure/closet/crate/necropolis/puzzle";
pub const NECROPOLIS_FLOOR: &str = "/turf/open/indestructible/necropolis";
const ASTEROID: &str = "/turf/open/misc/asteroid";
const MINERAL: &str = "/turf/closed/mineral";
const DESTROYED_SOUND: &str = "sound/effects/tendril_destroyed.ogg";
/// The infection radius (`range(4)`, `get_dist_euclidean > 4.5` skipped).
const RADIUS: f32 = 4.5;
/// `/datum/component/regenerator`: `regeneration_delay`, `brute_per_second`.
const REGEN_DELAY: f64 = 30.0;
const REGEN_PER_SECOND: f32 = 20.0;
/// Stands for the `pka_mod` spawner inside the loot lists.
const PKA_MOD: &str = "/obj/effect/spawner/random/mining_loot/pka_mod";

/// `/obj/effect/spawner/random/mining_loot` `loot` (all weight 1).
pub const MINING_LOOT: &[&str] = &[
    "/obj/item/shared_storage/red",
    "/obj/item/soulstone/anybody/mining",
    "/obj/item/clothing/glasses/godeye",
    "/obj/item/reagent_containers/cup/bottle/potion/flight",
    "/obj/item/clothing/gloves/gauntlets",
    PKA_MOD,
    "/obj/item/rod_of_asclepius",
    "/obj/item/organ/heart/cursed/wizard",
    "/obj/item/ship_in_a_bottle",
    "/obj/item/clothing/suit/hooded/berserker",
    "/obj/item/jacobs_ladder",
    "/obj/item/guardian_creator/miner",
    "/obj/item/warp_cube/red",
    "/obj/item/wisp_lantern",
    "/obj/item/immortality_talisman",
    "/obj/item/book/granter/action/spell/summonitem",
    "/obj/item/book_of_babel",
    "/obj/item/clothing/neck/necklace/memento_mori",
];

/// `mining_loot/demonic` `loot` (all weight 1).
pub const DEMONIC_LOOT: &[&str] = &[
    "/obj/item/shared_storage/red",
    "/obj/item/clothing/neck/cloak/wolf_coat",
    "/obj/item/soulstone/anybody/mining",
    "/obj/item/clothing/glasses/godeye",
    "/obj/item/reagent_containers/cup/bottle/potion/flight",
    "/obj/item/clothing/gloves/gauntlets",
    PKA_MOD,
    "/obj/item/rod_of_asclepius",
    "/obj/item/organ/heart/cursed/wizard",
    "/obj/item/jacobs_ladder",
    "/obj/item/guardian_creator/miner",
    "/obj/item/weldingtool/abductor",
    "/obj/item/warp_cube/red",
    "/obj/item/immortality_talisman",
    "/obj/item/book/granter/action/spell/summonitem",
    "/obj/item/clothing/neck/necklace/memento_mori",
    "/obj/item/book/granter/action/spell/sacredflame",
];

/// `mining_loot/pka_mod` `loot` (all weight 1).
pub const PKA_MOD_LOOT: &[&str] = &[
    "/obj/item/borg/upgrade/modkit/cooldown/aoe/mobs/andturfs",
    "/obj/item/borg/upgrade/modkit/cooldown/repeater",
    "/obj/item/borg/upgrade/modkit/resonator_blasts",
    "/obj/item/borg/upgrade/modkit/bounty",
    "/obj/item/borg/upgrade/modkit/lifesteal",
];

/// `necropolis/puzzle/PopulateContents`: `rand(1, 3)`.
pub const PUZZLE_LOOT: &[&str] = &[
    "/obj/item/soulstone/anybody/mining",
    "/obj/item/wisp_lantern",
    "/obj/item/prisoncube",
];

/// Every constant path spawned here (`ss13_mapload::runtime::GAME_RUNTIME_TYPES` lists them).
pub fn runtime_paths() -> impl Iterator<Item = &'static str> {
    MINING_LOOT
        .iter()
        .chain(DEMONIC_LOOT)
        .chain(PKA_MOD_LOOT)
        .chain(PUZZLE_LOOT)
        .copied()
        .filter(|p| *p != PKA_MOD)
        .chain([TENDRIL_CHEST, NECROPOLIS_FLOOR])
}

/// The list a chest's `PopulateContents` picks from, `None` for other closets.
pub fn chest_loot(path: &str) -> Option<&'static [&'static str]> {
    if is_under(path, DEMONIC_CHEST) {
        Some(DEMONIC_LOOT)
    } else if is_under(path, TENDRIL_CHEST) {
        Some(MINING_LOOT)
    } else if is_under(path, PUZZLE_CHEST) {
        Some(PUZZLE_LOOT)
    } else {
        None
    }
}

fn pick(rng: &mut CombatRng, list: &[&'static str]) -> &'static str {
    list[rng.range(0, list.len() as u32 - 1) as usize]
}

/// One roll of `list` (`pick_weight_recursive`: the modkit spawner rolls its own list).
pub fn roll(rng: &mut CombatRng, list: &[&'static str]) -> &'static str {
    let item = pick(rng, list);
    if item == PKA_MOD {
        pick(rng, PKA_MOD_LOOT)
    } else {
        item
    }
}

/// `PopulateContents` of a tendril or puzzle chest, on its first opening (from
/// `crate::closets::open`). True when the chest was one of them.
pub fn populate(world: &mut World, chest: Entity) -> bool {
    let Some(list) = world
        .get::<ss13_mapload::DmObj>(chest)
        .and_then(|o| chest_loot(&o.path))
    else {
        return false;
    };
    let path = roll(&mut world.get_resource_or_init::<CombatRng>(), list);
    if let Some(item) = crate::construction::spawn_detached(world, path) {
        crate::storage::put_inside(world, item, chest);
    }
    true
}

/// A live tendril: the floors it turned (`infected_turfs`) and its regenerator's state.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Tendril {
    pub infected: Vec<(TilePos, String)>,
    /// Brute damage at the last check, and when it last went up.
    pub last_brute: f32,
    pub damaged_at: f64,
}

/// Necropolis floors waiting to turn back: when, where, into what.
#[derive(Resource, Default, Debug)]
pub struct PendingRestores(pub Vec<(f64, TilePos, String)>);

pub struct NecropolisPlugin;

impl Plugin for NecropolisPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingRestores>()
            .add_observer(attach)
            .add_systems(
                Update,
                (
                    regenerate.run_if(any_with_component::<Tendril>),
                    restore.run_if(|p: Res<PendingRestores>| !p.0.is_empty()),
                ),
            );
    }
}

/// `get_dist_euclidean` on one level.
fn euclid(a: TilePos, b: TilePos) -> f32 {
    (((a.x - b.x) * (a.x - b.x) + (a.y - b.y) * (a.y - b.y)) as f32).sqrt()
}

/// `tendril/Initialize`: the rock around it cleared, the asteroid floor infected.
fn attach(add: On<Add<BasicMob>>, mobs: Query<&BasicMob>, mut commands: Commands) {
    let Ok(basic) = mobs.get(add.entity) else {
        return;
    };
    if !is_under(&basic.def.path, TENDRIL) {
        return;
    }
    let mob = add.entity;
    commands.queue(move |world: &mut World| {
        if world.get_entity(mob).is_err() || world.get::<Tendril>(mob).is_some() {
            return;
        }
        let now = world.resource::<Time>().elapsed_secs_f64();
        let infected = world
            .get::<TilePos>(mob)
            .copied()
            .map(|pos| infect(world, pos))
            .unwrap_or_default();
        let last_brute = world.get::<Health>(mob).map_or(0.0, |h| h.brute);
        world.entity_mut(mob).insert(Tendril {
            infected,
            last_brute,
            damaged_at: now,
        });
    });
}

/// The turfs within [`RADIUS`] of `center`: rock scraped away, asteroid floor made necropolis
/// floor. Returns the changed floors with their old types.
pub fn infect(world: &mut World, center: TilePos) -> Vec<(TilePos, String)> {
    let mut infected = Vec::new();
    for dy in -4..=4 {
        for dx in -4..=4 {
            let pos = TilePos::new(center.x + dx, center.y + dy, center.z);
            let dist = euclid(pos, center);
            if dist > RADIUS {
                continue;
            }
            let Some(turf) = turf_at(world, pos) else {
                continue;
            };
            let mut path = path_of(world, turf).unwrap_or_default();
            if is_under(&path, MINERAL) {
                let Some(open) = scrape_away(world, pos, true) else {
                    continue;
                };
                path = path_of(world, open).unwrap_or_default();
            }
            if !is_under(&path, ASTEROID) {
                continue;
            }
            let chance = 100.0 / dist.max(1.0).sqrt();
            if !world.get_resource_or_init::<CombatRng>().chance(chance) {
                continue;
            }
            let Some(turf) = turf_at(world, pos) else {
                continue;
            };
            let baseturfs = baseturfs_of(world, turf);
            if change_turf(world, pos, NECROPOLIS_FLOOR, baseturfs, true).is_some() {
                infected.push((pos, path));
            }
        }
    }
    infected
}

/// `/datum/component/regenerator`: damage restarts the 30 s wait, then 20 brute a second.
fn regenerate(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let dt = world.resource::<Time>().delta_secs();
    let tendrils: Vec<Entity> = world
        .query_filtered::<Entity, With<Tendril>>()
        .iter(world)
        .collect();
    for mob in tendrils {
        if world.get::<Stat>(mob).is_some_and(|s| *s == Stat::Dead) {
            continue;
        }
        let Some(brute) = world.get::<Health>(mob).map(|h| h.brute) else {
            continue;
        };
        let mut t = world.get_mut::<Tendril>(mob).expect("queried");
        if brute > t.last_brute {
            t.damaged_at = now;
        }
        let heal = now - t.damaged_at >= REGEN_DELAY && brute > 0.0;
        let mut after = brute;
        if heal && let Some(mut h) = world.get_mut::<Health>(mob) {
            h.adjust(DamageType::Brute, -REGEN_PER_SECOND * dt);
            after = h.brute;
        }
        if let Some(mut t) = world.get_mut::<Tendril>(mob) {
            t.last_brute = after;
        }
    }
}

/// `tendril/death` with its `death_drops`: the sound, the chest, the floors turning back.
/// Called from the basic mobs' death (`crate::mobs::lavaland::on_death`).
pub fn on_death(world: &mut World, mob: Entity, pos: TilePos) {
    let Some(tendril) = world.get::<Tendril>(mob).cloned() else {
        return;
    };
    // `playsound(turf, 'tendril_destroyed.ogg', 200, FALSE, 50, TRUE, TRUE)`.
    let mut sound = PlaySound::new(pos, DESTROYED_SOUND, 200.0, false);
    sound.extrarange = 50.0;
    sound.falloff_exponent = 1.0;
    playsound(world, sound);
    crate::construction::spawn_obj(world, TENDRIL_CHEST, pos);
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut pending = world.resource_mut::<PendingRestores>();
    for (at, old) in tendril.infected {
        let dist = euclid(at, pos);
        if at.z != pos.z || dist > RADIUS {
            continue;
        }
        pending.0.push((now + restore_delay(dist), at, old));
    }
}

/// `round(5 - dist, 0.5)` seconds.
pub fn restore_delay(dist: f32) -> f64 {
    (f64::from(5.0 - dist) * 2.0).round() / 2.0
}

/// The timers of [`on_death`]: a floor still necropolis floor turns back.
fn restore(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let due: Vec<(TilePos, String)> = {
        let mut pending = world.resource_mut::<PendingRestores>();
        let (due, wait): (Vec<_>, Vec<_>) = pending.0.drain(..).partition(|(at, ..)| *at <= now);
        pending.0 = wait;
        due.into_iter().map(|(_, pos, old)| (pos, old)).collect()
    };
    for (pos, old) in due {
        let Some(turf) = turf_at(world, pos) else {
            continue;
        };
        if !path_of(world, turf).is_some_and(|p| is_under(&p, NECROPOLIS_FLOOR)) {
            continue;
        }
        let baseturfs = baseturfs_of(world, turf);
        change_turf(world, pos, &old, baseturfs, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chests_pick_their_loot_list() {
        assert_eq!(chest_loot(TENDRIL_CHEST), Some(MINING_LOOT));
        assert_eq!(chest_loot(DEMONIC_CHEST), Some(DEMONIC_LOOT));
        assert_eq!(chest_loot(PUZZLE_CHEST), Some(PUZZLE_LOOT));
        assert_eq!(
            chest_loot("/obj/structure/closet/crate/necropolis/dragon"),
            None
        );
        assert_eq!(chest_loot("/obj/structure/closet/crate"), None);
    }

    #[test]
    fn a_roll_never_yields_the_modkit_spawner() {
        let mut rng = CombatRng(7);
        let mut modkits = 0;
        for _ in 0..500 {
            let item = roll(&mut rng, MINING_LOOT);
            assert_ne!(item, PKA_MOD);
            assert!(MINING_LOOT.contains(&item) || PKA_MOD_LOOT.contains(&item));
            modkits += usize::from(PKA_MOD_LOOT.contains(&item));
        }
        assert!(modkits > 0, "the modkit entry rolls from pka_mod");
    }

    #[test]
    fn floors_turn_back_from_the_outside_in() {
        assert_eq!(restore_delay(0.0), 5.0);
        assert_eq!(restore_delay(1.0), 4.0);
        assert_eq!(restore_delay(1.4142), 3.5);
        assert_eq!(restore_delay(4.4721), 0.5);
        assert!(runtime_paths().all(|p| p != PKA_MOD));
    }
}
