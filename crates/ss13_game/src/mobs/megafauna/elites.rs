//! Lavaland elites (server), after tg `mining_mobs/elites/`: the pulsing tumor that summons
//! one for a duel with whoever touched it, and the elites' attacks (`OpenFire`). An elite is a
//! [`super::Kind::Elite`] boss: the hostile AI of [`super`] (vision 6, 18 aggroed; it never
//! targets the dead, `stat_attack = HARD_CRIT`; no devouring), its attack picked at random.
//!
//! - Tumor (`/obj/structure/elite_tumor`): a human's hand wakes it (not a challenger of another
//!   tumor, `TRAIT_ELITE_CHALLENGER`); after 3 s a random elite (broodmother, pandora,
//!   legionnaire, herald) emerges. A regenerative core dropped in boosts it: then the ghosts
//!   are polled (5 s) and the chosen one plays the elite, which emerges after 10 s; nobody —
//!   it falls still. While the fight is on, every 5 s: a dead elite loses (the tumor gives a
//!   tendril chest, a boosted one with the elite's trophy half of the time, else a tumor shard,
//!   and goes); a dead or gone challenger loses (the elite revives, a boosted one at 40% of its
//!   health, and the tumor turns passive); a ring of magic walls 12 tiles out stops both fighters
//!   for 5 s; whoever got 12 tiles away is put back on the tumor. A passive tumor heals its
//!   elite standing on it (2.5% of its health a second); a new challenger's touch calls it back
//!   after 3 s, fully healed.
//! - Tumor shard on a dead elite: it revives at 40% health, on its reviver's side.
//! - Goliath broodmother: tentacle patch (1.5 s: a 3×3 and four tiles two out, 30-35 brute and a
//!   1 s grapple), two baby goliaths at its feet (4 s, at most 8), rage (10 s: faster for
//!   6.5 s), call children (6 s: around itself). Under half health tentacles rise around it
//!   every 3 s (5, 10 under a quarter). Babies dig one tentacle under a target within 7 (4 s)
//!   and blow up in flames (3) when they die.
//! - Pandora: its attacks come every 2 s (1.5 under half health, 1 under a quarter): a line of 8
//!   hierophant blasts, a box of them 2-3 tiles around the target, a teleport to a tile it sees
//!   within 12 with blasts around both ends, and a square spreading 3 rings out; 30 burn each,
//!   no extra harm to monsters.
// shortcut: no player attack buttons (a player elite attacks at random on a ranged click), no
// re-entering the tumor by an elite, no GPS signal or tumor light; the legionnaire (charge,
// head, bonfire, smoke) and the herald (shots, mirror) only fight in melee; the shard's reviver
// alliance is its faction; walls are not smoothed.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_shared::chat::Span;
use ss13_shared::{Dir, TilePos};
use ss13_world::{EnterPolicy, Mover, SpatialIndex};

use super::{
    BELOW_MOB_LAYER, Kind, Megafauna, Step as BossStep, Visual, kind_of, schedule, show, sound,
    spawn_visual, tell,
};
use crate::health::{DamageType, Health, Stat};
use crate::server::PlayerMob;

pub const ELITE_ROOT: &str = "/mob/living/simple_animal/hostile/asteroid/elite";
pub const BROODMOTHER: &str = "/mob/living/simple_animal/hostile/asteroid/elite/broodmother";
pub const BROOD_CHILD: &str = "/mob/living/simple_animal/hostile/asteroid/elite/broodmother_child";
pub const PANDORA: &str = "/mob/living/simple_animal/hostile/asteroid/elite/pandora";
pub const LEGIONNAIRE: &str = "/mob/living/simple_animal/hostile/asteroid/elite/legionnaire";
pub const LEGIONNAIRE_HEAD: &str =
    "/mob/living/simple_animal/hostile/asteroid/elite/legionnairehead";
pub const HERALD: &str = "/mob/living/simple_animal/hostile/asteroid/elite/herald";
pub const TUMOR: &str = "/obj/structure/elite_tumor";
const TUMOR_SHARD: &str = "/obj/item/tumor_shard";
const REGEN_CORE: &str = "/obj/item/organ/monster_core/regenerative_core";
const TUMOR_DMI: &str = "icons/obj/mining_zones/tumor.dmi";
const WALL_DMI: &str = "icons/turf/walls/hierophant_wall_temp.dmi";
const BIG: &str = "icons/effects/96x96.dmi";
const PHASE_IN: &str = "sound/effects/phasein.ogg";
const MAGIC: &str = "sound/effects/magic.ogg";

/// `vision_range` of an elite.
pub const VISION_RANGE: u32 = 6;
/// `potentialspawns` of the tumor.
const SPAWNS: [&str; 4] = [BROODMOTHER, PANDORA, LEGIONNAIRE, HERALD];
/// `ELITE_POST_BATTLE_HEALTH_MULTIPLIER`.
const POST_BATTLE: f32 = 0.4;
/// The tumor's timers: the elite emerging, the boosted one's poll and arrival, `arena_checks`.
const EMERGE: f64 = 3.0;
const POLL_SECS: f64 = 5.0;
const MIND_ARRIVAL: f64 = 10.0;
const ARENA_CHECK: f64 = 5.0;
/// `arena_trap`/`border_check` radius; the wall's `duration`.
const ARENA_RADIUS: i32 = 12;
const WALL_LIFE: f64 = 5.0;
/// The passive tumor heals its elite `maxHealth * 0.025` a second.
const TUMOR_HEAL: f32 = 0.025;
/// `goliath_tentacle/broodmother`: `min_damage`, `max_damage`, `grapple_time`.
const BROOD_TENTACLE: (u32, u32) = (30, 35);
const BROOD_GRAPPLE: f64 = 1.0;
const MAX_CHILDREN: usize = 8;
const RAGE_SECS: f64 = 6.5;
/// `move_to_delay` 5 -> 3 while raging.
const RAGE_PACE: f32 = 0.6;
const RANDOM_TENTACLES_EVERY: f64 = 3.0;
/// `sing_shot_length`; pandora's blast damage.
const SHOT_LENGTH: u32 = 8;
const PANDORA_BLAST: f32 = 30.0;
/// `pandora_teleport`: `view(12)`; the two 0.2 s steps.
const TELEPORT_VIEW: u32 = 12;
const TELEPORT_STEP: f64 = 0.2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EliteKind {
    Broodmother,
    BroodChild,
    Pandora,
    Legionnaire,
    LegionnaireHead,
    Herald,
}

