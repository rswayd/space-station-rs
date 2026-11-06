//! What the ported spells do (`cast` of each spell type), the magic bolts they and the staves
//! fire (`/obj/projectile/magic/*` `prehit_pierce` and `on_hit`), the touch hand, the ethereal
//! jaunt and the forcefield.
//!
//! - Magic Missile: a bolt at every living mob within 7 tiles (`view`, range only); each flies
//!   2 tiles a second through everything to its target (`can_only_hit_target`) and paralyzes it
//!   6 s. Fireball: a bolt (`6 + 2 × level` tiles) that burns 10 + 10 and explodes (light 2,
//!   flame 2, flash 3). Lightning Bolt: a 3.5 tiles/ds bolt (15 burn) whose hit arcs
//!   (`tesla_zap`, 20 000 power, range 15): the nearest living mob that is not the caster takes
//!   `min(power / 600, 90) ± 5` burn, the arc jumps on within 5 tiles with a third less power
//!   (a fifth of the time splitting in two halves) until the power is under 1000.
//! - Smite (touch): the hand gibs a touched living mob and flashes everyone around; an
//!   antimagic victim flashes the caster instead.
//! - Blind: 20 s blind, 40 s blur. Disable Tech: an EMP (heavy 6, light 10). Repulse: every
//!   unanchored item and mob within 5 tiles is thrown away (3-5 tiles, 4 s paralysis; on the
//!   caster's tile 10 s and 5 brute). Smoke: the choking smoke (`crate::grenades`), range 4.
//!   Forcewall: three dense forcefields (the caster's tile and both sides) for 30 s. Knock:
//!   doors (unbolted) and closets (unlocked) within 3 tiles open.
//! - Blink: a random tile within 6. Teleport: a random free tile of the chosen station area,
//!   forced (out of the den too). Swap: the caster and the target change places (with a
//!   right-click-marked second target: three ways). Summon Item: marks the held item, recalls it
//!   (with the bag it is in) to the caster's hands from anywhere, unmarks when used holding it.
//!   Charge: refills the spells of a pulled magic user, else the held staff, wand or energy gun.
//! - Ethereal Jaunt: 5 s invisible and untouchable (no clicks), moving through walls one tile
//!   per 0.2 s (`phased_mob/spell_jaunt` `movespeed`); then 2.5 s reappearing on the last
//!   unblocked tile it passed (or where it started) before it is solid again.
//! - Bolts on an antimagic mob fizzle (`prehit_pierce`). Death kills, resurrection heals all
//!   and revives (the ghost returns), teleport scatters what stands there within 10 tiles,
//!   safety sends to a random station floor, door turns a wall into an airlock or opens a door.
// shortcut: magic missile's homing and trails, the lightning chain's beams, blink's and
// teleport's harmless smoke, the jaunt's steam and visuals are not ported;
// targets are in range, not in `view()`. The jaunter stays dense (others bump into it) and
// explosions still reach it. The forcefield blocks its caster too (no `CanAllowThrough`). The
// door bolt's door is a wooden airlock (no mineral doors). Unmarking summon item is instant
// (tg: a 5 s `do_after`).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, ProjectileData};
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::{Appearance, Dir, Facing, MouseOpacity, TilePos};
use ss13_world::{Density, SpatialIndex};

use super::{
    CastOn, MAGIC_RESISTANCE, Spell, Spells, block_effects, can_block_magic, can_cast, chat_to,
    checked, feedback, no_teleport, now, start_cooldown,
};
use crate::click::ClickResolved;
use crate::combat::{CLICK_CD_MELEE, CombatRng, NextMove};
use crate::health::{DamageType, Health, Stat, apply_damage};
use crate::inventory::{CarriedBy, Inventory, NoDrop};
use crate::medical::name_of;
use crate::projectile::{PASSTABLE, Projectile, Shot};
use crate::storage::StoredIn;
use crate::zone::BodyZone;

/// `jaunt_duration`, the reappearing (`2.5 SECONDS` of `stop_jaunt`), `movespeed`.
pub const JAUNT_SECS: f64 = 5.0;
pub const JAUNT_REAPPEAR_SECS: f64 = 2.5;
pub const JAUNT_STEP_SECS: f64 = 0.2;
/// `/obj/effect/forcefield` `initial_duration`.
pub const FORCEFIELD_SECS: f64 = 30.0;
const FORCEFIELD: &str = "/obj/effect/forcefield/wizard";
/// Lightning bolt's `bolt_range`, `bolt_power`; `tesla_zap`'s cutoff.
const BOLT_RANGE: u32 = 15;
const BOLT_POWER: f64 = 2e4;
const ZAP_CUTOFF: f64 = 1e3;
const WOOD_AIRLOCK: &str = "/obj/machinery/door/airlock/wood";
const PLATING: &str = "/turf/open/floor/plating";

/// A spell's hand item (`/obj/item/melee/touch_attack`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct TouchHand {
    pub owner: Entity,
    pub spell: Spell,
}

/// Summon item's mark on its item (`marked_item`): a despawned and reused entity has none.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SummonMark {
    pub owner: Entity,
}

/// The ethereal jaunt in progress (the mob in tg's `phased_mob/spell_jaunt`).
#[derive(Component, Clone, Debug)]
pub struct Jaunting {
    pub start: TilePos,
    pub until: f64,
    /// `exit_point_list`: the last five unblocked tiles passed, newest first.
    pub exit_points: Vec<TilePos>,
    /// `reappearing`: the end of the exit.
    pub reappear_at: Option<f64>,
    pub next_step: f64,
    /// The look to restore.
    pub color: Option<[u8; 4]>,
    pub mouse_opacity: MouseOpacity,
}

/// A step of a jaunting mob, taken from its client's queue (`relaymove` of the holder).
#[derive(Message, Clone, Copy, Debug)]
pub struct JauntStep {
    pub mob: Entity,
    pub dir: Dir,
}

/// The magic projectiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoltKind {
    Nothing,
    MagicMissile,
    Fireball,
    Lightning,
    Death,
    Resurrection,
    Teleport,
    Safety,
    Door,
}

/// On a flying magic projectile: what its hit does (read by `crate::projectile`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct MagicBolt {
    pub kind: BoltKind,
    /// `antimagic_flags`.
    pub antimagic: u32,
    /// `can_only_hit_target`: it passes through everything but what it was fired at.
    pub only_target: bool,
}

/// A wizard's forcefield, gone at `until`.
#[derive(Component, Clone, Copy, Debug)]
pub struct Forcefield {
    pub until: f64,
}

// ---------------------------------------------------------------------------------------------
// Helpers

