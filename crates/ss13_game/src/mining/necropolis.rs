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
//!   one of three relics, a boss chest its boss's loot (unless the dying boss filled it,
//!   `crate::mobs::megafauna`).
//! - It fights (`tendril.bt.json`, `tendril_actions.dm`): the nearest hostile in 9 tiles is its
//!   target; within 7 it raises two waves of cross spikes (15 each, a checkerboard under 30%
//!   health) or lashes tentacles along the four cardinals and then the diagonals (10 + 10 and
//!   reeled in), farther away it sends a spike trail after the target (15, wide under 60%);
//!   hurt with a hostile within 2 it stabs all around (15, every 4 s). Its heart beats faster
//!   the more hurt it is while it has a target.
// shortcut: no GPS signal, achievements, emissive overlay, regeneration outline or
// `block_mining_mob_respawns` (the cave population does not respawn here), no reinforcement
// call; add each with its mechanic.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_shared::{Dir, TilePos};
use ss13_world::SpatialIndex;

use crate::combat::CombatRng;
use crate::construction::{baseturfs_of, change_turf, path_of, scrape_away, turf_at};
use crate::health::{DamageType, Health, Stat};
use crate::looping_sound::defs::TENDRIL_HEARTBEAT;
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

/// `PopulateContents` of a necropolis chest on its first opening (from `crate::closets::open`):
/// a tendril or puzzle chest rolls its list, a boss chest not filled by its dying boss gets the
/// boss's loot. True when the chest was one of them.
pub fn populate(world: &mut World, chest: Entity) -> bool {
    let Some(path) = world
        .get::<ss13_mapload::DmObj>(chest)
        .map(|o| o.path.clone())
    else {
        return false;
    };
    let filled = world
        .get::<crate::storage::Stored>(chest)
        .is_some_and(|s| !s.items().is_empty());
    if is_under(&path, NECROPOLIS_CHEST)
        && let Some(loot) = crate::mobs::megafauna::chest_loot(world, &path)
    {
        // A dying boss filled its chest already (`crate::mobs::megafauna::on_death`).
        if !filled {
            for item in loot {
                if let Some(item) = crate::construction::spawn_detached(world, item) {
                    crate::storage::put_inside(world, item, chest);
                }
            }
        }
        return true;
    }
    let Some(list) = chest_loot(&path) else {
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
                    combat.run_if(any_with_component::<TendrilCombat>),
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
        world.entity_mut(mob).insert(TendrilCombat::default());
        crate::looping_sound::start(world, mob, &TENDRIL_HEARTBEAT);
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
    crate::looping_sound::cancel(world, mob, TENDRIL_HEARTBEAT.path, true);
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

// --- The tendril's fight ---------------------------------------------------------------------
// The `tendril` AI tree (`tendril.bt.json`) with its four abilities, run here as one state
// machine: the tree only picks a target and fires whichever ability is ready, and the abilities
// are timelines of telegraphs and hits (`SLEEP_CHECK_DEATH`).
// shortcut: no telegraph or spike visuals (the hits land on tg's timings), the tentacles hit at
// once instead of flying, a snatched mob is put next to the tendril instead of thrown
// (`throw_at(gentle)`), cross spikes ignore line of sight (`oview`), no `call_reinforcements`.

/// `BB_AGGRO_RANGE` and `target_loss_distance`.
const AGGRO_RANGE: u32 = 9;
/// `is_at_distance` `maximum_distance`: spikes or lash within it, the chaser beyond.
const CLOSE_RANGE: u32 = 7;
/// `BASIC_MOB_FIND_TARGET_RATE`.
const TARGET_LOOK: f64 = 1.0;
const LASH_COOLDOWN: f64 = 12.0;
const CHASER_COOLDOWN: f64 = 12.0;
const SPIKES_COOLDOWN: f64 = 12.0;
const STAB_COOLDOWN: f64 = 4.0;
/// `tentacle_lash` `range`, `damage` and `snatch_damage`; `stab` `range` and `damage`.
const LASH_RANGE: i32 = 7;
const LASH_DAMAGE: f32 = 10.0;
const SNATCH_DAMAGE: f32 = 10.0;
const STAB_RANGE: i32 = 2;
const STAB_DAMAGE: f32 = 15.0;
/// `emerging_ground_spike/tendril` `impale_damage`, `single` `harm_delay`.
const SPIKE_DAMAGE: f32 = 15.0;
const SPIKE_HARM_DELAY: f64 = 0.3;
/// `tendril_chaser` `duration`, `move_speed` (deciseconds a step), `wide_chaser_threshold`.
const CHASER_DURATION: f64 = 10.0;
const CHASER_STEP: f64 = 0.5;
const WIDE_CHASER_THRESHOLD: f32 = 0.6;
/// `tendril_cross_spikes` `spike_range`, `health_threshold`.
const SPIKE_RANGE: i32 = 7;
const SPIKES_HEALTH_THRESHOLD: f32 = 0.3;
/// `revenge_ability` `max_range`.
const REVENGE_RANGE: u32 = 2;
/// `HEARTBEAT_NORMAL`, `HEARTBEAT_FAST`, `HEARTBEAT_FRANTIC`.
const BEAT_NORMAL: f64 = 1.8;
const BEAT_FAST: f64 = 1.0;
const BEAT_FRANTIC: f64 = 0.4;
const CHASER_SOUND: &str = "sound/effects/magic/demon_attack1.ogg";
const SPIKE_HIT_SOUND: &str = "sound/items/weapons/slice.ogg";
const TENTACLE_HIT_SOUND: &str = "sound/effects/wounds/pierce1.ogg";

const DIAGONALS: [Dir; 4] = [
    Dir::NorthEast,
    Dir::NorthWest,
    Dir::SouthEast,
    Dir::SouthWest,
];
const ALL_DIRS: [Dir; 8] = [
    Dir::North,
    Dir::South,
    Dir::East,
    Dir::West,
    Dir::NorthEast,
    Dir::NorthWest,
    Dir::SouthEast,
    Dir::SouthWest,
];

/// A hit an ability lands later.
#[derive(Clone, Debug, PartialEq)]
pub enum Strike {
    /// Tentacles along these directions (`tendril_lash`).
    Lash(Vec<Dir>),
    /// Spikes coming out of these tiles (`tendril_cross_spikes`).
    Spikes(Vec<TilePos>),
    /// Short tentacles all around (`tendril_melee`).
    Stab,
}

/// A spike trail after the target (`effect_trail/tendril_chaser`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chaser {
    pub pos: TilePos,
    pub target: Entity,
    pub next_step: f64,
    pub until: f64,
    pub wide: bool,
}

/// The tendril's AI state: its target, the abilities' cooldowns, what is still to land.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct TendrilCombat {
    pub target: Option<Entity>,
    pub next_look: f64,
    /// `disable_cooldown_actions` until then.
    pub busy_until: f64,
    pub lash_ready: f64,
    pub chaser_ready: f64,
    pub spikes_ready: f64,
    pub stab_ready: f64,
    pub strikes: Vec<(f64, Strike)>,
    pub chasers: Vec<Chaser>,
    /// Brute damage at the last look (`revenge_ability` reacts to being hurt).
    pub seen_brute: f32,
}