impl EliteKind {
    pub fn of(path: &str) -> Option<Self> {
        let under = |root: &str| is_under(path, root);
        Some(if under(BROODMOTHER) {
            Self::Broodmother
        } else if under(BROOD_CHILD) {
            Self::BroodChild
        } else if under(PANDORA) {
            Self::Pandora
        } else if under(LEGIONNAIRE_HEAD) {
            Self::LegionnaireHead
        } else if under(LEGIONNAIRE) {
            Self::Legionnaire
        } else if under(HERALD) {
            Self::Herald
        } else {
            return None;
        })
    }

    /// `loot_drop`.
    fn loot(self) -> Option<&'static str> {
        match self {
            Self::Broodmother => Some("/obj/item/crusher_trophy/broodmother_tongue"),
            Self::Pandora => Some("/obj/item/clothing/accessory/pandora_hope"),
            Self::Herald => Some("/obj/item/clothing/neck/cloak/herald_cloak"),
            _ => None,
        }
    }
}

/// An elite's own state.
#[derive(Debug, Default)]
pub struct State {
    pub kind: Option<EliteKind>,
    /// The broodmother's living babies (`children_list`).
    pub children: Vec<Entity>,
    /// `COOLDOWN_DECLARE(random_tentacle)`.
    tentacles_ready: f64,
    /// The broodmother's pace before her rage.
    rage_pace: Option<f32>,
}

/// The elites' timed steps.
#[derive(Debug)]
pub enum Step {
    /// `reset_rage`.
    RageEnd,
    /// `pandora_teleport_2`: blasts around both ends, fade out.
    TeleportBlasts { to: TilePos, from: TilePos },
    /// `pandora_teleport_3`: there.
    TeleportArrive { to: TilePos },
}

/// `TRAIT_TENTACLE_IMMUNE` of the broodmother and her babies.
pub fn tentacle_immune(world: &World, mob: Entity) -> bool {
    matches!(
        elite_kind(world, mob),
        Some(EliteKind::Broodmother | EliteKind::BroodChild)
    )
}

pub fn is_pandora(world: &World, mob: Entity) -> bool {
    elite_kind(world, mob) == Some(EliteKind::Pandora)
}

fn elite_kind(world: &World, mob: Entity) -> Option<EliteKind> {
    world.get::<Megafauna>(mob).and_then(|m| m.elite.kind)
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn name(world: &World, e: Entity) -> String {
    crate::security::name_of(world, e)
}

fn cap(text: &str) -> String {
    ss13_chat::capitalize(text)
}

fn roll(world: &mut World, low: u32, high: u32) -> u32 {
    world
        .resource_mut::<crate::combat::CombatRng>()
        .range(low, high)
}

fn chance(world: &mut World, percent: f32) -> bool {
    world
        .resource_mut::<crate::combat::CombatRng>()
        .chance(percent)
}

fn living(world: &World, e: Entity) -> bool {
    world.get_entity(e).is_ok() && world.get::<Stat>(e).is_some_and(|s| *s != Stat::Dead)
}

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    app.add_click_handler(ClickPhase::EmptyHand, 2907, |w, c| {
        tumor_hand(w, c.mob, c.target)
    })
    .add_click_handler(ClickPhase::HeldOnTarget, 3007, |w, c| {
        c.held.is_some_and(|held| held_on(w, c.mob, held, c.target))
    })
    .add_systems(
        Update,
        (
            tumors,
            broodmothers,
            player_fire.after(crate::click::ResolveClicks),
        )
            .before(crate::health::HealthSystems),
    );
}

pub(super) fn attach(world: &mut World, mob: Entity, path: &str) {
    if let Some(mut m) = world.get_mut::<Megafauna>(mob) {
        m.elite.kind = EliteKind::of(path);
    }
}

// ---------------------------------------------------------------------------------------------
// Attacks

/// `OpenFire`: one attack at random (`rand(1, 4)`), behind its `ranged_cooldown`.
pub(super) fn open_fire(world: &mut World, mob: Entity, target: Entity, now: f64) {
    let Some(kind) = elite_kind(world, mob) else {
        return;
    };
    match kind {
        EliteKind::Broodmother => match roll(world, 1, 4) {
            1 => tentacle_patch(world, mob, target, now),
            2 => spawn_children(world, mob, target, now),
            3 => rage(world, mob, now),
            _ => call_children(world, mob, now),
        },
        EliteKind::BroodChild => child_fire(world, mob, target, now),
        EliteKind::Pandora => match roll(world, 1, 4) {
            1 => singular_shot(world, mob, target, now),
            2 => magic_box(world, mob, target, now),
            3 => pandora_teleport(world, mob, target, now),
            _ => aoe_squares(world, mob, target, now),
        },
        EliteKind::Legionnaire | EliteKind::LegionnaireHead | EliteKind::Herald => {}
    }
}

fn set_ranged(world: &mut World, mob: Entity, ready: f64) {
    if let Some(mut m) = world.get_mut::<Megafauna>(mob) {
        m.ranged_ready = ready;
    }
}

