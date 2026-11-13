//! The lavaland mobs' abilities (tg `/datum/action/cooldown/mob_cooldown/*` of
//! `code/modules/mob/living/basic/lavaland/`), used by their AI leaves ([`super::ai`]).
//!
//! - Goliath `goliath_tentacles` (12 s, 1 s shorter each time it is hurt, never under 1 s): a
//!   tentacle under the target and each open cardinal neighbour (`/obj/effect/goliath_tentacle`):
//!   0.7 s rising, 0.3 s wiggling, then it grabs every living mob on its tile that is not of the
//!   goliath's faction (10-15 brute to a leg) and holds them up to 10 s, else it retracts. The
//!   goliath shows `goliath_preattack` from 2 s before the tentacles are ready. The immortal
//!   goliath churns the earth: every `Life` (2 s) each open tile within 4 gets a tentacle at 10%.
//! - Legion `skull_launcher` (4 s, 7 tiles): a skull lands on the target's tile (a free neighbour
//!   when it is blocked) after 0.5 s with the legion's faction and target; brood skulls and
//!   hivelord brood die after 10 s. A skull's bite on a human out cold (`IS_UNCONSCIOUS`)
//!   infests it: a new legion with the corpse inside (dead, its damage healed), named after it;
//!   the corpse falls out when the legion dies or is gone.
//! - Hivelord `hivelord_spawn` (2 s): a brood worm on a free tile next to it towards the target.
//! - Brimdemon `brimbeam` (3 s): only along a cardinal line; 1 s charging, then a beam of up to
//!   10 tiles (stopped by opaque tiles) burns everyone in it for 25 (laser armour) and stays 2 s;
//!   the demon holds still throughout. A dead brimdemon bursts after 1.9 s: 20 burn within a tile
//!   (bomb armour).
//! - Lobstrosity `basic_charge/lobster` (6 s): 1.5 s wind-up, then a rush of up to 8 tiles (one
//!   per 0.05 s) at the target's tile and 2 past it; a living mob in the way is knocked down for
//!   2.5 s and bitten at once, anything else stuns the lobster 0.6 s.
//! - Bileworm `bileworm_spew` (3 s): five globs 0.15 s apart, the first at the target's tile,
//!   the rest around it; each lands after its flight (up to 5 tiles a second) and burns
//!   everyone there for 20 (melee armour, 40 penetration).
// shortcut: no goliath `tentacle_burst`/`tentacle_grasp` revenge abilities, no legcuffs (a
// grabbed victim is immobilized for the legcuff's 6 s `breakouttime`, the time a victim
// resisting at once would be held), no skull healing friendly mobs or legion tumours, no beam
// abort when the brimdemon is hit from the front, no damage to objects and walls from rushes,
// beams or bile, no landing/despawn animations or ability sounds.

use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmTurf, Opaque};
use ss13_net::server::Networked;
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::{Appearance, Dir, Facing, Layer, MouseOpacity, TilePos};
use ss13_world::SpatialIndex;

use crate::combat::{CombatRng, NextMove};
use crate::health::{DEFAULT_MESSAGE_RANGE, DamageType, Health, Stat};
use crate::status_effects::Incap;
use crate::zone::BodyZone;

const MONSTERS_DMI: &str = "icons/mob/simple/lavaland/lavaland_monsters.dmi";
const TELEGRAPH_DMI: &str = "icons/mob/telegraphing/telegraph.dmi";
/// `BELOW_MOB_LAYER`, `ABOVE_MOB_LAYER`, `GAME_PLANE`.
const BELOW_MOB_LAYER: f32 = 3.7;
const ABOVE_MOB_LAYER: f32 = 4.1;
const GAME_PLANE: i16 = -4;

/// `goliath_tentacles`: `cooldown_time`, `max_range`; the tentacle's stage timers,
/// `grapple_time`, damage; the legcuff's `breakouttime`.
const TENTACLES_COOLDOWN: f64 = 12.0;
pub const TENTACLES_RANGE: u32 = 7;
const TENTACLE_RISE: f64 = 0.7;
const TENTACLE_WIGGLE: f64 = 0.3;
const TENTACLE_GRAPPLE: f64 = 10.0;
const TENTACLE_RETRACT: f64 = 0.7;
const TENTACLE_DAMAGE: (u32, u32) = (10, 15);
const TENTACLE_BREAKOUT: f32 = 6.0;
/// The immortal goliath's `orange(4)` and `prob(10)` per `Life`.
const CHURN_RANGE: u32 = 4;
const CHURN_CHANCE: f32 = 10.0;
const LIFE_SECS: f64 = 2.0;
/// `skull_launcher`: `cooldown_time`, `max_range`; `legion_skull_land` `duration`.
const SKULL_COOLDOWN: f64 = 4.0;
pub const SKULL_RANGE: u32 = 7;
const SKULL_FLIGHT: f64 = 0.5;
const SKULL_INDICATOR: f64 = 0.75;
/// `legion_brood` and `hivelord_brood`: `addtimer(death, 10 SECONDS)`.
const BROOD_LIFETIME: f64 = 10.0;
/// `hivelord_spawn` `cooldown_time`, and the spawn animation's length.
const HIVELORD_COOLDOWN: f64 = 2.0;
const HIVELORD_FLIGHT: f64 = 0.3;
/// `brimbeam`: `cooldown_time`, `beam_range`, `beam_duration`, `charge_duration`; the hit.
const BEAM_COOLDOWN: f64 = 3.0;
const BEAM_RANGE: i32 = 10;
const BEAM_DURATION: f64 = 2.0;
const BEAM_CHARGE: f64 = 1.0;
const BEAM_DAMAGE: f32 = 25.0;
/// `brim_burst`: `duration` minus a decisecond, damage.
const BURST_DELAY: f64 = 1.8;
const BURST_DAMAGE: f32 = 20.0;
/// `basic_charge/lobster`: `cooldown_time`, `charge_delay`, `charge_distance`, `charge_past`,
/// `charge_speed`, `knockdown_duration`, `recoil_duration`.
const RUSH_COOLDOWN: f64 = 6.0;
const RUSH_DELAY: f64 = 1.5;
const RUSH_DISTANCE: u32 = 8;
const RUSH_PAST: i32 = 2;
const RUSH_STEP: f64 = 0.05;
const RUSH_KNOCKDOWN: f32 = 2.5;
const RUSH_RECOIL: f32 = 0.6;
/// `bileworm_spew`: `cooldown_time`, `additional_shots`, `shot_delay`; `bileworm_acid`
/// `damage`, `speed`, `max_range`; the armour check's penetration.
const SPEW_COOLDOWN: f64 = 3.0;
const SPEW_EXTRA_SHOTS: u32 = 4;
const SPEW_DELAY: f64 = 0.15;
const BILE_DAMAGE: f32 = 20.0;
const BILE_SPEED: f64 = 5.0;
const BILE_MAX_RANGE: f64 = 10.0;
const BILE_PENETRATION: f32 = 40.0;