fn hostile(world: &World, tendril: Entity, mob: Entity) -> bool {
    mob != tendril
        && world.get::<Tendril>(mob).is_none()
        && world.get::<Stat>(mob).is_some_and(|s| *s != Stat::Dead)
        && world.get::<crate::closets::InCloset>(mob).is_none()
        && !crate::mobs::same_faction(world, tendril, mob)
}

/// Living mobs on `pos` a tendril's attacks hurt (`damage_blacklist_typecache`: not tendrils).
fn victims_on(world: &World, pos: TilePos) -> Vec<Entity> {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| world.get::<Health>(e).is_some() && world.get::<Tendril>(e).is_none())
        .filter(|&e| world.get::<Stat>(e).is_some_and(|s| *s != Stat::Dead))
        .filter(|&e| world.get::<crate::closets::InCloset>(e).is_none())
        .collect()
}

/// `istype(turf, /turf/open)` and nothing dense but mobs (`is_blocked_turf(exclude_mobs)`).
fn open_tile(world: &World, pos: TilePos) -> bool {
    let index = world.resource::<SpatialIndex>();
    if !index.in_bounds(pos) {
        return false;
    }
    let open = turf_at(world, pos)
        .and_then(|t| path_of(world, t))
        .is_some_and(|p| is_under(&p, "/turf/open"));
    open && index
        .dense_on_tile(pos)
        .iter()
        .all(|&e| world.get::<Health>(e).is_some())
}

fn health_ratio(world: &World, mob: Entity) -> f32 {
    world
        .get::<Health>(mob)
        .map_or(1.0, |h| (h.health() / h.max.max(1.0)).clamp(0.0, 1.0))
}