pub(super) fn run(world: &mut World, mob: Entity, _at: f64, step: Step) {
    match step {
        Step::RageEnd => {
            let pace = world
                .get_mut::<Megafauna>(mob)
                .and_then(|mut m| m.elite.rage_pace.take());
            if let (Some(pace), Some(mut mover)) = (pace, world.get_mut::<Mover>(mob)) {
                mover.delay = pace;
            }
            if let Some(mut look) = world.get_mut::<ss13_shared::Appearance>(mob) {
                look.color = None;
            }
        }
        Step::TeleportBlasts { to, from } => {
            let now = now(world);
            for center in [to, from] {
                teleport_telegraph(world, center);
                for pos in square(center, 1) {
                    super::hierophant::queue_blast(world, now, pos, mob, PANDORA_BLAST);
                }
            }
            if let Some(&at) = world.get::<TilePos>(mob) {
                let text = format!("{} исчезает!", cap(&name(world, mob)));
                show(world, at, Span::Warning, text);
            }
            schedule(
                world,
                mob,
                now + TELEPORT_STEP,
                BossStep::Elite(Step::TeleportArrive { to }),
            );
        }
        Step::TeleportArrive { to } => {
            world.entity_mut(mob).insert(to);
            let text = format!("{} появляется!", cap(&name(world, mob)));
            show(world, to, Span::Warning, text);
        }
    }
}

/// The tiles within `r` of `center` (`RANGE_TURFS`).
fn square(center: TilePos, r: i32) -> Vec<TilePos> {
    (-r..=r)
        .flat_map(|dy| (-r..=r).map(move |dx| TilePos::new(center.x + dx, center.y + dy, center.z)))
        .collect()
}

/// `tentacle_patch`: tentacles under the target, around it and two tiles out on each side.
fn tentacle_patch(world: &mut World, mob: Entity, target: Entity, now: f64) {
    set_ranged(world, mob, now + 1.5);
    let (Some(&at), Some(&from)) = (world.get::<TilePos>(target), world.get::<TilePos>(mob)) else {
        return;
    };
    let text = format!(
        "{} запускает щупальца под {}!",
        cap(&name(world, mob)),
        name(world, target)
    );
    show(world, from, Span::Warning, text);
    let mut tiles = square(at, 1);
    tiles.extend(Dir::CARDINALS.map(|d| at.step(d).step(d)));
    for pos in tiles {
        crate::mobs::lavaland::abilities::spawn_tentacle_with(
            world,
            pos,
            Some(mob),
            BROOD_TENTACLE,
            BROOD_GRAPPLE,
        );
    }
}

/// `spawn_children`: two babies at its feet, of its faction, after its target.
fn spawn_children(world: &mut World, mob: Entity, target: Entity, now: f64) {
    set_ranged(world, mob, now + 4.0);
    let Some(&at) = world.get::<TilePos>(mob) else {
        return;
    };
    let text = format!("Земля позади {} вздымается!", name(world, mob));
    show(world, at, Span::BoldWarning, text);
    let mut children: Vec<Entity> = world
        .get::<Megafauna>(mob)
        .map(|m| m.elite.children.clone())
        .unwrap_or_default();
    children.retain(|&c| living(world, c));
    let faction = world.get::<crate::mobs::Faction>(mob).cloned();
    for _ in 0..2 {
        if children.len() >= MAX_CHILDREN {
            break;
        }
        let Some(child) = crate::mobs::spawn::spawn_mob(world, BROOD_CHILD, at) else {
            break;
        };
        if let Some(f) = faction.clone() {
            world.entity_mut(child).insert(f);
        }
        // `GiveTarget(target)`.
        if let Some(mut m) = world.get_mut::<Megafauna>(child) {
            m.target = Some(target);
            m.aggro = true;
        }
        let text = format!(
            "{} появляется под {}!",
            cap(&name(world, child)),
            name(world, mob)
        );
        show(world, at, Span::BoldWarning, text);
        children.push(child);
    }
    if let Some(mut m) = world.get_mut::<Megafauna>(mob) {
        m.elite.children = children;
    }
}

/// `rage`: red and faster for 6.5 s.
fn rage(world: &mut World, mob: Entity, now: f64) {
    set_ranged(world, mob, now + 10.0);
    let Some(&at) = world.get::<TilePos>(mob) else {
        return;
    };
    sound(world, at, "sound/misc/insane_low_laugh.ogg", 200.0, 0.0);
    let text = format!("{} набирает скорость!", cap(&name(world, mob)));
    show(world, at, Span::Warning, text);
    let raging = world
        .get::<Megafauna>(mob)
        .is_some_and(|m| m.elite.rage_pace.is_some());
    let pace = world.get::<Mover>(mob).map(|m| m.delay).filter(|_| !raging);
    if let Some(pace) = pace {
        if let Some(mut mover) = world.get_mut::<Mover>(mob) {
            mover.delay = pace * RAGE_PACE;
        }
        if let Some(mut m) = world.get_mut::<Megafauna>(mob) {
            m.elite.rage_pace = Some(pace);
        }
    }
    if let Some(mut look) = world.get_mut::<ss13_shared::Appearance>(mob) {
        look.color = Some([0xFF, 0x00, 0x00, 0xFF]);
    }
    schedule(world, mob, now + RAGE_SECS, BossStep::Elite(Step::RageEnd));
}

/// `call_children`: every baby to a free direction around her.
fn call_children(world: &mut World, mob: Entity, now: f64) {
    set_ranged(world, mob, now + 6.0);
    let Some(&at) = world.get::<TilePos>(mob) else {
        return;
    };
    let text = format!("Земля дрожит рядом с {}!", name(world, mob));
    show(world, at, Span::Warning, text);
    let children: Vec<Entity> = world
        .get::<Megafauna>(mob)
        .map(|m| m.elite.children.clone())
        .unwrap_or_default()
        .into_iter()
        .filter(|&c| living(world, c))
        .collect();
    let mut dirs: Vec<Dir> = Dir::DMI_ORDER.to_vec();
    for child in children {
        if dirs.is_empty() {
            break;
        }
        let i = roll(world, 0, dirs.len() as u32 - 1) as usize;
        let dir = dirs.swap_remove(i);
        world.entity_mut(child).insert(at.step(dir));
        sound(world, at, "sound/effects/bamf.ogg", 100.0, 0.0);
    }
}