fn hurt(world: &mut World, mob: Entity, kind: DamageType, amount: f32) {
    let (Some(mut health), Some(mut stat)) = (
        world.get::<Health>(mob).cloned(),
        world.get::<Stat>(mob).copied(),
    ) else {
        return;
    };
    let changed = apply_damage(&mut health, &mut stat, kind, amount);
    world.entity_mut(mob).insert(health);
    if changed.is_some() {
        world.entity_mut(mob).insert(stat);
    }
}

fn play(world: &mut World, pos: TilePos, sound: &str) {
    crate::sound::playsound(world, crate::sound::PlaySound::new(pos, sound, 50.0, true));
}

fn living(world: &World, e: Entity) -> bool {
    world.get::<Health>(e).is_some()
}

fn rng_range(world: &mut World, low: u32, high: u32) -> u32 {
    world.resource_mut::<CombatRng>().range(low, high)
}

fn pick<T: Copy>(world: &mut World, list: &[T]) -> Option<T> {
    if list.is_empty() {
        return None;
    }
    let i = rng_range(world, 0, list.len() as u32 - 1) as usize;
    list.get(i).copied()
}

/// `do_teleport(..., forced)` of a movable: off the map or out of a CentCom area without
/// `forced`, nothing.
fn teleport(world: &mut World, mover: Entity, dest: TilePos, forced: bool) -> bool {
    let Some(&from) = world.get::<TilePos>(mover) else {
        return false;
    };
    if !world.resource::<SpatialIndex>().in_bounds(dest)
        || (!forced && (no_teleport(world, from) || no_teleport(world, dest)))
    {
        return false;
    }
    world.entity_mut(mover).insert(dest);
    true
}

/// The hands of `mob`: `item` into an empty one, the active first (`put_in_hands`).
fn put_in_hands(world: &mut World, mob: Entity, item: Entity) -> bool {
    let Some(inventory) = world.get::<Inventory>(mob) else {
        return false;
    };
    let hand = [inventory.active, 1 - inventory.active]
        .into_iter()
        .find(|&h| inventory.hands[h].is_none());
    let Some(hand) = hand else {
        return false;
    };
    world
        .entity_mut(item)
        .remove::<(TilePos, StoredIn)>()
        .insert(CarriedBy(mob));
    if let Some(mut inv) = world.get_mut::<Inventory>(mob) {
        inv.hands[hand] = Some(item);
    }
    true
}

/// tg `flash_act()` of the living around `center`, the `except` one left out.
fn flash_around(world: &mut World, center: TilePos, except: Entity) {
    let mobs: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(center, 7)
        .filter(|&e| e != except && living(world, e))
        .collect();
    for mob in mobs {
        crate::vision::flash_act(world, mob, 1, false, false, 2.5);
    }
}

/// `do_sparks(sparks_amt, ...)`: one spark visual for the burst.
fn sparks(world: &mut World, at: Entity) {
    crate::emp::sparks(world, at);
}

// ---------------------------------------------------------------------------------------------
// Casting

/// `cast()` of `spell` at `level`. False when it had no effect that starts the cooldown.
pub(super) fn cast(
    world: &mut World,
    _client: Entity,
    mob: Entity,
    spell: Spell,
    level: u8,
    on: CastOn,
) -> bool {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return false;
    };
    match (spell, on) {
        (Spell::MagicMissile, _) => {
            magic_missile(world, mob, pos);
            true
        }
        (
            Spell::Fireball,
            CastOn::Atom {
                target, pos: at, ..
            },
        ) => {
            fire_bolt(world, mob, pos, Some(target), at, BoltKind::Fireball, level);
            true
        }
        (
            Spell::LightningBolt,
            CastOn::Atom {
                target, pos: at, ..
            },
        ) => {
            fire_bolt(
                world,
                mob,
                pos,
                Some(target),
                at,
                BoltKind::Lightning,
                level,
            );
            true
        }
        (Spell::Blind, CastOn::Atom { target, .. }) => {
            blind(world, mob, target);
            true
        }
        (Spell::DisableTech, _) => {
            world.write_message(crate::emp::Empulse {
                epicenter: pos,
                heavy: 6,
                light: 10,
            });
            true
        }
        (Spell::Repulse, _) => {
            repulse(world, mob, pos);
            true
        }
        (Spell::Smoke, _) => {
            // `after_cast`: `do_smoke(4, ...)` of the bad smoke.
            crate::grenades::do_smoke(world, pos, 4);
            true
        }
        (Spell::Forcewall, _) => {
            forcewall(world, mob, pos);
            true
        }
        (Spell::Knock, _) => {
            knock(world, pos);
            true
        }
        (Spell::Blink, _) => {
            blink(world, mob, pos);
            true
        }
        (Spell::Teleport, CastOn::Area(area)) => area_teleport(world, mob, area),
        (Spell::EtherealJaunt, _) => {
            start_jaunt(world, mob, pos);
            true
        }
        (Spell::Swap, CastOn::Atom { target, .. }) => {
            swap(world, mob, target);
            true
        }
        (Spell::SummonItem, _) => {
            summon_item(world, mob);
            true
        }
        (Spell::Charge, _) => {
            charge(world, mob);
            true
        }
        _ => false,
    }
}

/// `aoe/magic_missile/cast`: a missile at every other living mob around.
fn magic_missile(world: &mut World, mob: Entity, pos: TilePos) {
    let mut targets: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(pos, 7)
        .filter(|&e| e != mob && living(world, e))
        .collect();
    targets.sort();
    for target in targets {
        let Some(&at) = world.get::<TilePos>(target) else {
            continue;
        };
        fire_bolt(world, mob, pos, Some(target), at, BoltKind::MagicMissile, 1);
    }
}

/// The `/obj/projectile/magic/*` vars of a bolt (tg's `/obj/projectile` defaults otherwise).
pub fn bolt_data(kind: BoltKind, level: u8) -> ProjectileData {
    let base = ProjectileData {
        damage: 0.0,
        damage_type: "oxy".into(),
        armour_penetration: 100.0,
        speed: 1.25,
        range: 50.0,
        pass_flags: PASSTABLE,
        accurate_range: 100.0,
        accuracy_falloff: 7.0,
        ..default()
    };
    match kind {
        BoltKind::Nothing => ProjectileData {
            path: "/obj/projectile/magic/nothing".into(),
            ..base
        },
        BoltKind::MagicMissile => ProjectileData {
            path: "/obj/projectile/magic/aoe/magic_missile".into(),
            range: 100.0,
            speed: 0.2,
            paralyze: 60.0,
            ..base
        },
        BoltKind::Fireball => ProjectileData {
            path: "/obj/projectile/magic/fireball".into(),
            damage: 10.0,
            damage_type: "burn".into(),
            // `fireball/ready_projectile`.
            range: 6.0 + 2.0 * f32::from(level.max(1)),
            ..base
        },
        BoltKind::Lightning => ProjectileData {
            path: "/obj/projectile/magic/aoe/lightning".into(),
            damage: 15.0,
            damage_type: "burn".into(),
            speed: 3.5,
            ..base
        },
        BoltKind::Death => ProjectileData {
            path: "/obj/projectile/magic/death".into(),
            ..base
        },
        BoltKind::Resurrection => ProjectileData {
            path: "/obj/projectile/magic/resurrection".into(),
            ..base
        },
        BoltKind::Teleport => ProjectileData {
            path: "/obj/projectile/magic/teleport".into(),
            ..base
        },
        BoltKind::Safety => ProjectileData {
            path: "/obj/projectile/magic/safety".into(),
            ..base
        },
        BoltKind::Door => ProjectileData {
            path: "/obj/projectile/magic/door".into(),
            ..base
        },
    }
}