/// `update_combat_targets`: the nearest hostile in [`AGGRO_RANGE`].
fn find_target(world: &mut World, tendril: Entity, at: TilePos) -> Option<Entity> {
    // Cost: once a second per tendril, over the living mobs (a few hundred at most).
    let mobs: Vec<(Entity, TilePos)> = world
        .query_filtered::<(Entity, &TilePos), (With<Health>, With<Stat>)>()
        .iter(world)
        .map(|(e, p)| (e, *p))
        .collect();
    mobs.into_iter()
        .filter_map(|(e, p)| {
            at.chebyshev(p)
                .filter(|&d| d <= AGGRO_RANGE)
                .map(|d| (d, e))
        })
        .filter(|&(_, e)| hostile(world, tendril, e))
        .min_by_key(|&(d, _)| d)
        .map(|(_, e)| e)
}

/// `tendril_cross_spikes/spawn_spikes`: the tiles of one wave around `at`.
pub fn spike_pattern(at: TilePos, inverse: bool, reduced: bool) -> Vec<TilePos> {
    let mut tiles = Vec::new();
    for dy in -SPIKE_RANGE..=SPIKE_RANGE {
        for dx in -SPIKE_RANGE..=SPIKE_RANGE {
            if dx == 0 && dy == 0 {
                continue;
            }
            // "big circle is a lie"
            if ((dx * dx + dy * dy) as f32).sqrt() > 9.5 {
                continue;
            }
            let (ax, ay) = (dx.abs(), dy.abs());
            let pos = TilePos::new(at.x + dx, at.y + dy, at.z);
            if reduced {
                if ax % 2 == (dy + i32::from(inverse)).abs() % 2 {
                    tiles.push(pos);
                }
                continue;
            }
            let (row_diff, column_diff) = if inverse { (ax, ay) } else { (ay, ax) };
            let row = (row_diff + 1) / 3;
            let on_row = row_diff - row * 3 == 0;
            let skip = if row % 2 == 0 {
                if on_row {
                    column_diff % 4 == 2
                } else {
                    column_diff % 4 != 0
                }
            } else if on_row {
                column_diff % 4 == 0
            } else {
                column_diff % 4 != 2
            };
            if !skip {
                tiles.push(pos);
            }
        }
    }
    tiles
}

fn hit(world: &mut World, victim: Entity, amount: f32) {
    crate::mobs::lavaland::hurt(world, victim, DamageType::Brute, amount, None);
}

fn sound(world: &mut World, at: TilePos, file: &str, vol: f32, vary: bool) {
    let mut s = PlaySound::new(at, file, vol, vary);
    s.pressure_affected = false;
    playsound(world, s);
}

/// The first victim along `dir` within `range` tiles, stopping at a blocked tile.
fn first_along(world: &World, from: TilePos, dir: Dir, range: i32) -> Option<(Entity, TilePos)> {
    let mut pos = from;
    for _ in 0..range {
        pos = pos.step(dir);
        if !open_tile(world, pos) {
            return None;
        }
        if let Some(&v) = victims_on(world, pos).first() {
            return Some((v, pos));
        }
    }
    None
}

/// One [`Strike`] landing.
fn land(world: &mut World, tendril: Entity, at: TilePos, strike: Strike) {
    match strike {
        Strike::Lash(dirs) => {
            for dir in dirs {
                let Some((victim, _)) = first_along(world, at, dir, LASH_RANGE) else {
                    continue;
                };
                hit(world, victim, LASH_DAMAGE);
                // `snatch_target`: "You're snatched", more damage, reeled in.
                if let Some(client) = world
                    .get::<crate::server::PlayerMob>(victim)
                    .map(|p| p.client)
                {
                    crate::tools::tell(
                        world,
                        client,
                        ss13_shared::chat::Span::Danger,
                        "Щупальца некрополисного тендрила хватают вас!",
                    );
                }
                hit(world, victim, SNATCH_DAMAGE);
                let next = at.step(dir);
                if open_tile(world, next) && world.get_entity(victim).is_ok() {
                    world.entity_mut(victim).insert(next);
                }
                sound(world, next, TENTACLE_HIT_SOUND, 50.0, false);
            }
        }
        Strike::Spikes(tiles) => {
            let mut hit_any = false;
            for pos in tiles {
                for victim in victims_on(world, pos) {
                    hit(world, victim, SPIKE_DAMAGE);
                    if !hit_any {
                        sound(world, pos, SPIKE_HIT_SOUND, 50.0, true);
                        hit_any = true;
                    }
                }
            }
        }
        Strike::Stab => {
            for dir in ALL_DIRS {
                if let Some((victim, pos)) = first_along(world, at, dir, STAB_RANGE) {
                    hit(world, victim, STAB_DAMAGE);
                    sound(world, pos, TENTACLE_HIT_SOUND, 50.0, false);
                }
            }
        }
    }
    let _ = tendril;
}