/// `broodmother_child/OpenFire`: one tentacle under a target within 7.
fn child_fire(world: &mut World, mob: Entity, target: Entity, now: f64) {
    set_ranged(world, mob, now + 4.0);
    let (Some(&at), Some(&from)) = (world.get::<TilePos>(target), world.get::<TilePos>(mob)) else {
        return;
    };
    if from.chebyshev(at).is_none_or(|d| d > 7) {
        return;
    }
    let text = format!(
        "{} запускает щупальце под {}!",
        cap(&name(world, mob)),
        name(world, target)
    );
    show(world, from, Span::Warning, text);
    crate::mobs::lavaland::abilities::spawn_tentacle_with(
        world,
        at,
        Some(mob),
        BROOD_TENTACLE,
        BROOD_GRAPPLE,
    );
}

/// `broodmother/Life`: under half health, tentacles rise around her every 3 s.
// Cost: per tick, one pass over the bosses (a handful).
fn broodmothers(world: &mut World) {
    let now = now(world);
    let mothers: Vec<(Entity, f32, f32, TilePos)> = world
        .query::<(Entity, &Megafauna, &Health, &Stat, &TilePos)>()
        .iter(world)
        .filter(|(_, m, _, s, _)| {
            m.elite.kind == Some(EliteKind::Broodmother)
                && **s == Stat::Conscious
                && now >= m.elite.tentacles_ready
        })
        .map(|(e, _, h, _, p)| (e, h.health(), h.max, *p))
        .collect();
    for (mob, health, max, at) in mothers {
        if health >= max * 0.5 {
            continue;
        }
        if let Some(mut m) = world.get_mut::<Megafauna>(mob) {
            m.elite.tentacles_ready = now + RANDOM_TENTACLES_EVERY;
        }
        let amount = if health < max * 0.25 { 10 } else { 5 };
        let mut tiles = square(at, 5);
        for _ in 0..amount {
            if tiles.is_empty() {
                break;
            }
            let i = roll(world, 0, tiles.len() as u32 - 1) as usize;
            let pos = tiles.swap_remove(i);
            crate::mobs::lavaland::abilities::spawn_tentacle_with(
                world,
                pos,
                Some(mob),
                BROOD_TENTACLE,
                BROOD_GRAPPLE,
            );
        }
    }
}

/// Pandora's `cooldown_time` by its health (`Life`).
pub fn pandora_cooldown(health: f32, max: f32) -> f64 {
    if health >= max * 0.5 {
        2.0
    } else if health > max * 0.25 {
        1.5
    } else {
        1.0
    }
}

fn cooldown_of(world: &World, mob: Entity) -> f64 {
    world
        .get::<Health>(mob)
        .map_or(2.0, |h| pandora_cooldown(h.health(), h.max))
}

/// `singular_shot`: a line of 8 blasts toward the target, one every tenth of the cooldown.
fn singular_shot(world: &mut World, mob: Entity, target: Entity, now: f64) {
    let cd = cooldown_of(world, mob);
    set_ranged(world, mob, now + cd * 0.5);
    let (Some(&from), Some(&to)) = (world.get::<TilePos>(mob), world.get::<TilePos>(target)) else {
        return;
    };
    let Some(dir) = crate::pull::dir_between(from, to) else {
        return;
    };
    let mut pos = from.step(dir);
    for i in 0..SHOT_LENGTH {
        super::hierophant::queue_blast(
            world,
            now + f64::from(i) * cd * 0.1,
            pos,
            mob,
            PANDORA_BLAST,
        );
        pos = pos.step(dir);
    }
}

/// `magic_box`: blasts 2-3 tiles around the target.
fn magic_box(world: &mut World, mob: Entity, target: Entity, now: f64) {
    let cd = cooldown_of(world, mob);
    set_ranged(world, mob, now + cd);
    let Some(&at) = world.get::<TilePos>(target) else {
        return;
    };
    for pos in square(at, 3) {
        if pos.chebyshev(at).is_some_and(|d| d > 1) {
            super::hierophant::queue_blast(world, now, pos, mob, PANDORA_BLAST);
        }
    }
}

fn telegraph(world: &mut World, pos: TilePos, state: &'static str, secs: f64) {
    spawn_visual(
        world,
        pos,
        Visual {
            layer: BELOW_MOB_LAYER,
            offset: [-32, -32],
            ..Visual::new(BIG, state, secs)
        },
    );
}

fn teleport_telegraph(world: &mut World, pos: TilePos) {
    telegraph(world, pos, "hierophant_telegraph_teleport", 0.9);
}

/// `pandora_teleport`: to the target's tile if it sees it within 12.
fn pandora_teleport(world: &mut World, mob: Entity, target: Entity, now: f64) {
    let (Some(&from), Some(&to)) = (world.get::<TilePos>(mob), world.get::<TilePos>(target)) else {
        return;
    };
    if !crate::mobs::ai::behaviors::can_see(world, from, to, TELEPORT_VIEW) {
        return;
    }
    let cd = cooldown_of(world, mob);
    set_ranged(world, mob, now + cd * 2.0);
    telegraph(world, to, "hierophant_telegraph", 0.6);
    telegraph(world, from, "hierophant_telegraph", 0.6);
    sound(
        world,
        from,
        "sound/machines/airlock/airlockopen.ogg",
        200.0,
        0.0,
    );
    schedule(
        world,
        mob,
        now + TELEPORT_STEP,
        BossStep::Elite(Step::TeleportBlasts { to, from }),
    );
}