/// `fire_projectile` of a spell or a staff: the bolt from `from` at `target` / `at`.
pub fn fire_bolt(
    world: &mut World,
    firer: Entity,
    from: TilePos,
    target: Option<Entity>,
    at: TilePos,
    kind: BoltKind,
    level: u8,
) -> Option<Entity> {
    let data = bolt_data(kind, level);
    let bolt = MagicBolt {
        kind,
        antimagic: MAGIC_RESISTANCE,
        only_target: kind == BoltKind::MagicMissile,
    };
    crate::projectile::fire_with(
        world,
        Shot {
            data: &data,
            firer,
            from,
            target,
            target_pos: at,
            zone: BodyZone::Chest,
            hit_prone: false,
            pixel: [0, 0],
            deviation: 0.0,
            fired_from: None,
        },
        bolt,
    )
}

/// `/obj/projectile/magic/prehit_pierce`: an antimagic mob makes the bolt fizzle. True when the
/// hit is cancelled.
pub fn bolt_blocked(world: &mut World, bolt: &MagicBolt, me: Entity, target: Entity) -> bool {
    if !living(world, target) || !can_block_magic(world, target, bolt.antimagic) {
        return false;
    }
    let (name, victim) = (name_of(world, me), name_of(world, target));
    if let Some(&pos) = world.get::<TilePos>(target) {
        crate::security::show(
            world,
            pos,
            crate::health::COMBAT_MESSAGE_RANGE,
            ChatChannel::Combat,
            (
                Span::Warning,
                format!("{name} fizzles on contact with {victim}!"),
            ),
            &[],
        );
    }
    block_effects(world, target);
    true
}

/// `on_hit` of the magic bolts, after the hit's damage (`crate::projectile`).
pub fn bolt_hit(world: &mut World, bolt: &MagicBolt, p: &Projectile, target: Entity) {
    apply_bolt(world, bolt.kind, p.firer, target);
}

/// What a bolt of `kind` from `firer` does to `target` (also a wand zapped at its user).
pub fn apply_bolt(world: &mut World, kind: BoltKind, firer: Entity, target: Entity) {
    let Some(&at) = world.get::<TilePos>(target) else {
        return;
    };
    match kind {
        BoltKind::Nothing | BoltKind::MagicMissile => {}
        BoltKind::Fireball => {
            // "between this 10 burn, the 10 brute, the explosion brute, and the onfire burn".
            if living(world, target) {
                hurt(world, target, DamageType::Burn, 10.0);
            }
            crate::explosions::explosion(
                world,
                at,
                crate::explosions::Explosion::new(-1, 0, 2)
                    .flame(2)
                    .flash(3)
                    .cause("fireball"),
            );
        }
        BoltKind::Lightning => {
            let mut shocked = vec![firer];
            tesla_zap(world, at, BOLT_RANGE, BOLT_POWER, &mut shocked, 0);
        }
        BoltKind::Death => {
            if living(world, target) && world.get::<Stat>(target) != Some(&Stat::Dead) {
                info!("{target} killed by a bolt of death from {firer:?}");
                crate::medical::organs::kill(world, target);
            }
        }
        BoltKind::Resurrection => {
            if living(world, target) {
                let was_dead = world.get::<Stat>(target) == Some(&Stat::Dead);
                full_heal(world, target);
                if was_dead {
                    crate::ghost::return_to_body(world, target);
                    chat_to(
                        world,
                        target,
                        Span::Notice,
                        "You rise with a start, you're alive!!!",
                    );
                } else {
                    chat_to(world, target, Span::Notice, "Вы чувствуете себя прекрасно!");
                }
            }
        }
        BoltKind::Teleport => teleport_bolt(world, at),
        BoltKind::Safety => {
            let movable =
                living(world, target) || world.get::<DmItem>(target).is_some_and(|i| !i.anchored);
            if movable
                && let Some(dest) = crate::events::electrical_storm::random_station_turf(world)
            {
                teleport(world, target, dest, false);
            }
        }
        BoltKind::Door => door_bolt(world, target, at),
    }
}

/// `tesla_zap(source, zap_range, power, cutoff, ZAP_MOB_DAMAGE)` over living mobs only.
pub fn tesla_zap(
    world: &mut World,
    from: TilePos,
    range: u32,
    power: f64,
    shocked: &mut Vec<Entity>,
    depth: u32,
) {
    if power < ZAP_CUTOFF || depth > 16 {
        return;
    }
    // `oview(zap_range + 2)`: the closest living mob not shocked yet nor shock immune.
    let mut best: Option<(u32, Entity)> = None;
    let candidates: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(from, range + 2)
        .collect();
    for e in candidates {
        if shocked.contains(&e)
            || !living(world, e)
            || world.get::<Stat>(e) == Some(&Stat::Dead)
            // `TRAIT_TESLA_SHOCKIMMUNE` of a lightning bolt's caster.
            || super::knows(world, e, Spell::LightningBolt)
        {
            continue;
        }
        let Some(d) = world.get::<TilePos>(e).and_then(|p| p.chebyshev(from)) else {
            continue;
        };
        if best.is_none_or(|(bd, be)| d < bd || (d == bd && e < be)) {
            best = Some((d, e));
        }
    }
    let Some((_, target)) = best else {
        return;
    };
    shocked.push(target);
    let spread = rng_range(world, 0, 10) as f32 - 5.0;
    let damage = ((power / 600.0).round().min(90.0) as f32 + spread).max(0.0)
        * crate::species::def_of(world, target).siemens_coeff;
    hurt(world, target, DamageType::Burn, damage);
    let Some(&at) = world.get::<TilePos>(target) else {
        return;
    };
    let victim = name_of(world, target);
    crate::security::show(
        world,
        at,
        crate::health::COMBAT_MESSAGE_RANGE,
        ChatChannel::Combat,
        (Span::Danger, format!("Молния ударяет током {victim}!")),
        &[(target, Span::Danger, "Вас бьёт молнией!".to_owned())],
    );
    let power = power / 1.5;
    if world.resource_mut::<CombatRng>().chance(20.0) {
        let mut copy = shocked.clone();
        tesla_zap(world, at, 5, power * 0.5, &mut copy, depth + 1);
        tesla_zap(world, at, 5, power * 0.5, shocked, depth + 1);
        for e in copy {
            if !shocked.contains(&e) {
                shocked.push(e);
            }
        }
    } else {
        tesla_zap(world, at, 5, power, shocked, depth + 1);
    }
}