/// The chasers' steps: a spike on each tile they enter (and beside it when wide); the first
/// one that hurts ends the chaser (`chaser/harm_mob`).
fn step_chasers(world: &mut World, c: &mut TendrilCombat, now: f64) {
    let mut keep = Vec::new();
    for mut ch in std::mem::take(&mut c.chasers) {
        if now >= ch.until {
            continue;
        }
        if now < ch.next_step {
            keep.push(ch);
            continue;
        }
        ch.next_step = now + CHASER_STEP;
        let Some(&goal) = world
            .get_entity(ch.target)
            .ok()
            .and_then(|e| e.get::<TilePos>())
        else {
            continue;
        };
        if goal.z != ch.pos.z {
            continue;
        }
        let dx = (goal.x - ch.pos.x).signum();
        let dy = (goal.y - ch.pos.y).signum();
        let Some(dir) = ALL_DIRS.into_iter().find(|d| d.offset() == (dx, dy)) else {
            // On the target's tile: its spike is already there.
            keep.push(ch);
            continue;
        };
        ch.pos = ch.pos.step(dir);
        let mut tiles = Vec::new();
        if ch.wide {
            let back = dir.reverse().offset();
            for d in Dir::CARDINALS {
                let (ox, oy) = d.offset();
                // `spawn_dir & REVERSE_DIR(movement_dir)`: no spike behind.
                if (ox != 0 && ox == back.0) || (oy != 0 && oy == back.1) {
                    continue;
                }
                tiles.push(ch.pos.step(d));
            }
        } else {
            tiles.push(ch.pos);
        }
        let mut struck = false;
        for pos in tiles {
            if !open_tile(world, pos) {
                continue;
            }
            for victim in victims_on(world, pos) {
                hit(world, victim, SPIKE_DAMAGE);
                sound(world, pos, SPIKE_HIT_SOUND, 50.0, true);
                struck = true;
            }
        }
        if !struck {
            keep.push(ch);
        }
    }
    c.chasers = keep;
}