/// `aoe_squares`: a blast on the target's tile, then rings spreading 3 out.
fn aoe_squares(world: &mut World, mob: Entity, target: Entity, now: f64) {
    let cd = cooldown_of(world, mob);
    set_ranged(world, mob, now + cd);
    let Some(&at) = world.get::<TilePos>(target) else {
        return;
    };
    super::hierophant::queue_blast(world, now, at, mob, PANDORA_BLAST);
    for ring in 0..=3_u32 {
        let when = now + TELEPORT_STEP + f64::from(ring) * cd * 0.1;
        for pos in square(at, ring as i32) {
            if pos.chebyshev(at) == Some(ring) {
                super::hierophant::queue_blast(world, when, pos, mob, PANDORA_BLAST);
            }
        }
    }
}

/// `broodmother_child/death`: it blows up in flames and is gone. Other elites stop their rage.
pub(super) fn on_death(world: &mut World, mob: Entity, pos: TilePos) {
    if elite_kind(world, mob) != Some(EliteKind::BroodChild) {
        return;
    }
    let text = format!("{} взрывается!", cap(&name(world, mob)));
    show(world, pos, Span::Warning, text);
    let mut ex = crate::explosions::Explosion::new(0, 0, 0).flame(3);
    ex.cause = "baby goliath".to_owned();
    crate::explosions::explosion(world, pos, ex);
    if world.get_entity(mob).is_ok() {
        crate::health::ghostize_gone(world, mob);
        world.despawn(mob);
    }
}

/// A player elite's ranged click opens fire (an attack at random), behind its cooldown.
fn player_fire(world: &mut World, mut cursor: Local<MessageCursor<crate::click::RangedClick>>) {
    let clicks: Vec<crate::click::RangedClick> = cursor
        .read(world.resource::<Messages<crate::click::RangedClick>>())
        .copied()
        .collect();
    let now = now(world);
    for click in clicks {
        let ready = world
            .get::<Megafauna>(click.mob)
            .is_some_and(|m| m.elite.kind.is_some() && now >= m.ranged_ready);
        if !ready
            || world.get::<Stat>(click.mob) != Some(&Stat::Conscious)
            || world.get::<Health>(click.target).is_none()
        {
            continue;
        }
        if let Some(mut m) = world.get_mut::<Megafauna>(click.mob) {
            m.target = Some(click.target);
        }
        open_fire(world, click.mob, click.target, now);
    }
}

// ---------------------------------------------------------------------------------------------
// The tumor

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Activity {
    #[default]
    Inactive,
    Active,
    Passive,
}

/// What the tumor waits for.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Pending {
    /// `spawn_elite(elitemind)` at the time; the chosen ghost (client, ghost mob).
    Spawn {
        at: f64,
        mind: Option<(Entity, Entity)>,
    },
    /// `return_elite`.
    Return { at: f64 },
    /// The ghosts are being polled.
    Poll,
}

/// `/obj/structure/elite_tumor`.
#[derive(Component, Clone, Debug, Default)]
pub struct EliteTumor {
    pub activity: Activity,
    pub boosted: bool,
    pub times_won: u32,
    pub activator: Option<Entity>,
    pub child: Option<Entity>,
    pending: Option<Pending>,
    next_check: f64,
}

/// `TRAIT_ELITE_CHALLENGER` from this tumor.
#[derive(Component, Clone, Copy, Debug)]
pub struct EliteChallenger(pub Entity);

/// `/obj/effect/temp_visual/elite_tumor_wall`: stops the two fighters.
#[derive(Component, Clone, Copy, Debug)]
pub struct TumorWall {
    pub fighters: [Option<Entity>; 2],
}

fn is_tumor(world: &World, e: Entity) -> bool {
    world
        .get::<ss13_mapload::DmObj>(e)
        .is_some_and(|o| is_under(&o.path, TUMOR))
}

/// A mapped tumor gets its state on first use.
fn ensure_tumor(world: &mut World, e: Entity) -> bool {
    if world.get::<EliteTumor>(e).is_some() {
        return true;
    }
    if !is_tumor(world, e) {
        return false;
    }
    world.entity_mut(e).insert(EliteTumor::default());
    true
}

fn tumor_state(world: &mut World, tumor: Entity, state: &str) {
    let Some(id) = world
        .get_resource::<ss13_mapload::IconIndex>()
        .and_then(|i| i.get(TUMOR_DMI, state))
    else {
        return;
    };
    if let Some(mut look) = world.get_mut::<ss13_shared::Appearance>(tumor) {
        look.icon_state = id;
    }
}

fn is_human(world: &World, mob: Entity) -> bool {
    world.get::<crate::medical::wounds::Limbs>(mob).is_some()
        && world.get::<crate::mobs::BasicMob>(mob).is_none()
}

/// `elite_tumor/attack_hand` of a human.
fn tumor_hand(world: &mut World, user: Entity, tumor: Entity) -> bool {
    if !ensure_tumor(world, tumor) {
        return false;
    }
    if !is_human(world, user) {
        return true;
    }
    let Some(t) = world.get::<EliteTumor>(tumor).cloned() else {
        return true;
    };
    let Some(&at) = world.get::<TilePos>(tumor) else {
        return true;
    };
    if t.activity == Activity::Active {
        return true;
    }
    let who = name(world, user);
    if world.get::<EliteChallenger>(user).is_some() {
        show(
            world,
            at,
            Span::Warning,
            format!("{who} тянется к опухоли, но ничего не происходит."),
        );
        return true;
    }
    let now = now(world);
    make_activator(world, tumor, user);
    let pending = match t.activity {
        Activity::Passive => {
            show(
                world,
                at,
                Span::BoldWarning,
                format!("Опухоль содрогается, когда к ней тянется рука {who}. Ох-ох..."),
            );
            if t.boosted
                && let Some(child) = t.child
            {
                tell(
                    world,
                    child,
                    "Кто-то активировал вашу опухоль. Скоро вас вернут на бой — готовьтесь!"
                        .to_owned(),
                );
            }
            Some(Pending::Return { at: now + EMERGE })
        }
        _ => {
            show(
                world,
                at,
                Span::BoldWarning,
                "Опухоль начинает содрогаться. Инстинкты велят вам отступить.".to_owned(),
            );
            if t.boosted {
                show(
                    world,
                    at,
                    Span::BoldWarning,
                    "Что-то шевелится внутри опухоли...".to_owned(),
                );
                Some(Pending::Poll)
            } else {
                Some(Pending::Spawn {
                    at: now + EMERGE,
                    mind: None,
                })
            }
        }
    };
    if let Some(mut t) = world.get_mut::<EliteTumor>(tumor) {
        t.activity = Activity::Active;
        t.pending = pending;
        t.next_check = now + EMERGE;
    }
    // After the state is set: with no ghost to ask the poll answers at once.
    if pending == Some(Pending::Poll) {
        poll_for_elite(world, tumor);
    }
    true
}