/// `revive(ADMIN_HEAL_ALL)`: every damage, organ, limb, wound, the blood and the status effects
/// healed; a dead mob gets up.
pub fn full_heal(world: &mut World, mob: Entity) {
    use crate::medical::blood::{BLOOD_VOLUME_NORMAL, Blood};
    use crate::status_effects::{Incap, effects_mut};
    crate::medical::organs::regenerate(world, mob);
    crate::medical::bodyparts::regenerate_limbs(world, mob);
    crate::medical::organs::set_heartattack(world, mob, false);
    if let Some(mut blood) = world.get_mut::<Blood>(mob)
        && blood.volume < BLOOD_VOLUME_NORMAL
    {
        blood.volume = BLOOD_VOLUME_NORMAL;
    }
    if let Some(mut limbs) = world.get_mut::<crate::medical::wounds::Limbs>(mob)
        && limbs.parts.iter().any(|l| !l.wounds.is_empty())
    {
        for limb in &mut limbs.parts {
            limb.wounds.clear();
        }
    }
    if let Some(health) = world.get::<Health>(mob).cloned() {
        world.entity_mut(mob).insert(health.healed());
    }
    if let Some(mut effects) = effects_mut(world, mob) {
        for kind in [
            Incap::Stun,
            Incap::Knockdown,
            Incap::Paralyze,
            Incap::Immobilize,
        ] {
            effects.set(kind, 0.0);
        }
        effects.stamina = 0.0;
        effects.stamcrit = false;
    }
    crate::vision::cure_temp_blindness(world, mob);
    crate::vision::cure_eye_blur(world, mob);
    world.entity_mut(mob).insert(Stat::Conscious);
    if let Some(mut alerts) = world.get_mut::<crate::breath::Alerts>(mob) {
        alerts.0.clear();
    }
}

/// `/obj/projectile/magic/teleport/on_hit`: everything unanchored on the tile, within 10.
fn teleport_bolt(world: &mut World, at: TilePos) {
    let stuff: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .on_tile(at)
        .iter()
        .copied()
        .filter(|&e| {
            world.get::<crate::ghost::Ghost>(e).is_none()
                && world.get::<Projectile>(e).is_none()
                && (living(world, e) || world.get::<DmItem>(e).is_some_and(|i| !i.anchored))
        })
        .collect();
    for thing in stuff {
        if let Some(dest) = crate::events::wormholes::teleport_turf(world, at, 10) {
            teleport(world, thing, dest, false);
        }
    }
}

/// `/obj/projectile/magic/door/on_hit`: a door opens (unbolted), a wall becomes a door.
fn door_bolt(world: &mut World, target: Entity, at: TilePos) {
    if world.get::<crate::doors::Door>(target).is_some() {
        open_door(world, target);
        return;
    }
    let Some(turf) = crate::construction::turf_at(world, at) else {
        return;
    };
    let closed = world.get::<Density>(turf).is_some()
        && crate::construction::path_of(world, turf).is_some_and(|p| {
            is_under(&p, "/turf/closed") && !is_under(&p, "/turf/closed/indestructible")
        });
    if !closed {
        return;
    }
    let baseturfs = crate::construction::baseturfs_of(world, turf);
    if crate::construction::change_turf(world, at, PLATING, baseturfs, true).is_some() {
        crate::construction::spawn_obj(world, WOOD_AIRLOCK, at);
    }
}

/// `COMSIG_ATOM_MAGICALLY_UNLOCKED` of a door: unbolted and opened.
fn open_door(world: &mut World, door: Entity) {
    crate::doors::with_door(world, door, |door, appearance, ctx| {
        door.locked = false;
        let opened = ctx.open(door, appearance, true);
        ((), opened)
    });
}

/// `blind/cast`.
fn blind(world: &mut World, mob: Entity, target: Entity) {
    if can_block_magic(world, target, MAGIC_RESISTANCE) {
        chat_to(
            world,
            target,
            Span::Notice,
            "Your eye itches, but it passes momentarily.",
        );
        chat_to(world, mob, Span::Warning, "The spell had no effect!");
        return;
    }
    chat_to(world, target, Span::Warning, "Your eyes cry out in pain!");
    crate::vision::adjust_temp_blindness(world, target, 20.0);
    crate::vision::set_eye_blur_if_lower(world, target, 40.0);
}

/// `aoe/repulse/cast_on_thing_in_aoe` over the unanchored items and mobs within 5 tiles.
fn repulse(world: &mut World, mob: Entity, center: TilePos) {
    const MAX_THROW: i32 = 5;
    let mut things: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(center, 5)
        .filter(|&e| {
            e != mob
                && world.get::<Projectile>(e).is_none()
                && world.get::<crate::ghost::Ghost>(e).is_none()
                && (living(world, e) || world.get::<DmItem>(e).is_some_and(|i| !i.anchored))
        })
        .collect();
    things.sort();
    let caster = name_of(world, mob);
    for victim in things {
        let Some(&pos) = world.get::<TilePos>(victim) else {
            continue;
        };
        let is_living = living(world, victim);
        if is_living && can_block_magic(world, victim, MAGIC_RESISTANCE) {
            block_effects(world, victim);
            continue;
        }
        let dist = pos.chebyshev(center).unwrap_or(0) as i32;
        if dist == 0 {
            if is_living {
                crate::status_effects::apply(
                    world,
                    victim,
                    crate::status_effects::Incap::Paralyze,
                    10.0,
                );
                hurt(world, victim, DamageType::Brute, 5.0);
                chat_to(
                    world,
                    victim,
                    Span::Danger,
                    format!("You're slammed into the floor by {caster}!"),
                );
            }
            continue;
        }
        if is_living {
            crate::status_effects::apply(
                world,
                victim,
                crate::status_effects::Incap::Paralyze,
                4.0,
            );
            chat_to(
                world,
                victim,
                Span::Danger,
                format!("You're thrown back by {caster}!"),
            );
        }
        let range = (MAX_THROW - (dist - 2).clamp(0, dist)).clamp(3, MAX_THROW);
        let (dx, dy) = ((pos.x - center.x).signum(), (pos.y - center.y).signum());
        let target = TilePos::new(pos.x + dx * range, pos.y + dy * range, pos.z);
        crate::throw::throw_at(world, victim, target, range as u32, 1.0);
    }
}