/// A goliath's `goliath_tentacles`.
#[derive(Component, Clone, Debug)]
pub struct Tentacles {
    pub cooldown: f64,
    pub ready_at: f64,
    /// `tentacle_warning_state`.
    warning: String,
    /// Health at the last look, to see it hurt (`apply_damage`).
    last_health: f32,
    /// `/mob/living/basic/mining/goliath/ancient/immortal`, its next `Life`.
    churn: Option<f64>,
}

/// `/obj/effect/goliath_tentacle`.
#[derive(Component, Clone, Debug)]
pub struct Tentacle {
    owner: Option<Entity>,
    stage: TentacleStage,
    next: f64,
    held: Vec<Entity>,
    /// `min_damage`/`max_damage` and `grapple_time` (the broodmother's are its own).
    damage: (u32, u32),
    grapple: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TentacleStage {
    Rising,
    Wiggling,
    Holding,
    Retracting,
}

/// Held by a tentacle (tg `get_item_by_slot(ITEM_SLOT_LEGCUFFED)`).
#[derive(Component, Clone, Copy, Debug)]
pub struct Grabbed(pub Entity);

/// A legion's `skull_launcher` or a hivelord's `hivelord_spawn`.
#[derive(Component, Clone, Copy, Debug)]
pub struct BroodLauncher {
    pub ready_at: f64,
    hivelord: bool,
}

/// A brood skull or worm flying to where it lands.
#[derive(Component, Clone, Copy, Debug)]
struct Landing {
    at: f64,
    spawn: &'static str,
    creator: Entity,
    target: Option<Entity>,
}

/// A visual gone at `until`.
#[derive(Component, Clone, Copy, Debug)]
struct Fleeting {
    until: f64,
}

/// A short-lived brood mob: it dies at `dies_at`.
#[derive(Component, Clone, Copy, Debug)]
pub struct Brood {
    pub dies_at: f64,
}

/// The corpse inside a legion (`stored_mob`).
#[derive(Component, Clone, Copy, Debug)]
pub struct StoredCorpse(pub Entity);

/// A brimdemon's `brimbeam`.
#[derive(Component, Clone, Debug)]
pub struct Brimbeam {
    pub ready_at: f64,
    phase: BeamPhase,
}

#[derive(Clone, Debug, PartialEq)]
enum BeamPhase {
    Idle,
    Charging { until: f64, dir: Dir },
    Firing { until: f64, beams: Vec<Entity> },
}

/// A dead brimdemon about to burst (`brim_burst`).
#[derive(Component, Clone, Copy, Debug)]
struct BrimBurst {
    at: f64,
}

/// A lobstrosity's `basic_charge/lobster`.
#[derive(Component, Clone, Debug)]
pub struct Rush {
    pub ready_at: f64,
    phase: RushPhase,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum RushPhase {
    Idle,
    WindUp { until: f64, to: TilePos },
    Dashing { next: f64, to: TilePos, left: u32 },
}

/// A bileworm's `bileworm_spew`, and the shots still to come.
#[derive(Component, Clone, Debug)]
pub struct Spew {
    pub ready_at: f64,
    target: Option<Entity>,
    next: f64,
    left: u32,
    dirs: Vec<Option<Dir>>,
    aimed_at: Option<TilePos>,
}

/// `/obj/effect/bileworm_acid` in flight to `pos`.
#[derive(Component, Clone, Copy, Debug)]
struct Glob {
    lands: f64,
    firer: Entity,
}

pub(super) fn build(app: &mut App) {
    app.add_systems(
        Update,
        (
            goliaths, tentacles, landings, broods, beams, bursts, rushes, spews, globs, fleeting,
        )
            .before(crate::health::HealthSystems),
    )
    .add_observer(release_corpse);
}

pub(super) use crate::tools::now;

/// The abilities a mob of type `path` starts with.
pub(super) fn attach(world: &mut World, mob: Entity, path: &str) {
    let now = now(world);
    let under = |root: &str| is_under(path, root);
    let mut e = world.entity_mut(mob);
    if under(super::GOLIATH) {
        let warning = if under("/mob/living/basic/mining/goliath/ancient") {
            "ancient_goliath_preattack"
        } else {
            "goliath_preattack"
        };
        let health = e.get::<Health>().map_or(0.0, Health::health);
        e.insert(Tentacles {
            cooldown: TENTACLES_COOLDOWN,
            // `tentacles_ready()` at once: the ability starts off cooldown.
            ready_at: 0.0,
            warning: warning.to_owned(),
            last_health: health,
            churn: under(super::IMMORTAL_GOLIATH).then_some(now + LIFE_SECS),
        });
    } else if under(super::LEGION) {
        e.insert(BroodLauncher {
            ready_at: 0.0,
            hivelord: false,
        });
    } else if under(super::HIVELORD) {
        e.insert(BroodLauncher {
            ready_at: 0.0,
            hivelord: true,
        });
    } else if under(super::BRIMDEMON) {
        e.insert(Brimbeam {
            ready_at: 0.0,
            phase: BeamPhase::Idle,
        });
    } else if under(super::LOBSTROSITY) {
        e.insert(Rush {
            ready_at: 0.0,
            phase: RushPhase::Idle,
        });
    } else if under(super::BILEWORM) {
        e.insert(Spew {
            ready_at: 0.0,
            target: None,
            next: 0.0,
            left: 0,
            dirs: Vec::new(),
            aimed_at: None,
        });
    } else if under(super::LEGION_BROOD) || under(super::HIVELORD_BROOD) {
        e.insert(Brood {
            dies_at: now + BROOD_LIFETIME,
        });
    }
}

pub(super) fn on_death(world: &mut World, mob: Entity, path: &str) {
    if is_under(path, super::BRIMDEMON) {
        extinguish(world, mob);
        let at = now(world) + BURST_DELAY;
        world.entity_mut(mob).insert(BrimBurst { at });
    }
}

/// An ability that holds the mob still is running.
pub(super) fn channeling(world: &World, mob: Entity) -> bool {
    world
        .get::<Brimbeam>(mob)
        .is_some_and(|b| b.phase != BeamPhase::Idle)
        || world
            .get::<Rush>(mob)
            .is_some_and(|r| r.phase != RushPhase::Idle)
}

pub(super) fn alive(world: &World, mob: Entity) -> bool {
    world.get::<Stat>(mob) == Some(&Stat::Conscious)
}

/// A visual of `state` (with `Marker`) on `pos`; without the icon only the marker (the logic
/// still runs on asset-less fixtures).
fn spawn_visual(
    world: &mut World,
    pos: TilePos,
    icon: &str,
    state: &str,
    layer: f32,
    marker: impl Bundle,
) -> Entity {
    let look = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(icon, state))
        .map(|icon_state| Appearance {
            icon_state,
            layer: Layer(layer),
            plane: GAME_PLANE,
            color: None,
            pixel_offset: [0, 0],
            rotation: 0,
            overlays: vec![],
            mouse_opacity: MouseOpacity::Transparent,
        });
    let mut e = world.spawn((Name::new(state.to_owned()), pos, marker));
    if let Some(look) = look {
        e.insert((look, Networked));
    }
    e.id()
}

fn set_state(world: &mut World, e: Entity, icon: &str, state: &str) {
    let Some(id) = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(icon, state))
    else {
        return;
    };
    if let Some(mut look) = world.get_mut::<Appearance>(e)
        && look.icon_state != id
    {
        look.icon_state = id;
    }
}