/// `SSpolling.poll_ghosts_for_target` (5 s) for a boosted tumor's elite.
fn poll_for_elite(world: &mut World, tumor: Entity) {
    let spec = crate::ghost_poll::PollSpec {
        question: "Хотите сыграть за элиту Лаваленда?".to_owned(),
        role_name: "lavaland elite".to_owned(),
        secs: POLL_SECS,
        ignore: Some("lavaland_elite"),
        target: Some(tumor),
        ..Default::default()
    };
    let ghosts = crate::ghost_poll::ghosts(world);
    crate::ghost_poll::poll_candidates(world, spec, ghosts, move |world, mut signed_up| {
        if world.get::<EliteTumor>(tumor).is_none() {
            return;
        }
        let chosen = crate::ghost_poll::pick_candidate(world, &mut signed_up);
        let Some(&at) = world.get::<TilePos>(tumor) else {
            return;
        };
        let now = now(world);
        match chosen {
            Some(c) => {
                show(
                    world,
                    at,
                    Span::BoldWarning,
                    "Шевеление внутри усиливается!".to_owned(),
                );
                crate::tools::tell(
                    world,
                    c.client,
                    Span::BoldNotice,
                    "Вы избраны элитой Лаваленда. Через несколько секунд вы появитесь на \
                     Лаваленде чудовищем, чтобы сразиться насмерть с тем, кто вас пробудил. \
                     Победив, вы узнаете, что делать дальше. Удачи!",
                );
                if let Some(mut t) = world.get_mut::<EliteTumor>(tumor) {
                    t.pending = Some(Pending::Spawn {
                        at: now + MIND_ARRIVAL,
                        mind: Some((c.client, c.mob)),
                    });
                }
            }
            None => {
                show(
                    world,
                    at,
                    Span::BoldWarning,
                    "Шевеление стихает, и ничего не появляется. Может, попробовать позже."
                        .to_owned(),
                );
                let activator = world.get::<EliteTumor>(tumor).and_then(|t| t.activator);
                if let Some(mut t) = world.get_mut::<EliteTumor>(tumor) {
                    t.activity = Activity::Inactive;
                    t.pending = None;
                }
                clear_activator(world, tumor, activator);
            }
        }
    });
}

fn make_activator(world: &mut World, tumor: Entity, user: Entity) {
    if world
        .get::<EliteTumor>(tumor)
        .is_some_and(|t| t.activator.is_some())
    {
        return;
    }
    if let Some(mut t) = world.get_mut::<EliteTumor>(tumor) {
        t.activator = Some(user);
    }
    world.entity_mut(user).insert(EliteChallenger(tumor));
}

fn clear_activator(world: &mut World, tumor: Entity, activator: Option<Entity>) {
    if let Some(mut t) = world.get_mut::<EliteTumor>(tumor) {
        t.activator = None;
    }
    if let Some(user) = activator
        && let Ok(mut e) = world.get_entity_mut(user)
        && e.get::<EliteChallenger>().is_some_and(|c| c.0 == tumor)
    {
        e.remove::<EliteChallenger>();
    }
}

/// `spawn_elite`: a random elite out of the tumor; a boosted one gets its player.
fn spawn_elite(world: &mut World, tumor: Entity, mind: Option<(Entity, Entity)>) {
    let Some(&at) = world.get::<TilePos>(tumor) else {
        return;
    };
    let path = SPAWNS[roll(world, 0, SPAWNS.len() as u32 - 1) as usize];
    let Some(child) = crate::mobs::spawn::spawn_mob(world, path, at) else {
        warn!("no prototype for {path}");
        return;
    };
    let text = format!("{} выходит из опухоли!", cap(&name(world, child)));
    show(world, at, Span::BoldWarning, text);
    sound(world, at, PHASE_IN, 200.0, 50.0);
    if let Some((client, ghost)) = mind
        && world.get::<crate::ghost::Ghost>(ghost).is_some()
        && world.get::<PlayerMob>(ghost).map(|p| p.client) == Some(client)
        && crate::medical::brainmob::possess(world, client, ghost, child)
    {
        world.despawn(ghost);
        let text = format!("{} пробудилась на Лаваленде!", cap(&name(world, child)));
        for g in crate::ghost_poll::ghosts(world) {
            crate::tools::tell_mob(world, g, Span::Notice, text.clone());
        }
    }
    tumor_state(world, tumor, "tumor_popped");
    let now = now(world);
    if let Some(mut t) = world.get_mut::<EliteTumor>(tumor) {
        t.child = Some(child);
        t.next_check = now;
    }
}

/// `return_elite`: the passive elite back on the tumor, fully healed.
fn return_elite(world: &mut World, tumor: Entity) {
    let Some(t) = world.get::<EliteTumor>(tumor).cloned() else {
        return;
    };
    let (Some(child), Some(&at)) = (t.child, world.get::<TilePos>(tumor)) else {
        return;
    };
    if world.get_entity(child).is_err() {
        return;
    }
    world.entity_mut(child).insert(at);
    let text = format!("{} выходит из опухоли!", cap(&name(world, child)));
    show(world, at, Span::BoldWarning, text);
    sound(world, at, PHASE_IN, 200.0, 50.0);
    if t.boosted
        && let Some(mut h) = world.get_mut::<Health>(child)
    {
        h.max /= POST_BATTLE;
    }
    crate::spells::effects::full_heal(world, child);
    let now = now(world);
    if let Some(mut t) = world.get_mut::<EliteTumor>(tumor) {
        t.next_check = now;
    }
}