/// `forcewall/cast`: the caster's tile and the two beside it, across its facing.
fn forcewall(world: &mut World, mob: Entity, pos: TilePos) {
    let (dx, dy) = world
        .get::<Facing>(mob)
        .map_or(Dir::South, |f| f.0)
        .offset();
    let until = now(world) + FORCEFIELD_SECS;
    for tile in [
        pos,
        TilePos::new(pos.x - dy, pos.y + dx, pos.z),
        TilePos::new(pos.x + dy, pos.y - dx, pos.z),
    ] {
        if !world.resource::<SpatialIndex>().in_bounds(tile) {
            continue;
        }
        if let Some(wall) = crate::construction::spawn_obj(world, FORCEFIELD, tile) {
            world
                .entity_mut(wall)
                .insert((Forcefield { until }, Density));
        }
    }
}

/// `aoe/knock`: doors and closets within 3 tiles.
fn knock(world: &mut World, center: TilePos) {
    let things: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(center, 3)
        .collect();
    for thing in things {
        if world.get::<crate::doors::Door>(thing).is_some() {
            open_door(world, thing);
            continue;
        }
        let Some(mut closet) = world.get_mut::<crate::closets::Closet>(thing) else {
            continue;
        };
        closet.locked = false;
        crate::closets::open(world, thing);
        crate::closets::refresh_look(world, thing);
    }
}

/// `radius_turf/blink`: a random tile within 6 (not at the map's edge).
fn blink(world: &mut World, mob: Entity, pos: TilePos) {
    const RADIUS: i32 = 6;
    let Some((w, h)) = world.resource::<SpatialIndex>().level_size(pos.z) else {
        return;
    };
    let mut tiles = Vec::new();
    for dy in -RADIUS..=RADIUS {
        for dx in -RADIUS..=RADIUS {
            let t = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
            // `is_valid_destination`: not within `outer_tele_radius` of the edge.
            if t.x < RADIUS || t.y < RADIUS || t.x > w as i32 - RADIUS || t.y > h as i32 - RADIUS {
                continue;
            }
            tiles.push(t);
        }
    }
    let Some(dest) = pick(world, &tiles) else {
        return;
    };
    if teleport(world, mob, dest, false) {
        play(world, dest, "sound/effects/magic/blink.ogg");
    }
}

/// The free tiles of `area` (`TELEPORT_SPELL_SKIP_BLOCKED`: dense things but mobs block).
// Cost: one pass over the area table of the station levels (~65k tiles) per cast.
fn area_tiles(world: &World, area: Entity) -> Vec<TilePos> {
    let index = world.resource::<SpatialIndex>();
    let areas = world.resource::<ss13_mapload::Areas>();
    let mut out = Vec::new();
    for z in crate::events::controller::station_levels(world) {
        let Some((w, h)) = index.level_size(z) else {
            continue;
        };
        for y in 1..=h as i32 {
            for x in 1..=w as i32 {
                let pos = TilePos::new(x, y, z);
                if areas.area_at(pos) != Some(area) {
                    continue;
                }
                if index
                    .dense_on_tile(pos)
                    .iter()
                    .all(|&e| world.get::<Health>(e).is_some())
                {
                    out.push(pos);
                }
            }
        }
    }
    out
}

/// `area_teleport/cast`: forced onto a random free tile of `area`. False without one.
fn area_teleport(world: &mut World, mob: Entity, area: Entity) -> bool {
    let tiles = area_tiles(world, area);
    let Some(dest) = pick(world, &tiles) else {
        return false;
    };
    if !teleport(world, mob, dest, true) {
        return false;
    }
    play(world, dest, "sound/effects/magic/teleport_app.ogg");
    true
}

/// A scroll of teleportation's spell (`area_teleport/wizard/scroll`): held, a use spent; the
/// last one crumbles it.
pub(super) fn scroll_teleport(world: &mut World, mob: Entity, scroll: Entity, area: Entity) {
    let held = world
        .get::<Inventory>(mob)
        .is_some_and(|i| i.hands.contains(&Some(scroll)));
    if !held
        || world.get::<Stat>(mob) != Some(&Stat::Conscious)
        || crate::status_effects::incapacitation(world, mob).incapacitated
    {
        return;
    }
    let Some(uses) = world.get::<TeleportScroll>(scroll).map(|s| s.uses) else {
        return;
    };
    if uses == 0 || !area_teleport(world, mob, area) {
        return;
    }
    let left = uses - 1;
    if left > 0 {
        if let Some(mut s) = world.get_mut::<TeleportScroll>(scroll) {
            s.uses = left;
        }
        return;
    }
    let name = name_of(world, scroll);
    chat_to(
        world,
        mob,
        Span::Warning,
        format!("{name} runs out of uses and crumbles to dust!"),
    );
    world.despawn(scroll);
}

/// `/obj/item/teleportation_scroll`: its `uses`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct TeleportScroll {
    pub uses: u32,
}

/// `swap/cast`.
fn swap(world: &mut World, mob: Entity, target: Entity) {
    let second = world
        .get_mut::<Spells>(mob)
        .and_then(|mut s| s.second_target.take());
    if can_block_magic(world, target, MAGIC_RESISTANCE) {
        chat_to(world, mob, Span::Warning, "The spell had no effect!");
        chat_to(
            world,
            target,
            Span::Warning,
            "You feel space bending, but it rapidly dissipates.",
        );
        return;
    }
    chat_to(world, target, Span::Danger, "You feel space bending.");
    let (Some(&here), Some(&there)) = (world.get::<TilePos>(mob), world.get::<TilePos>(target))
    else {
        return;
    };
    let second = second.filter(|&s| {
        s != target
            && living(world, s)
            && world
                .get::<TilePos>(s)
                .and_then(|p| p.chebyshev(here))
                .is_some_and(|d| d <= 9)
    });
    const SWAP: &str = "sound/effects/magic/swap.ogg";
    match second.and_then(|s| Some((s, *world.get::<TilePos>(s)?))) {
        Some((second, second_pos)) => {
            chat_to(world, second, Span::Danger, "You feel space bending.");
            world.entity_mut(second).insert(here);
            world.entity_mut(target).insert(second_pos);
            world.entity_mut(mob).insert(there);
            play(world, here, SWAP);
            play(world, second_pos, SWAP);
        }
        None => {
            world.entity_mut(target).insert(here);
            world.entity_mut(mob).insert(there);
            play(world, here, SWAP);
        }
    }
    play(world, there, SWAP);
}