pub(super) fn fleeting_visual(world: &mut World, pos: TilePos, icon: &str, state: &str, secs: f64) {
    let until = now(world) + secs;
    spawn_visual(world, pos, icon, state, BELOW_MOB_LAYER, Fleeting { until });
}

fn fleeting(world: &mut World) {
    let now = now(world);
    let gone: Vec<Entity> = world
        .query::<(Entity, &Fleeting)>()
        .iter(world)
        .filter(|(_, f)| now >= f.until)
        .map(|(e, _)| e)
        .collect();
    for e in gone {
        world.despawn(e);
    }
}

pub(super) fn visible(world: &mut World, at: TilePos, span: Span, text: String) {
    crate::security::show(
        world,
        at,
        DEFAULT_MESSAGE_RANGE,
        ChatChannel::Combat,
        (span, text),
        &[],
    );
}

pub(super) fn tell(world: &mut World, mob: Entity, span: Span, text: String) {
    if let Some(client) = world.get::<crate::server::PlayerMob>(mob).map(|p| p.client) {
        crate::tools::tell(world, client, span, text);
    }
}

pub(super) fn name(world: &World, e: Entity) -> String {
    crate::security::name_of(world, e)
}

pub(super) use ss13_chat::capitalize;

/// Living mobs on `pos` other than `except`.
pub(super) fn living_on(world: &World, pos: TilePos, except: Option<Entity>) -> Vec<Entity> {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| Some(e) != except && world.get::<Health>(e).is_some())
        .filter(|&e| world.get::<crate::closets::InCloset>(e).is_none())
        .collect()
}

/// `apply_damage(amount, kind, blocked = run_armor_check(zone, flag, penetration))`.
fn armoured_hit(
    world: &mut World,
    mob: Entity,
    kind: DamageType,
    amount: f32,
    flag: &str,
    pen: f32,
) {
    let zone = BodyZone::Chest;
    let worn = crate::armor::getarmor(world, mob, zone, flag);
    let (armor, _) = crate::armor::run_armor_check(worn, pen, false, None, None);
    if armor >= 100.0 {
        return;
    }
    let damage = crate::armor::blocked_damage(amount, armor);
    super::hurt(world, mob, kind, damage, Some(zone));
}

/// `isopenturf` and not groundless (no tentacle over space or a chasm).
pub(super) fn open_ground(world: &World, pos: TilePos) -> bool {
    // A turf type without the vars has no `DmTurf`: an open floor.
    crate::construction::turf_at(world, pos).is_some_and(|t| {
        world
            .get::<DmTurf>(t)
            .is_none_or(|t| !t.closed && !t.groundless)
    })
}

// ---------------------------------------------------------------------------------------------
// Goliath

/// `goliath_tentacles` ready.
pub fn tentacles_ready(world: &World, goliath: Entity, now: f64) -> bool {
    world
        .get::<Tentacles>(goliath)
        .is_some_and(|t| now >= t.ready_at)
}

/// `goliath_tentacles/Activate` at `target`'s tile: one under it, one on each open cardinal
/// neighbour.
pub fn unleash_tentacles(world: &mut World, goliath: Entity, target: Entity) -> bool {
    let now = now(world);
    let (Some(&from), Some(&at)) = (world.get::<TilePos>(goliath), world.get::<TilePos>(target))
    else {
        return false;
    };
    if from.chebyshev(at).is_none_or(|d| d > TENTACLES_RANGE)
        || !tentacles_ready(world, goliath, now)
    {
        return false;
    }
    spawn_tentacle(world, at, Some(goliath));
    for dir in Dir::CARDINALS {
        spawn_tentacle(world, at.step(dir), None);
    }
    let text = format!(
        "{} запускает свои щупальца под {}!",
        capitalize(&name(world, goliath)),
        name(world, target)
    );
    visible(world, from, Span::Warning, text);
    if let Some(mut t) = world.get_mut::<Tentacles>(goliath) {
        t.ready_at = now + t.cooldown;
    }
    let living = world
        .get::<crate::mobs::BasicMob>(goliath)
        .map(|b| (b.def.icon.clone(), b.icon_living.clone()));
    if let Some((icon, state)) = living {
        crate::mobs::set_icon_state(world, goliath, &icon, &state);
    }
    true
}

/// `new /obj/effect/goliath_tentacle(pos, owner)`: none off open ground or on another.
fn spawn_tentacle(world: &mut World, pos: TilePos, owner: Option<Entity>) {
    spawn_tentacle_with(world, pos, owner, TENTACLE_DAMAGE, TENTACLE_GRAPPLE);
}

/// A tentacle subtype with its `min_damage`/`max_damage` and `grapple_time`
/// (`goliath_tentacle/broodmother`).
pub(crate) fn spawn_tentacle_with(
    world: &mut World,
    pos: TilePos,
    owner: Option<Entity>,
    damage: (u32, u32),
    grapple: f64,
) {
    if !open_ground(world, pos) {
        return;
    }
    let taken = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| world.get::<Tentacle>(e).is_some());
    if taken {
        return;
    }
    let next = now(world) + TENTACLE_RISE;
    spawn_visual(
        world,
        pos,
        MONSTERS_DMI,
        "goliath_tentacle_spawn",
        BELOW_MOB_LAYER,
        Tentacle {
            owner,
            stage: TentacleStage::Rising,
            next,
            held: Vec::new(),
            damage,
            grapple,
        },
    );
}