/// `onEliteLoss`: the tumor gives its chest and goes.
fn elite_lost(world: &mut World, tumor: Entity) {
    let Some(t) = world.get::<EliteTumor>(tumor).cloned() else {
        return;
    };
    let Some(&at) = world.get::<TilePos>(tumor) else {
        return;
    };
    sound(
        world,
        at,
        "sound/effects/tendril_destroyed.ogg",
        200.0,
        50.0,
    );
    show(
        world,
        at,
        Span::BoldWarning,
        "Опухоль бешено содрогается и начинает рассеиваться.".to_owned(),
    );
    show(
        world,
        at,
        Span::BoldWarning,
        "Закрываясь, опухоль выталкивает что-то из глубины.".to_owned(),
    );
    let chest = crate::construction::spawn_obj(world, crate::mining::necropolis::TENDRIL_CHEST, at);
    if t.boosted
        && let Some(chest) = chest
    {
        let loot = t
            .child
            .and_then(|c| elite_kind(world, c))
            .and_then(EliteKind::loot);
        let item = match loot {
            Some(loot) if chance(world, 50.0) => loot,
            _ => TUMOR_SHARD,
        };
        if let Some(item) = crate::construction::spawn_detached(world, item) {
            crate::storage::put_inside(world, item, chest);
        }
    }
    clear_activator(world, tumor, t.activator);
    world.despawn(tumor);
}

/// `onEliteWon`: passive, the elite revived (a boosted one weakened) and told what now.
fn elite_won(world: &mut World, tumor: Entity) {
    let Some(t) = world.get::<EliteTumor>(tumor).cloned() else {
        return;
    };
    clear_activator(world, tumor, t.activator);
    let times_won = t.times_won + u32::from(t.boosted);
    if let Some(mut tt) = world.get_mut::<EliteTumor>(tumor) {
        tt.activity = Activity::Passive;
        tt.times_won = times_won;
    }
    let Some(child) = t.child.filter(|&c| world.get_entity(c).is_ok()) else {
        return;
    };
    crate::spells::effects::full_heal(world, child);
    if t.boosted
        && let Some(mut h) = world.get_mut::<Health>(child)
    {
        h.max *= POST_BATTLE;
        let max = h.max;
        let healed = h.clone().healed();
        *h = healed;
        h.max = max;
    }
    if times_won == 1 {
        tell(
            world,
            child,
            "С угасанием жизни в глазах пробудившего силовое поле вокруг вас исчезает, а \
             сила убывает. Отныне ваша цель — ваше собственное решение. Ваше здоровье \
             уменьшено, но вы лечитесь, стоя на своей опухоли — это единственный способ. \
             Если кто-то коснётся опухоли, вас вернут на новый бой с полным здоровьем. \
             Вы — чудовище Лаваленда и не союзник станции."
                .to_owned(),
        );
    }
}

/// `arena_trap`: the ring of magic walls 12 tiles out (5 s), stopping both fighters.
fn arena_trap(world: &mut World, tumor: Entity, t: &EliteTumor) {
    let Some(&at) = world.get::<TilePos>(tumor) else {
        return;
    };
    let r = ARENA_RADIUS;
    let ring: Vec<TilePos> = square(at, r)
        .into_iter()
        .filter(|p| p.chebyshev(at) == Some(r as u32))
        .filter(|&p| world.resource::<SpatialIndex>().in_bounds(p))
        .collect();
    let now = now(world);
    for pos in ring {
        let visual = Visual {
            layer: BELOW_MOB_LAYER,
            color: Some([0xFF, 0x00, 0x00, 0xFF]),
            ..Visual::new(WALL_DMI, "hierophant_wall_temp-0", WALL_LIFE)
        };
        let wall = match spawn_visual(world, pos, visual) {
            Some(e) => e,
            None => world
                .spawn((
                    pos,
                    super::TempVisual {
                        until: now + WALL_LIFE,
                    },
                ))
                .id(),
        };
        world.entity_mut(wall).insert((
            TumorWall {
                fighters: [t.activator, t.child],
            },
            EnterPolicy(wall_lets_through),
        ));
    }
}

/// `elite_tumor_wall/CanAllowThrough`: not the challenger or the elite.
fn wall_lets_through(world: &World, mover: Entity, wall: Entity) -> bool {
    world
        .get::<TumorWall>(wall)
        .is_none_or(|w| !w.fighters.contains(&Some(mover)))
}

/// `border_check`: a fighter 12 tiles away is put back on the tumor.
fn border_check(world: &mut World, tumor: Entity, t: &EliteTumor) {
    let Some(&at) = world.get::<TilePos>(tumor) else {
        return;
    };
    for fighter in [t.activator, t.child].into_iter().flatten() {
        let far = world
            .get::<TilePos>(fighter)
            .is_some_and(|p| p.chebyshev(at).is_none_or(|d| d >= ARENA_RADIUS as u32));
        if !far {
            continue;
        }
        world.entity_mut(fighter).insert(at);
        let text = format!(
            "{} внезапно появляется над опухолью!",
            cap(&name(world, fighter))
        );
        show(world, at, Span::BoldWarning, text);
        sound(world, at, PHASE_IN, 200.0, 50.0);
    }
}