/// `swap/InterceptClickOn` with the right button: the secondary target.
pub(super) fn mark_swap_target(world: &mut World, mob: Entity, target: Entity, pos: TilePos) {
    if !checked(world, mob, Spell::Swap) {
        return;
    }
    let target = if living(world, target) {
        Some(target)
    } else {
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| living(world, e))
    };
    let Some(target) = target else {
        chat_to(
            world,
            mob,
            Span::Warning,
            "You can only select living beings as secondary target!",
        );
        return;
    };
    if target == mob {
        let had = world
            .get_mut::<Spells>(mob)
            .and_then(|mut s| s.second_target.take())
            .is_some();
        let text = if had {
            "You cancel your secondary swap target!"
        } else {
            "You have no secondary swap target!"
        };
        chat_to(world, mob, Span::Notice, text);
        return;
    }
    if let Some(mut spells) = world.get_mut::<Spells>(mob) {
        spells.second_target = Some(target);
    }
    let name = name_of(world, target);
    chat_to(
        world,
        mob,
        Span::Notice,
        format!("You select {name} as a secondary swap target!"),
    );
}

/// `summonitem/cast`: mark the held item, unmark it, or recall it.
fn summon_item(world: &mut World, mob: Entity) {
    let marked = world
        .get::<Spells>(mob)
        .and_then(|s| s.get(Spell::SummonItem))
        .and_then(|k| k.marked)
        .filter(|&e| world.get::<SummonMark>(e).is_some_and(|m| m.owner == mob));
    let active = world.get::<Inventory>(mob).and_then(Inventory::active_item);
    let set_mark = |world: &mut World, item: Option<Entity>| {
        if let Some(mut spells) = world.get_mut::<Spells>(mob)
            && let Some(known) = spells.get_mut(Spell::SummonItem)
        {
            known.marked = item;
        }
    };
    let Some(item) = marked else {
        // `try_link_item`.
        let Some(held) = active.filter(|&e| world.get::<TouchHand>(e).is_none()) else {
            let other = world
                .get::<Inventory>(mob)
                .is_some_and(|i| i.hands.iter().flatten().next().is_some());
            let text = if other {
                "You must hold the desired item in your hands to mark it for recall!"
            } else {
                "You aren't holding anything that can be marked for recall!"
            };
            chat_to(world, mob, Span::Warning, text);
            return;
        };
        let name = name_of(world, held);
        let mut text = String::new();
        if world.get::<NoDrop>(held).is_some() {
            text.push_str("Though it feels redundant... ");
        }
        text.push_str(&format!("You mark {name} for recall."));
        chat_to(world, mob, Span::Notice, text);
        world.entity_mut(held).insert(SummonMark { owner: mob });
        set_mark(world, Some(held));
        return;
    };
    if active == Some(item) {
        // `try_unlink_item`.
        let name = name_of(world, item);
        world.entity_mut(item).remove::<SummonMark>();
        set_mark(world, None);
        chat_to(
            world,
            mob,
            Span::Notice,
            format!("You remove the mark on {name} to use elsewhere."),
        );
        return;
    }
    recall(world, mob, item);
}

/// `try_recall_item`: up the containers (an unanchored one comes along), out of a holder's hands.
fn recall(world: &mut World, mob: Entity, item: Entity) {
    let mut thing = item;
    for _ in 0..10 {
        let Some(container) = world.get::<StoredIn>(thing).map(|s| s.0) else {
            break;
        };
        let anchored = world
            .get::<crate::obj_damage::Anchored>(container)
            .is_some_and(|a| a.0);
        if anchored {
            break;
        }
        thing = container;
    }
    let Some(&dest) = world.get::<TilePos>(mob) else {
        return;
    };
    if let Some(holder) = world.get::<CarriedBy>(thing).map(|c| c.0) {
        if holder == mob {
            return;
        }
        if world.get::<NoDrop>(thing).is_some() {
            return;
        }
        world.entity_mut(thing).remove::<CarriedBy>();
        crate::inventory::refresh_look(world, holder);
    } else if world.get::<StoredIn>(thing).is_some() {
        crate::storage::take_out(world, thing, dest);
    }
    let name = name_of(world, thing);
    if world.get::<DmItem>(thing).is_some() && put_in_hands(world, mob, thing) {
        crate::inventory::refresh_look(world, mob);
        let owner = name_of(world, mob);
        crate::medical::visible(
            world,
            mob,
            &format!("{name} suddenly appears in your hand!"),
            &format!("{name} suddenly appears in {owner}'s hand!"),
            Span::Warning,
        );
    } else {
        world.entity_mut(thing).insert(dest);
        crate::medical::visible(
            world,
            mob,
            &format!("{name} suddenly appears!"),
            &format!("{name} suddenly appears!"),
            Span::Warning,
        );
    }
    play(world, dest, "sound/effects/magic/summonitems_generic.ogg");
}

/// `charge/cast`: a pulled magic user's spells, else the held item.
fn charge(world: &mut World, mob: Entity) {
    if let Some(pulled) = world.get::<crate::pull::Pulling>(mob).map(|p| p.0)
        && living(world, pulled)
    {
        let has_spells = world
            .get::<Spells>(pulled)
            .is_some_and(|s| !s.known.is_empty());
        if has_spells {
            if let Some(mut spells) = world.get_mut::<Spells>(pulled) {
                for known in &mut spells.known {
                    known.ready_at = 0.0;
                }
            }
            super::refresh(world, pulled);
            chat_to(
                world,
                pulled,
                Span::Notice,
                "You feel raw magic flowing through you. It feels good!",
            );
            let name = name_of(world, pulled);
            chat_to(
                world,
                mob,
                Span::Notice,
                format!("{name} suddenly feels very warm!"),
            );
            return;
        }
        chat_to(
            world,
            pulled,
            Span::Notice,
            "You feel very strange for a moment, but then it passes.",
        );
    }
    let held = world
        .get::<Inventory>(mob)
        .and_then(|i| i.active_item().or(i.hands[1 - i.active]));
    let Some(item) = held else {
        chat_to(
            world,
            mob,
            Span::Notice,
            "You feel magical power surging through your hands, but the feeling rapidly fades.",
        );
        return;
    };
    let name = name_of(world, item);
    let text = match super::magic_guns::magically_charge(world, item) {
        super::magic_guns::Charged::BurntOut => {
            format!("{name} seems to react negatively to Charge, becoming uncomfortably warm!")
        }
        super::magic_guns::Charged::Charged => format!("{name} suddenly feels very warm!"),
        super::magic_guns::Charged::Nothing => {
            format!("{name} doesn't seem to be react to Charge.")
        }
    };
    chat_to(world, mob, Span::Notice, text);
}

// ---------------------------------------------------------------------------------------------
// Touch spells