/// The `tendril` AI tree and abilities, and the heartbeat (`update_heartbeat`).
fn combat(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let tendrils: Vec<Entity> = world
        .query_filtered::<Entity, With<TendrilCombat>>()
        .iter(world)
        .collect();
    for mob in tendrils {
        if world.get::<Stat>(mob).is_none_or(|s| *s == Stat::Dead) {
            continue;
        }
        let (Some(mut c), Some(&at)) = (
            world.get::<TendrilCombat>(mob).cloned(),
            world.get::<TilePos>(mob),
        ) else {
            continue;
        };
        // Due strikes, in order.
        let (due, later): (Vec<_>, Vec<_>) = std::mem::take(&mut c.strikes)
            .into_iter()
            .partition(|(t, _)| *t <= now);
        c.strikes = later;
        for (_, strike) in due {
            land(world, mob, at, strike);
        }
        step_chasers(world, &mut c, now);
        // Target upkeep.
        let lost = c.target.is_some_and(|t| {
            world.get_entity(t).is_err()
                || !hostile(world, mob, t)
                || world
                    .get::<TilePos>(t)
                    .and_then(|p| at.chebyshev(*p))
                    .is_none_or(|d| d > AGGRO_RANGE)
        });
        if lost {
            c.target = None;
        }
        if c.target.is_none() && now >= c.next_look {
            c.next_look = now + TARGET_LOOK;
            c.target = find_target(world, mob, at);
        }
        let ratio = health_ratio(world, mob);
        // `revenge_ability`: hurt, a hostile within 2 gets stabbed.
        let brute = world.get::<Health>(mob).map_or(0.0, |h| h.brute);
        let hurt = brute > c.seen_brute;
        c.seen_brute = brute;
        if now >= c.busy_until {
            if hurt
                && now >= c.stab_ready
                && find_target(world, mob, at).is_some_and(|t| {
                    world
                        .get::<TilePos>(t)
                        .and_then(|p| at.chebyshev(*p))
                        .is_some_and(|d| d <= REVENGE_RANGE)
                })
            {
                // `tendril_melee/Activate(warning = TRUE)`: 0.5 s warning, 0.5 s after.
                c.strikes.push((now + 0.5, Strike::Stab));
                c.busy_until = now + 1.0;
                c.stab_ready = c.busy_until + STAB_COOLDOWN;
            } else if let Some(target) = c.target {
                let dist = world
                    .get::<TilePos>(target)
                    .and_then(|p| at.chebyshev(*p))
                    .unwrap_or(u32::MAX);
                if dist <= CLOSE_RANGE && now >= c.spikes_ready {
                    // Cross spikes: wave, 0.4 s, the inverse wave; chasers die with it and
                    // the chaser's cooldown resets after (`on_ability_started/finished`).
                    let reduced = ratio <= SPIKES_HEALTH_THRESHOLD;
                    c.chasers.clear();
                    c.strikes.push((
                        now + 1.0 + SPIKE_HARM_DELAY,
                        Strike::Spikes(spike_pattern(at, false, reduced)),
                    ));
                    c.strikes.push((
                        now + 3.2 + SPIKE_HARM_DELAY,
                        Strike::Spikes(spike_pattern(at, true, reduced)),
                    ));
                    c.busy_until = now + 4.0;
                    c.spikes_ready = c.busy_until + SPIKES_COOLDOWN;
                    c.chaser_ready = c.busy_until;
                } else if dist <= CLOSE_RANGE && now >= c.lash_ready {
                    c.strikes
                        .push((now + 1.2, Strike::Lash(Dir::CARDINALS.to_vec())));
                    c.strikes
                        .push((now + 4.2, Strike::Lash(DIAGONALS.to_vec())));
                    c.busy_until = now + 6.0;
                    c.lash_ready = now + 5.4 + LASH_COOLDOWN;
                } else if dist > CLOSE_RANGE && dist <= AGGRO_RANGE && now >= c.chaser_ready {
                    c.chasers.push(Chaser {
                        pos: at,
                        target,
                        next_step: now,
                        until: now + CHASER_DURATION,
                        wide: ratio <= WIDE_CHASER_THRESHOLD,
                    });
                    sound(world, at, CHASER_SOUND, 100.0, true);
                    c.busy_until = now + 0.6;
                    c.chaser_ready = c.busy_until + CHASER_COOLDOWN;
                }
            }
        }
        // `update_heartbeat`.
        crate::looping_sound::set_mid_length(
            world,
            mob,
            TENDRIL_HEARTBEAT.path,
            beat_rate(c.target.is_some(), ratio),
        );
        if let Ok(mut e) = world.get_entity_mut(mob) {
            e.insert(c);
        }
    }
}

/// `update_heartbeat`: normal without a target, faster the more hurt with one.
pub fn beat_rate(has_target: bool, health_ratio: f32) -> f64 {
    if !has_target {
        return BEAT_NORMAL;
    }
    let rate = BEAT_FRANTIC + f64::from(health_ratio) * (BEAT_FAST - BEAT_FRANTIC);
    (rate / 0.05).round() * 0.05
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
        assert_eq!(restore_delay(std::f32::consts::SQRT_2), 3.5);
        assert_eq!(restore_delay(4.4721), 0.5);
        assert!(runtime_paths().all(|p| p != PKA_MOD));
    }

    #[test]
    fn heartbeat_quickens_as_the_tendril_is_hurt() {
        assert_eq!(beat_rate(false, 0.1), 1.8);
        assert!((beat_rate(true, 1.0) - 1.0).abs() < 1e-9);
        assert!((beat_rate(true, 0.0) - 0.4).abs() < 1e-9);
        assert!((beat_rate(true, 0.5) - 0.7).abs() < 1e-9);
    }

    #[test]
    fn cross_spike_waves_leave_gaps_and_cover_each_other() {
        let at = TilePos::new(20, 20, 1);
        let a = spike_pattern(at, false, false);
        let b = spike_pattern(at, true, false);
        assert!(!a.contains(&at) && !b.contains(&at));
        assert!(!a.is_empty() && a.len() < 200, "{}", a.len());
        assert_ne!(a, b);
        // Reduced: a checkerboard, the inverse wave its complement.
        let r = spike_pattern(at, false, true);
        let ri = spike_pattern(at, true, true);
        assert!(r.iter().all(|p| !ri.contains(p)));
        assert!(r.contains(&TilePos::new(21, 21, 1)) || ri.contains(&TilePos::new(21, 21, 1)));
    }
}