/// The tumors' timers, `arena_checks` every 5 s while a fight is on, the passive heal.
// Cost: per tick, one pass over the tumors (a few on the mining level).
fn tumors(world: &mut World) {
    let now = now(world);
    let dt = world.resource::<Time>().delta_secs();
    let tumors: Vec<(Entity, EliteTumor)> = world
        .query::<(Entity, &EliteTumor)>()
        .iter(world)
        .map(|(e, t)| (e, t.clone()))
        .collect();
    for (tumor, t) in tumors {
        match t.pending {
            Some(Pending::Spawn { at, mind }) if now >= at => {
                if let Some(mut tt) = world.get_mut::<EliteTumor>(tumor) {
                    tt.pending = None;
                }
                spawn_elite(world, tumor, mind);
                continue;
            }
            Some(Pending::Return { at }) if now >= at => {
                if let Some(mut tt) = world.get_mut::<EliteTumor>(tumor) {
                    tt.pending = None;
                }
                return_elite(world, tumor);
                continue;
            }
            _ => {}
        }
        match t.activity {
            Activity::Active if t.pending.is_none() => {
                // A gone elite loses at once (`COMSIG_QDELETING`).
                let gone = t.child.is_none_or(|c| world.get_entity(c).is_err());
                if gone {
                    elite_lost(world, tumor);
                    continue;
                }
                if now < t.next_check {
                    continue;
                }
                if let Some(mut tt) = world.get_mut::<EliteTumor>(tumor) {
                    tt.next_check = now + ARENA_CHECK;
                }
                // `fighters_check`.
                if t.child.is_none_or(|c| !living(world, c)) {
                    elite_lost(world, tumor);
                    continue;
                }
                if t.activator.is_none_or(|a| !living(world, a)) {
                    elite_won(world, tumor);
                    continue;
                }
                arena_trap(world, tumor, &t);
                border_check(world, tumor, &t);
            }
            Activity::Passive => {
                let Some(child) = t.child.filter(|&c| living(world, c)) else {
                    continue;
                };
                let on_tumor = world.get::<TilePos>(child) == world.get::<TilePos>(tumor);
                if !on_tumor {
                    continue;
                }
                if let Some(mut h) = world.get_mut::<Health>(child) {
                    let heal = h.max * TUMOR_HEAL * dt;
                    h.adjust(DamageType::Brute, -heal);
                }
            }
            _ => {}
        }
    }
}

/// A regenerative core into an idle tumor boosts it; a tumor shard revives a dead elite.
fn held_on(world: &mut World, user: Entity, held: Entity, target: Entity) -> bool {
    let Some(path) = crate::construction::path_of(world, held) else {
        return false;
    };
    if is_under(&path, REGEN_CORE) && ensure_tumor(world, target) {
        let Some(t) = world.get::<EliteTumor>(target).cloned() else {
            return false;
        };
        if t.activity != Activity::Inactive || t.boosted {
            return false;
        }
        if let Some(&at) = world.get::<TilePos>(target) {
            let text = format!(
                "Когда {} бросает ядро в опухоль, та будто набухает.",
                name(world, user)
            );
            show(world, at, Span::BoldWarning, text);
        }
        tumor_state(world, target, "advanced_tumor");
        if let Some(mut t) = world.get_mut::<EliteTumor>(target) {
            t.boosted = true;
        }
        world.despawn(held);
        return true;
    }
    if is_under(&path, TUMOR_SHARD) && kind_of(world, target) == Some(Kind::Elite) {
        revive_with_shard(world, user, held, target);
        return true;
    }
    false
}

/// `tumor_shard/interact_with_atom`: a dead elite back at 40% health, on its reviver's side.
fn revive_with_shard(world: &mut World, user: Entity, shard: Entity, elite: Entity) {
    let Some(&at) = world.get::<TilePos>(elite) else {
        return;
    };
    let dead = world.get::<Stat>(elite) == Some(&Stat::Dead);
    let spawned = matches!(
        elite_kind(world, elite),
        Some(
            EliteKind::Broodmother
                | EliteKind::Pandora
                | EliteKind::Legionnaire
                | EliteKind::Herald
        )
    );
    let who = name(world, elite);
    if !dead || !spawned {
        show(
            world,
            at,
            Span::Notice,
            format!("Похоже, {who} сейчас нельзя оживить. Попробуйте позже."),
        );
        return;
    }
    let faction = world.get::<crate::mobs::Faction>(user).cloned();
    if let Some(f) = faction {
        world.entity_mut(elite).insert(f);
    }
    crate::spells::effects::full_heal(world, elite);
    if let Some(mut h) = world.get_mut::<Health>(elite) {
        h.max *= POST_BATTLE;
        let max = h.max;
        let healed = h.clone().healed();
        *h = healed;
        h.max = max;
    }
    let text = format!(
        "{} вонзает осколок в {who}, оживляя его.",
        cap(&name(world, user))
    );
    show(world, at, Span::Notice, text);
    if let Some(client) = world.get::<PlayerMob>(elite).map(|p| p.client) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(at, MAGIC, 40.0, false));
        let text = format!(
            "Вас оживил {}. Вы не можете говорить с ним, но в огромном долгу перед ним: \
             помогайте ему в его целях любой ценой.",
            name(world, user)
        );
        crate::tools::tell(world, client, Span::Danger, text);
    }
    world.despawn(shard);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elite_kinds_by_path() {
        assert_eq!(EliteKind::of(BROODMOTHER), Some(EliteKind::Broodmother));
        assert_eq!(EliteKind::of(BROOD_CHILD), Some(EliteKind::BroodChild));
        assert_eq!(
            EliteKind::of(LEGIONNAIRE_HEAD),
            Some(EliteKind::LegionnaireHead)
        );
        assert_eq!(EliteKind::of(LEGIONNAIRE), Some(EliteKind::Legionnaire));
        assert_eq!(EliteKind::of(ELITE_ROOT), None);
        assert_eq!(Kind::of(PANDORA), Some(Kind::Elite));
    }

    #[test]
    fn pandora_attacks_faster_as_it_is_hurt() {
        assert_eq!(pandora_cooldown(1000.0, 1000.0), 2.0);
        assert_eq!(pandora_cooldown(400.0, 1000.0), 1.5);
        assert_eq!(pandora_cooldown(250.0, 1000.0), 1.0);
    }
}