/// `touch/Activate`: the hand into an empty hand, or the hand back.
pub(super) fn toggle_hand(world: &mut World, mob: Entity, spell: Spell) {
    let super::Targeting::Touch { hand: hand_path } = spell.def().targeting else {
        return;
    };
    let current = world
        .get::<Spells>(mob)
        .and_then(|s| s.get(spell))
        .and_then(|k| k.hand)
        .filter(|&h| world.get::<TouchHand>(h).is_some_and(|t| t.owner == mob));
    if let Some(hand) = current {
        remove_hand(world, mob, hand);
        chat_to(
            world,
            mob,
            Span::Notice,
            "You draw the power out of your hand.",
        );
        return;
    }
    if !super::ready(world, mob, spell) || !checked(world, mob, spell) {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let Some(hand) = crate::construction::spawn_obj(world, hand_path, pos) else {
        return;
    };
    world
        .entity_mut(hand)
        .insert((NoDrop, TouchHand { owner: mob, spell }));
    if !put_in_hands(world, mob, hand) {
        world.despawn(hand);
        crate::construction::tile_changed(world, pos);
        chat_to(world, mob, Span::Warning, "Your hands are full!");
        return;
    }
    crate::construction::tile_changed(world, pos);
    crate::inventory::refresh_look(world, mob);
    if let Some(mut spells) = world.get_mut::<Spells>(mob)
        && let Some(known) = spells.get_mut(spell)
    {
        known.hand = Some(hand);
    }
    chat_to(
        world,
        mob,
        Span::Notice,
        "You channel the power of the spell to your hand.",
    );
    super::refresh(world, mob);
}

/// `remove_hand`.
pub(super) fn remove_hand(world: &mut World, mob: Entity, hand: Entity) {
    if let Some(mut inv) = world.get_mut::<Inventory>(mob) {
        for h in &mut inv.hands {
            if *h == Some(hand) {
                *h = None;
            }
        }
    }
    if world.get_entity(hand).is_ok() {
        world.despawn(hand);
    }
    crate::inventory::refresh_look(world, mob);
    if let Some(mut spells) = world.get_mut::<Spells>(mob) {
        for known in &mut spells.known {
            if known.hand == Some(hand) {
                known.hand = None;
            }
        }
    }
    super::refresh(world, mob);
}

/// A touch hand on a living mob in reach, before melee: `on_hand_hit` -> `do_hand_hit`.
pub(super) fn touch_clicks(world: &mut World, mut clicks: Local<MessageCursor<ClickResolved>>) {
    let clicks: Vec<ClickResolved> = clicks
        .read(world.resource::<Messages<ClickResolved>>())
        .copied()
        .collect();
    for click in clicks {
        let (mob, target) = (click.mob, click.target);
        if world
            .resource::<crate::security::MeleeHandled>()
            .0
            .contains(&mob)
        {
            continue;
        }
        let Some(hand) = world.get::<Inventory>(mob).and_then(Inventory::active_item) else {
            continue;
        };
        let Some(touch) = world.get::<TouchHand>(hand).copied() else {
            continue;
        };
        // `can_hit_with_hand`: a living mob, not the caster (`can_cast_on_self`).
        if touch.owner != mob || target == mob || !living(world, target) {
            continue;
        }
        if world.get::<NextMove>(mob).is_some_and(|n| now(world) < n.0) {
            continue;
        }
        world
            .resource_mut::<crate::security::MeleeHandled>()
            .0
            .insert(mob);
        if can_cast(world, mob, touch.spell).is_err() {
            checked(world, mob, touch.spell);
            continue;
        }
        hand_hit(world, mob, hand, touch.spell, target);
    }
}

/// `do_hand_hit`: antimagic or the spell's effect, the feedback, the cooldown, the hand gone.
fn hand_hit(world: &mut World, mob: Entity, hand: Entity, spell: Spell, victim: Entity) {
    let Some(&center) = world.get::<TilePos>(mob) else {
        return;
    };
    if can_block_magic(world, victim, spell.def().antimagic) {
        // `smite/on_antimagic_triggered`: off goes the arm holding the hand.
        let target = name_of(world, victim);
        let caster = name_of(world, mob);
        let arm = world
            .get::<Inventory>(mob)
            .and_then(|i| i.hands.iter().position(|&h| h == Some(hand)))
            .map(|i| {
                if i == 0 {
                    BodyZone::LArm
                } else {
                    BodyZone::RArm
                }
            });
        let blown =
            arm.is_some_and(|zone| crate::medical::bodyparts::dismember(world, mob, zone, false));
        let others = if blown {
            format!("The feedback blows {caster}'s arm off!")
        } else {
            "The feedback releases a bright flash of light!".to_owned()
        };
        crate::medical::visible(
            world,
            mob,
            &format!("The spell bounces from {target}'s skin back into your arm!"),
            &others,
            Span::Warning,
        );
        crate::vision::flash_act(world, mob, 1, false, false, 2.5);
        sparks(world, mob);
        flash_around(world, center, mob);
    } else {
        match spell {
            Spell::Smite => {
                // `smite/cast_on_hand_hit`.
                sparks(world, victim);
                flash_around(world, center, mob);
                info!("{victim} gibbed by {mob}'s smite");
                crate::health::gib(world, victim);
            }
            _ => return,
        }
    }
    feedback(world, mob, spell, None);
    let t = now(world);
    world.entity_mut(mob).insert(NextMove(t + CLICK_CD_MELEE));
    remove_hand(world, mob, hand);
    start_cooldown(world, mob, spell, None);
}

// ---------------------------------------------------------------------------------------------
// Ethereal jaunt

/// `ethereal_jaunt/do_jaunt` -> `enter_jaunt`.
fn start_jaunt(world: &mut World, mob: Entity, pos: TilePos) {
    let (color, mouse_opacity) = world
        .get::<Appearance>(mob)
        .map_or((None, MouseOpacity::Icon), |a| (a.color, a.mouse_opacity));
    let t = now(world);
    world.entity_mut(mob).insert(Jaunting {
        start: pos,
        until: t + JAUNT_SECS,
        exit_points: Vec::new(),
        reappear_at: None,
        next_step: t,
        color,
        mouse_opacity,
    });
    // `extinguish_mob`.
    let stacks = world
        .get::<crate::fire::FireStacks>(mob)
        .map_or(0.0, |f| f.stacks);
    if stacks > 0.0 {
        crate::fire::adjust_fire_stacks(world, mob, -stacks);
    }
    if world.get::<Spells>(mob).and_then(|s| s.aiming).is_some() {
        super::set_aim(world, mob, None);
    }
    super::refresh(world, mob);
}

/// The jaunter's look: transparent to eyes and clicks while it lasts.
pub(super) fn jaunt_look(
    mut mobs: Query<(&Jaunting, &mut Appearance), Or<(Changed<Appearance>, Added<Jaunting>)>>,
) {
    for (_, mut appearance) in &mut mobs {
        let color = Some([255, 255, 255, 0]);
        if appearance.color != color || appearance.mouse_opacity != MouseOpacity::Transparent {
            appearance.color = color;
            appearance.mouse_opacity = MouseOpacity::Transparent;
        }
    }
}

/// `phased_mob/relaymove` -> `phased_check`: one tile per `movespeed`, through anything but
/// into CentCom.
pub(super) fn jaunt_steps(world: &mut World, mut steps: Local<MessageCursor<JauntStep>>) {
    let steps: Vec<JauntStep> = steps
        .read(world.resource::<Messages<JauntStep>>())
        .copied()
        .collect();
    let t = now(world);
    for JauntStep { mob, dir } in steps {
        let Some(jaunt) = world.get::<Jaunting>(mob).cloned() else {
            continue;
        };
        if jaunt.reappear_at.is_some() || t < jaunt.next_step {
            continue;
        }
        let Some(&pos) = world.get::<TilePos>(mob) else {
            continue;
        };
        let to = pos.step(dir);
        if let Some(mut facing) = world.get_mut::<Facing>(mob) {
            facing.set_if_neq(Facing(dir));
        }
        let mut exit_points = jaunt.exit_points.clone();
        // A nightmare's shadow walk refuses space and warns before light
        // (`crate::antag::nightmare`).
        if !crate::antag::nightmare::shadow_step_allowed(world, mob, to) {
            if let Some(mut j) = world.get_mut::<Jaunting>(mob) {
                j.next_step = t + JAUNT_STEP_SECS;
            }
            continue;
        }
        if world.resource::<SpatialIndex>().in_bounds(to) {
            if no_teleport(world, to) && !no_teleport(world, pos) {
                chat_to(
                    world,
                    mob,
                    Span::Danger,
                    "Some dull, universal force is blocking the way. Its overwhelmingly \
                     oppressive force feels dangerous.",
                );
            } else {
                world.entity_mut(mob).insert(to);
                // `update_exit_point`.
                let blocked = world
                    .resource::<SpatialIndex>()
                    .dense_on_tile(to)
                    .iter()
                    .any(|&e| e != mob);
                if !blocked {
                    exit_points.insert(0, to);
                    exit_points.truncate(4);
                }
            }
        }
        if let Some(mut j) = world.get_mut::<Jaunting>(mob) {
            j.next_step = t + JAUNT_STEP_SECS;
            j.exit_points = exit_points;
        }
    }
}

/// Per tick: the jaunts' end and the forcefields' timeout.
// Cost: one pass over the jaunting mobs and the forcefields (a handful per round).
pub(super) fn tick_spells(world: &mut World) {
    let t = now(world);
    let walls: Vec<(Entity, Option<TilePos>)> = world
        .query::<(Entity, &Forcefield, Option<&TilePos>)>()
        .iter(world)
        .filter(|(_, f, _)| t >= f.until)
        .map(|(e, _, p)| (e, p.copied()))
        .collect();
    for (wall, pos) in walls {
        world.despawn(wall);
        if let Some(pos) = pos {
            crate::construction::tile_changed(world, pos);
        }
    }
    let jaunts: Vec<(Entity, Jaunting)> = world
        .query::<(Entity, &Jaunting)>()
        .iter(world)
        .map(|(e, j)| (e, j.clone()))
        .collect();
    for (mob, jaunt) in jaunts {
        // `on_stat_change`: thrown out when it passes out.
        let conscious = world.get::<Stat>(mob) == Some(&Stat::Conscious);
        match jaunt.reappear_at {
            None if t >= jaunt.until || !conscious => {
                stop_jaunt(world, mob, &jaunt, t);
                if !conscious {
                    end_jaunt(world, mob, &jaunt);
                }
            }
            Some(at) if t >= at => end_jaunt(world, mob, &jaunt),
            _ => {}
        }
    }
}

/// `stop_jaunt`: the exit tile and the reappearing.
fn stop_jaunt(world: &mut World, mob: Entity, jaunt: &Jaunting, t: f64) {
    let free = |world: &World, p: TilePos| {
        world
            .resource::<SpatialIndex>()
            .dense_on_tile(p)
            .iter()
            .all(|&e| e == mob)
    };
    let here = world.get::<TilePos>(mob).copied();
    let found = jaunt.exit_points.iter().copied().find(|&p| free(world, p));
    let exit = match found {
        Some(p) => p,
        None => {
            if here.is_some_and(|h| h != jaunt.start) {
                chat_to(
                    world,
                    mob,
                    Span::Danger,
                    "Unable to find an unobstructed space, you find yourself ripped back to \
                     where you started.",
                );
            }
            jaunt.start
        }
    };
    world.entity_mut(mob).insert(exit);
    play(world, exit, "sound/effects/magic/ethereal_exit.ogg");
    if let Some(mut j) = world.get_mut::<Jaunting>(mob) {
        j.reappear_at = Some(t + JAUNT_REAPPEAR_SECS);
    }
}

/// `end_jaunt`: solid and seen again (beside a dense tile when it ended on one).
fn end_jaunt(world: &mut World, mob: Entity, jaunt: &Jaunting) {
    world.entity_mut(mob).remove::<Jaunting>();
    if let Some(mut appearance) = world.get_mut::<Appearance>(mob) {
        appearance.color = jaunt.color;
        appearance.mouse_opacity = jaunt.mouse_opacity;
    }
    if let Some(&pos) = world.get::<TilePos>(mob) {
        let turf_dense = crate::construction::turf_at(world, pos)
            .is_some_and(|t| world.get::<Density>(t).is_some());
        if turf_dense {
            let aside: Vec<TilePos> = Dir::CARDINALS
                .into_iter()
                .map(|d| pos.step(d))
                .filter(|&p| {
                    crate::construction::turf_at(world, p)
                        .is_some_and(|t| world.get::<Density>(t).is_none())
                })
                .collect();
            if let Some(p) = pick(world, &aside) {
                world.entity_mut(mob).insert(p);
            }
        }
    }
    super::refresh(world, mob);
}

/// `exit_jaunt` at once, without the ethereal jaunt's reappearing (a shadow walk's
/// `eject_jaunter`, `crate::antag::nightmare`).
pub fn exit_jaunt(world: &mut World, mob: Entity) {
    if let Some(jaunt) = world.get::<Jaunting>(mob).cloned() {
        end_jaunt(world, mob, &jaunt);
    }
}

/// The jaunting mob's own client keeps it: true when `mob` is jaunting (`crate::server`).
pub fn is_jaunting(world: &World, mob: Entity) -> bool {
    world.get::<Jaunting>(mob).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bolts_carry_tg_numbers() {
        let fireball = bolt_data(BoltKind::Fireball, 3);
        assert_eq!(fireball.range, 12.0);
        assert_eq!(
            (fireball.damage, fireball.damage_type.as_str()),
            (10.0, "burn")
        );
        let missile = bolt_data(BoltKind::MagicMissile, 1);
        assert_eq!(
            (missile.speed, missile.paralyze, missile.range),
            (0.2, 60.0, 100.0)
        );
        assert_eq!(bolt_data(BoltKind::Lightning, 1).speed, 3.5);
        // Magic ignores armor (`armor_flag = NONE`).
        assert!(bolt_data(BoltKind::Death, 1).armor_flag.is_empty());
    }
}