/// The goliaths' tells and cooldown: hurt, the cooldown shrinks a second (`apply_damage`); from
/// 2 s before the tentacles are ready it shows its warning state (`tentacles_ready`); the
/// immortal one churns.
fn goliaths(world: &mut World) {
    let now = now(world);
    let goliaths: Vec<Entity> = world
        .query_filtered::<Entity, With<Tentacles>>()
        .iter(world)
        .collect();
    for goliath in goliaths {
        if !alive(world, goliath) {
            continue;
        }
        let health = world.get::<Health>(goliath).map_or(0.0, Health::health);
        let Some(mut t) = world.get_mut::<Tentacles>(goliath) else {
            continue;
        };
        if health < t.last_health && t.cooldown > 1.0 {
            t.cooldown = (t.cooldown - 1.0).max(1.0);
        }
        t.last_health = health;
        let warn = now >= t.ready_at - 2.0;
        let warning = t.warning.clone();
        let churn = t.churn.filter(|&c| now >= c);
        if churn.is_some() {
            t.churn = Some(now + LIFE_SECS);
        }
        let look = world
            .get::<crate::mobs::BasicMob>(goliath)
            .map(|b| (b.def.icon.clone(), b.icon_living.clone()));
        if let Some((icon, living)) = look {
            let state = if warn { warning } else { living };
            crate::mobs::set_icon_state(world, goliath, &icon, &state);
        }
        if churn.is_some()
            && let Some(&at) = world.get::<TilePos>(goliath)
        {
            let mut rng = CombatRng(world.resource::<CombatRng>().0);
            let r = CHURN_RANGE as i32;
            let mut tiles = Vec::new();
            for dy in -r..=r {
                for dx in -r..=r {
                    if (dx, dy) != (0, 0) && rng.chance(CHURN_CHANCE) {
                        tiles.push(TilePos::new(at.x + dx, at.y + dy, at.z));
                    }
                }
            }
            world.resource_mut::<CombatRng>().0 = rng.0;
            for tile in tiles {
                let blocked = !world
                    .resource::<SpatialIndex>()
                    .dense_on_tile(tile)
                    .is_empty();
                if !blocked {
                    spawn_tentacle(world, tile, Some(goliath));
                }
            }
        }
    }
}

/// The tentacles' stages: `animate_grab`, `grab`, `retract`.
fn tentacles(world: &mut World) {
    let now = now(world);
    let due: Vec<(Entity, Tentacle)> = world
        .query::<(Entity, &Tentacle)>()
        .iter(world)
        .map(|(e, t)| (e, t.clone()))
        .collect();
    for (e, t) in due {
        // `on_owner_del`; a dead goliath lets go too (`COMSIG_MOB_STATCHANGE`).
        let owner_gone = t
            .owner
            .is_some_and(|o| world.get_entity(o).is_err() || !alive(world, o));
        if owner_gone && t.stage != TentacleStage::Retracting {
            retract(world, e, now);
            continue;
        }
        if now < t.next {
            continue;
        }
        match t.stage {
            TentacleStage::Rising => {
                set_state(world, e, MONSTERS_DMI, "goliath_tentacle_wiggle");
                if let Some(mut t) = world.get_mut::<Tentacle>(e) {
                    t.stage = TentacleStage::Wiggling;
                    t.next = now + TENTACLE_WIGGLE;
                }
            }
            TentacleStage::Wiggling => grab(world, e, &t, now),
            TentacleStage::Holding => retract(world, e, now),
            TentacleStage::Retracting => {
                release_held(world, &t);
                world.despawn(e);
            }
        }
    }
}

/// `grab`: everyone alive on the tile not of the owner's faction is held and hurt.
fn grab(world: &mut World, tentacle: Entity, t: &Tentacle, now: f64) {
    let Some(&pos) = world.get::<TilePos>(tentacle) else {
        return;
    };
    let mut held = Vec::new();
    for victim in living_on(world, pos, t.owner) {
        let friendly = t
            .owner
            .is_some_and(|o| crate::mobs::same_faction(world, o, victim));
        // `TRAIT_TENTACLE_IMMUNE` (goliaths), already held.
        let immune = world.get::<Tentacles>(victim).is_some()
            || world.get::<Grabbed>(victim).is_some()
            || crate::mobs::megafauna::elites::tentacle_immune(world, victim);
        if world.get::<Stat>(victim) == Some(&Stat::Dead) || friendly || immune {
            continue;
        }
        let mut rng = CombatRng(world.resource::<CombatRng>().0);
        let damage = rng.range(t.damage.0, t.damage.1) as f32;
        let leg = if rng.chance(50.0) {
            BodyZone::LLeg
        } else {
            BodyZone::RLeg
        };
        world.resource_mut::<CombatRng>().0 = rng.0;
        let text = format!("Щупальце голиафа хватает {}!", name(world, victim));
        visible(world, pos, Span::Danger, text);
        super::hurt(world, victim, DamageType::Brute, damage, Some(leg));
        crate::status_effects::apply(world, victim, Incap::Immobilize, TENTACLE_BREAKOUT);
        world.entity_mut(victim).insert(Grabbed(tentacle));
        held.push(victim);
    }
    if held.is_empty() {
        retract(world, tentacle, now);
        return;
    }
    if let Some(mut t) = world.get_mut::<Tentacle>(tentacle) {
        t.stage = TentacleStage::Holding;
        t.next = now + t.grapple;
        t.held = held;
    }
}

fn retract(world: &mut World, tentacle: Entity, now: f64) {
    set_state(world, tentacle, MONSTERS_DMI, "goliath_tentacle_retract");
    let held = world.get::<Tentacle>(tentacle).cloned();
    if let Some(t) = held {
        release_held(world, &t);
    }
    if let Some(mut t) = world.get_mut::<Tentacle>(tentacle) {
        t.stage = TentacleStage::Retracting;
        t.next = now + TENTACLE_RETRACT;
        t.held.clear();
    }
}

fn release_held(world: &mut World, t: &Tentacle) {
    for &victim in &t.held {
        if let Ok(mut e) = world.get_entity_mut(victim) {
            e.remove::<Grabbed>();
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Legion and hivelord brood

/// `skull_launcher` or `hivelord_spawn` ready.
pub fn launcher_ready(world: &World, mob: Entity, now: f64) -> bool {
    world
        .get::<BroodLauncher>(mob)
        .is_some_and(|l| now >= l.ready_at)
}

/// `get_ranged_target_turf_direct(owner, target, max_range)`: `to` pulled back along the line to
/// within `range` of `from`.
pub fn clamp_range(from: TilePos, to: TilePos, range: u32) -> TilePos {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let far = dx.abs().max(dy.abs());
    if far <= range as i32 {
        return to;
    }
    let t = f64::from(range) / f64::from(far);
    TilePos::new(
        from.x + (f64::from(dx) * t).round() as i32,
        from.y + (f64::from(dy) * t).round() as i32,
        from.z,
    )
}

pub(super) fn blocked(world: &World, pos: TilePos) -> bool {
    let index = world.resource::<SpatialIndex>();
    !index.in_bounds(pos) || !index.dense_on_tile(pos).is_empty() || !open_ground(world, pos)
}

/// `skull_launcher/Activate` (legion) or `hivelord_spawn/spawn_brood` (hivelord) at `target`.
/// False (and a 0.5 s cooldown) when there is no room.
pub fn launch_brood(world: &mut World, mob: Entity, target: Entity) -> bool {
    let now = now(world);
    let (Some(launcher), Some(&from), Some(&at)) = (
        world.get::<BroodLauncher>(mob).copied(),
        world.get::<TilePos>(mob),
        world.get::<TilePos>(target),
    ) else {
        return false;
    };
    if now < launcher.ready_at {
        return false;
    }
    let mut rng = CombatRng(world.resource::<CombatRng>().0);
    let land = if launcher.hivelord {
        // Up to three tries a step towards the target turned by -45, 0 or 45 degrees.
        let dir = crate::pull::dir_between(from, at).unwrap_or(Dir::South);
        let mut options = Vec::new();
        for _ in 0..3 {
            let turn = rng.range(0, 2) as i32 - 1;
            let test = rotate(dir, turn).unwrap_or(dir);
            let tile = from.step(test);
            if !blocked(world, tile) {
                options.push(tile);
            }
        }
        (!options.is_empty()).then(|| options[rng.range(0, options.len() as u32 - 1) as usize])
    } else {
        let to = clamp_range(from, at, SKULL_RANGE);
        if blocked(world, to) {
            let near: Vec<TilePos> = Dir::DMI_ORDER
                .into_iter()
                .map(|d| to.step(d))
                .filter(|&p| !blocked(world, p))
                .collect();
            (!near.is_empty()).then(|| near[rng.range(0, near.len() as u32 - 1) as usize])
        } else {
            Some(to)
        }
    };
    world.resource_mut::<CombatRng>().0 = rng.0;
    let Some(land) = land else {
        if let Some(mut l) = world.get_mut::<BroodLauncher>(mob) {
            l.ready_at = now + 0.5;
        }
        return false;
    };
    let (spawn, flight, cooldown) = if launcher.hivelord {
        (super::HIVELORD_BROOD, HIVELORD_FLIGHT, HIVELORD_COOLDOWN)
    } else {
        (super::LEGION_BROOD, SKULL_FLIGHT, SKULL_COOLDOWN)
    };
    if !launcher.hivelord {
        fleeting_visual(world, land, TELEGRAPH_DMI, "skull", SKULL_INDICATOR);
    }
    world.spawn((
        land,
        Landing {
            at: now + flight,
            spawn,
            creator: mob,
            target: Some(target),
        },
    ));
    if let Some(mut l) = world.get_mut::<BroodLauncher>(mob) {
        l.ready_at = now + cooldown;
    }
    true
}

/// BYOND `turn(dir, 45 * steps)` (positive counterclockwise).
fn rotate(dir: Dir, steps: i32) -> Option<Dir> {
    const RING: [Dir; 8] = [
        Dir::North,
        Dir::NorthWest,
        Dir::West,
        Dir::SouthWest,
        Dir::South,
        Dir::SouthEast,
        Dir::East,
        Dir::NorthEast,
    ];
    let i = RING.iter().position(|&d| d == dir)? as i32;
    Some(RING[(i + steps).rem_euclid(8) as usize])
}

/// `spawn_skull` / `complete_spawn`: the brood lands with its creator's faction and target.
fn landings(world: &mut World) {
    let now = now(world);
    let due: Vec<(Entity, Landing, TilePos)> = world
        .query::<(Entity, &Landing, &TilePos)>()
        .iter(world)
        .filter(|(_, l, _)| now >= l.at)
        .map(|(e, l, p)| (e, *l, *p))
        .collect();
    for (e, landing, pos) in due {
        world.despawn(e);
        let Some(brood) = crate::mobs::spawn::spawn_mob(world, landing.spawn, pos) else {
            continue;
        };
        let faction = world.get::<crate::mobs::Faction>(landing.creator).cloned();
        if let Some(faction) = faction {
            world.entity_mut(brood).insert(faction);
        }
        let target = landing.target.filter(|&t| world.get_entity(t).is_ok());
        if let (Some(target), Some(mut ai)) = (
            target,
            world.get_mut::<crate::mobs::ai::AiController>(brood),
        ) {
            ai.bb.set(
                crate::mobs::ai::Key::CurrentTarget,
                crate::mobs::ai::Value::Entity(target),
            );
        }
    }
}

/// Brood dies at the end of its 10 s.
fn broods(world: &mut World) {
    let now = now(world);
    let due: Vec<Entity> = world
        .query::<(Entity, &Brood)>()
        .iter(world)
        .filter(|(_, b)| now >= b.dies_at)
        .map(|(e, _)| e)
        .collect();
    for e in due {
        world.entity_mut(e).remove::<Brood>();
        if let Some(mut stat) = world.get_mut::<Stat>(e) {
            stat.set_if_neq(Stat::Dead);
        }
    }
}

/// tg `IS_UNCONSCIOUS` of a human: hard crit, dead or knocked out.
pub fn out_cold(world: &World, mob: Entity) -> bool {
    world
        .get::<Stat>(mob)
        .is_some_and(|s| matches!(s, Stat::HardCrit | Stat::Dead))
        || crate::status_effects::knocked_out(world, mob)
}

/// `legion_brood/infest` + `legion/consume`: `brood` makes a legion of the human `victim`.
pub fn infest(world: &mut World, brood: Entity, victim: Entity) {
    let Some(&pos) = world.get::<TilePos>(brood) else {
        return;
    };
    let text = format!(
        "{} зарывается в плоть {}!",
        capitalize(&name(world, brood)),
        name(world, victim)
    );
    visible(world, pos, Span::Warning, text);
    let faction = world.get::<crate::mobs::Faction>(brood).cloned();
    // `qdel(src)`: it dies (`DEL_ON_DEATH`) rather than vanishing in the middle of its own AI
    // tick.
    if let Some(mut stat) = world.get_mut::<Stat>(brood) {
        stat.set_if_neq(Stat::Dead);
    }
    let Some(legion) = crate::mobs::spawn::spawn_mob(world, super::LEGION, pos) else {
        return;
    };
    info!("{victim} has been killed by legion infestation ({legion})");
    crate::medical::organs::kill(world, victim);
    let healed = world.get::<Health>(victim).map(Health::healed);
    if let Some(healed) = healed {
        world.entity_mut(victim).insert(healed);
    }
    crate::closets::put_mob_inside(world, victim, legion);
    let corpse_name = name(world, victim);
    let mut e = world.entity_mut(legion);
    e.insert((StoredCorpse(victim), Name::new(corpse_name)));
    if let Some(faction) = faction {
        e.insert(faction);
    }
    let text = format!("{} поднимается на ноги!", capitalize(&name(world, legion)));
    visible(world, pos, Span::Warning, text);
}

/// The corpse falls out where the legion was when it is gone (`content_barfer` and
/// `Exited`).
fn release_corpse(
    remove: On<Remove<StoredCorpse>>,
    legions: Query<(&StoredCorpse, Option<&TilePos>)>,
    mut commands: Commands,
) {
    let Ok((&StoredCorpse(corpse), pos)) = legions.get(remove.entity) else {
        return;
    };
    let pos = pos.copied();
    commands.queue(move |world: &mut World| {
        if world.get_entity(corpse).is_err() {
            return;
        }
        if let Some(pos) = pos {
            world.entity_mut(corpse).insert(pos);
        }
        crate::closets::let_out(world, corpse);
        if let Some(pos) = pos {
            crate::construction::tile_changed(world, pos);
        }
    });
}

// ---------------------------------------------------------------------------------------------
// Brimdemon

/// `brimbeam` ready and idle.
pub fn beam_ready(world: &World, demon: Entity, now: f64) -> bool {
    world
        .get::<Brimbeam>(demon)
        .is_some_and(|b| b.phase == BeamPhase::Idle && now >= b.ready_at)
}

/// The cardinal direction from `from` to `to`, when they share a row or column.
pub fn cardinal_towards(from: TilePos, to: TilePos) -> Option<Dir> {
    if from == to || from.z != to.z || (from.x != to.x && from.y != to.y) {
        return None;
    }
    crate::pull::dir_between(from, to)
}

/// `brimbeam/Activate`: faces the target and starts charging.
pub fn start_beam(world: &mut World, demon: Entity, dir: Dir) -> bool {
    let now = now(world);
    if !beam_ready(world, demon, now) {
        return false;
    }
    if let Some(mut facing) = world.get_mut::<Facing>(demon) {
        facing.set_if_neq(Facing(dir));
    }
    if let Some(mut b) = world.get_mut::<Brimbeam>(demon) {
        b.phase = BeamPhase::Charging {
            until: now + BEAM_CHARGE,
            dir,
        };
    }
    crate::mobs::defense::emote(world, demon, "заряжается...");
    true
}

/// `fire_laser`'s tiles: the line in `dir`, up to `BEAM_RANGE`, until an opaque tile.
pub fn beam_tiles(world: &World, from: TilePos, dir: Dir) -> Vec<TilePos> {
    let index = world.resource::<SpatialIndex>();
    let mut out = Vec::new();
    let mut at = from;
    for _ in 0..BEAM_RANGE {
        at = at.step(dir);
        if !index.in_bounds(at) {
            break;
        }
        let opaque = index
            .on_tile(at)
            .iter()
            .any(|&e| world.get::<Opaque>(e).is_some());
        if opaque {
            break;
        }
        out.push(at);
    }
    out
}

fn beams(world: &mut World) {
    let now = now(world);
    let demons: Vec<(Entity, Brimbeam)> = world
        .query::<(Entity, &Brimbeam)>()
        .iter(world)
        .map(|(e, b)| (e, b.clone()))
        .collect();
    for (demon, beam) in demons {
        match beam.phase {
            BeamPhase::Idle => {}
            BeamPhase::Charging { until, dir } if now >= until => fire_beam(world, demon, dir, now),
            BeamPhase::Firing { until, .. } if now >= until => {
                extinguish(world, demon);
                if let Some(mut b) = world.get_mut::<Brimbeam>(demon) {
                    b.ready_at = now + BEAM_COOLDOWN;
                }
            }
            _ => {}
        }
    }
}

/// `fire_laser`: the beam parts, everyone in them burnt.
fn fire_beam(world: &mut World, demon: Entity, dir: Dir, now: f64) {
    let Some(&from) = world.get::<TilePos>(demon) else {
        return;
    };
    let tiles = beam_tiles(world, from, dir);
    if tiles.is_empty() {
        let fails = ["кашляет.", "хрипит.", "выдыхает клуб чёрного дыма."];
        let mut rng = CombatRng(world.resource::<CombatRng>().0);
        let text = fails[rng.range(0, 2) as usize];
        world.resource_mut::<CombatRng>().0 = rng.0;
        crate::mobs::defense::emote(world, demon, text);
        if let Some(mut b) = world.get_mut::<Brimbeam>(demon) {
            b.phase = BeamPhase::Idle;
            b.ready_at = now + BEAM_COOLDOWN;
        }
        return;
    }
    let text = format!("{} выпускает адский луч!", capitalize(&name(world, demon)));
    visible(world, from, Span::Danger, text);
    let demon_name = name(world, demon);
    let last = tiles.len() - 1;
    let mut parts = Vec::new();
    for (i, &tile) in tiles.iter().enumerate() {
        let state = match i {
            0 => "brimbeam_start",
            i if i == last => "brimbeam_end",
            _ => "brimbeam_mid",
        };
        let part = spawn_visual(
            world,
            tile,
            MONSTERS_DMI,
            state,
            ABOVE_MOB_LAYER,
            Facing(dir),
        );
        parts.push(part);
        for victim in living_on(world, tile, Some(demon)) {
            armoured_hit(world, victim, DamageType::Burn, BEAM_DAMAGE, "laser", 0.0);
            tell(
                world,
                victim,
                Span::BoldWarning,
                format!("Вас поражает адский луч {demon_name}!"),
            );
        }
    }
    let firing = world
        .get::<crate::mobs::BasicMob>(demon)
        .map(|b| b.def.icon.clone());
    if let Some(icon) = firing {
        crate::mobs::set_icon_state(world, demon, &icon, "brimdemon_firing");
    }
    if let Some(mut b) = world.get_mut::<Brimbeam>(demon) {
        b.phase = BeamPhase::Firing {
            until: now + BEAM_DURATION,
            beams: parts,
        };
    }
}

/// `extinguish_laser`: the beam is gone, the demon looks itself again.
fn extinguish(world: &mut World, demon: Entity) {
    let Some(beam) = world.get::<Brimbeam>(demon).cloned() else {
        return;
    };
    if let BeamPhase::Firing { beams, .. } = &beam.phase {
        for &part in beams {
            if let Ok(e) = world.get_entity_mut(part) {
                e.despawn();
            }
        }
    }
    if let Some(mut b) = world.get_mut::<Brimbeam>(demon) {
        b.phase = BeamPhase::Idle;
    }
    if alive(world, demon) {
        let look = world
            .get::<crate::mobs::BasicMob>(demon)
            .map(|b| (b.def.icon.clone(), b.icon_living.clone()));
        if let Some((icon, state)) = look {
            crate::mobs::set_icon_state(world, demon, &icon, &state);
        }
    }
}

/// `brim_burst/bang`: 20 burn to everyone within a tile (bomb armour).
fn bursts(world: &mut World) {
    let now = now(world);
    let due: Vec<(Entity, TilePos)> = world
        .query::<(Entity, &BrimBurst, &TilePos)>()
        .iter(world)
        .filter(|(_, b, _)| now >= b.at)
        .map(|(e, _, p)| (e, *p))
        .collect();
    for (demon, at) in due {
        world.entity_mut(demon).remove::<BrimBurst>();
        let around: Vec<Entity> = world
            .resource::<SpatialIndex>()
            .in_radius(at, 1)
            .filter(|&e| e != demon && world.get::<Health>(e).is_some())
            .collect();
        for victim in around {
            armoured_hit(world, victim, DamageType::Burn, BURST_DAMAGE, "bomb", 0.0);
        }
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(at, "sound/effects/pop_expl.ogg", 50.0, true),
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Lobstrosity

/// `basic_charge/lobster` ready and idle.
pub fn rush_ready(world: &World, mob: Entity, now: f64) -> bool {
    world
        .get::<Rush>(mob)
        .is_some_and(|r| r.phase == RushPhase::Idle && now >= r.ready_at)
}

/// `charge/Activate` -> `do_charge` at `target`: the wind-up towards 2 tiles past it.
pub fn start_rush(world: &mut World, mob: Entity, target: Entity) -> bool {
    let now = now(world);
    let (Some(&from), Some(&at)) = (world.get::<TilePos>(mob), world.get::<TilePos>(target)) else {
        return false;
    };
    if !rush_ready(world, mob, now) || from == at {
        return false;
    }
    let dir = crate::pull::dir_between(from, at).unwrap_or(Dir::South);
    let (dx, dy) = dir.offset();
    let to = TilePos::new(at.x + dx * RUSH_PAST, at.y + dy * RUSH_PAST, at.z);
    if let Some(mut facing) = world.get_mut::<Facing>(mob) {
        facing.set_if_neq(Facing(dir));
    }
    if let Some(mut r) = world.get_mut::<Rush>(mob) {
        r.phase = RushPhase::WindUp {
            until: now + RUSH_DELAY,
            to,
        };
    }
    true
}

fn rushes(world: &mut World) {
    let now = now(world);
    let movers: Vec<(Entity, Rush)> = world
        .query::<(Entity, &Rush)>()
        .iter(world)
        .map(|(e, r)| (e, r.clone()))
        .collect();
    for (mob, rush) in movers {
        if rush.phase != RushPhase::Idle && !alive(world, mob) {
            end_rush(world, mob, now);
            continue;
        }
        match rush.phase {
            RushPhase::WindUp { until, to } if now >= until => {
                let from = world.get::<TilePos>(mob).copied();
                let left = from
                    .and_then(|f| f.chebyshev(to))
                    .unwrap_or(0)
                    .min(RUSH_DISTANCE);
                if let Some(mut r) = world.get_mut::<Rush>(mob) {
                    r.phase = RushPhase::Dashing {
                        next: now,
                        to,
                        left,
                    };
                }
            }
            RushPhase::Dashing { next, to, left } if now >= next => {
                if left == 0 || !rush_step(world, mob, to) {
                    end_rush(world, mob, now);
                } else if let Some(mut r) = world.get_mut::<Rush>(mob) {
                    r.phase = RushPhase::Dashing {
                        next: now + RUSH_STEP,
                        to,
                        left: left - 1,
                    };
                }
            }
            _ => {}
        }
    }
}

fn end_rush(world: &mut World, mob: Entity, now: f64) {
    if let Some(mut r) = world.get_mut::<Rush>(mob) {
        r.phase = RushPhase::Idle;
        r.ready_at = now + RUSH_COOLDOWN;
    }
}

/// One step of the rush (`home_onto`); a bump ends it (`on_bump` -> `hit_target`). False when
/// the rush is over.
fn rush_step(world: &mut World, mob: Entity, to: TilePos) -> bool {
    let Some(&from) = world.get::<TilePos>(mob) else {
        return false;
    };
    let Some(dir) = crate::pull::dir_between(from, to) else {
        return false;
    };
    let next = from.step(dir);
    let index = world.resource::<SpatialIndex>();
    if crate::mobs::ai::movement::can_step(index, mob, from, dir) {
        world.entity_mut(mob).insert(next);
        return true;
    }
    let victim = living_on(world, next, Some(mob))
        .into_iter()
        .find(|&e| world.get::<ss13_world::Density>(e).is_some());
    let user = capitalize(&name(world, mob));
    match victim {
        Some(victim) => {
            let text = format!("{user} делает рывок в сторону {}!", name(world, victim));
            visible(world, from, Span::Danger, text);
            crate::status_effects::knockdown(world, victim, RUSH_KNOCKDOWN, 0.0);
            // `melee_attack(ignore_cooldown = TRUE)`.
            world.entity_mut(mob).insert(NextMove(0.0));
            crate::mobs::defense::melee_attack(world, mob, victim);
        }
        None => {
            visible(
                world,
                from,
                Span::Danger,
                format!("{user} во что-то врезается!"),
            );
            crate::status_effects::apply(world, mob, Incap::Stun, RUSH_RECOIL);
        }
    }
    false
}

// ---------------------------------------------------------------------------------------------
// Bileworm

/// `bileworm_spew` ready and not spewing.
pub fn spew_ready(world: &World, worm: Entity, now: f64) -> bool {
    world
        .get::<Spew>(worm)
        .is_some_and(|s| s.left == 0 && now >= s.ready_at)
}

/// `bileworm_spew/Activate` -> `attack_sequence`: the first glob at the target.
pub fn start_spew(world: &mut World, worm: Entity, target: Entity) -> bool {
    let now = now(world);
    let Some(&at) = world.get::<TilePos>(target) else {
        return false;
    };
    if !spew_ready(world, worm, now) {
        return false;
    }
    lob_glob(world, worm, at);
    if let Some(mut s) = world.get_mut::<Spew>(worm) {
        s.target = Some(target);
        s.left = SPEW_EXTRA_SHOTS;
        s.next = now + SPEW_DELAY;
        s.dirs = Dir::DMI_ORDER.into_iter().map(Some).collect();
        s.aimed_at = Some(at);
        s.ready_at = now + SPEW_COOLDOWN;
    }
    true
}

fn spews(world: &mut World) {
    let now = now(world);
    let worms: Vec<(Entity, Spew)> = world
        .query::<(Entity, &Spew)>()
        .iter(world)
        .filter(|(_, s)| s.left > 0 && now >= s.next)
        .map(|(e, s)| (e, s.clone()))
        .collect();
    for (worm, mut spew) in worms {
        let target_pos = spew.target.and_then(|t| world.get::<TilePos>(t).copied());
        let (Some(at), true) = (target_pos, alive(world, worm)) else {
            if let Some(mut s) = world.get_mut::<Spew>(worm) {
                s.left = 0;
            }
            continue;
        };
        // They moved: every direction (and the tile itself) is in play again.
        if spew.aimed_at != Some(at) {
            spew.dirs = std::iter::once(None)
                .chain(Dir::DMI_ORDER.into_iter().map(Some))
                .collect();
            spew.aimed_at = Some(at);
        }
        if spew.dirs.is_empty() {
            spew.left = 0;
        } else {
            let mut rng = CombatRng(world.resource::<CombatRng>().0);
            let mut pick = |dirs: &mut Vec<Option<Dir>>| {
                let i = rng.range(0, dirs.len() as u32 - 1) as usize;
                dirs.swap_remove(i)
            };
            let dir = pick(&mut spew.dirs);
            let mut tile = dir.map_or(at, |d| at.step(d));
            if Some(tile) == world.get::<TilePos>(worm).copied() && !spew.dirs.is_empty() {
                let dir = pick(&mut spew.dirs);
                tile = dir.map_or(at, |d| at.step(d));
            }
            world.resource_mut::<CombatRng>().0 = rng.0;
            lob_glob(world, worm, tile);
            spew.left -= 1;
            spew.next = now + SPEW_DELAY;
        }
        if let Some(mut s) = world.get_mut::<Spew>(worm) {
            *s = spew;
        }
    }
}

/// `bileworm_acid/fire_at`: its flight time over `dist` tiles.
pub fn glob_flight(dist: f64) -> f64 {
    let speed = if dist < BILE_MAX_RANGE {
        BILE_SPEED * (0.5 + dist / BILE_MAX_RANGE * 0.5)
    } else {
        BILE_SPEED
    };
    dist / speed
}

fn lob_glob(world: &mut World, worm: Entity, at: TilePos) {
    let Some(&from) = world.get::<TilePos>(worm) else {
        return;
    };
    let dist = f64::from((from.x - at.x).pow(2) + (from.y - at.y).pow(2)).sqrt();
    let lands = now(world) + glob_flight(dist);
    spawn_visual(
        world,
        at,
        TELEGRAPH_DMI,
        "projectile_circle",
        BELOW_MOB_LAYER,
        Glob { lands, firer: worm },
    );
}

/// `bileworm_acid/impact`: everyone on the tile burnt (melee armour, 40 penetration).
fn globs(world: &mut World) {
    let now = now(world);
    let due: Vec<(Entity, Glob, TilePos)> = world
        .query::<(Entity, &Glob, &TilePos)>()
        .iter(world)
        .filter(|(_, g, _)| now >= g.lands)
        .map(|(e, g, p)| (e, *g, *p))
        .collect();
    for (e, glob, at) in due {
        world.despawn(e);
        let mut hit = false;
        for victim in living_on(world, at, Some(glob.firer)) {
            armoured_hit(
                world,
                victim,
                DamageType::Burn,
                BILE_DAMAGE,
                "melee",
                BILE_PENETRATION,
            );
            tell(
                world,
                victim,
                Span::BoldWarning,
                "В вас попадает едкая желчь!".to_owned(),
            );
            hit = true;
        }
        if hit {
            crate::sound::playsound(
                world,
                crate::sound::PlaySound::new(at, "sound/items/weapons/sear.ogg", 50.0, true),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skulls_land_at_most_seven_tiles_away() {
        let from = TilePos::new(10, 10, 1);
        assert_eq!(
            clamp_range(from, TilePos::new(13, 12, 1), 7),
            TilePos::new(13, 12, 1)
        );
        assert_eq!(
            clamp_range(from, TilePos::new(24, 10, 1), 7),
            TilePos::new(17, 10, 1)
        );
        let far = clamp_range(from, TilePos::new(24, 24, 1), 7);
        assert_eq!(far, TilePos::new(17, 17, 1));
    }

    #[test]
    fn brimbeams_fire_only_along_rows_and_columns() {
        let o = TilePos::new(5, 5, 1);
        assert!(cardinal_towards(o, TilePos::new(5, 9, 1)).is_some());
        assert!(cardinal_towards(o, TilePos::new(1, 5, 1)).is_some());
        assert!(cardinal_towards(o, TilePos::new(6, 6, 1)).is_none());
        assert!(cardinal_towards(o, o).is_none());
    }

    #[test]
    fn bile_flies_faster_over_longer_distances() {
        // 10 tiles at the full 5 tiles a second; 2 tiles at 60% of it.
        assert!((glob_flight(10.0) - 2.0).abs() < 1e-9);
        assert!((glob_flight(2.0) - 2.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn turn_by_45_degree_steps() {
        assert_eq!(rotate(Dir::North, 1), Some(Dir::NorthWest));
        assert_eq!(rotate(Dir::North, -1), Some(Dir::NorthEast));
        assert_eq!(rotate(Dir::South, 0), Some(Dir::South));
    }
}
