//! The heretic paths other than Ash (`knowledge/{blade,cosmic,flesh,lock,moon,rust,void}_lore.dm`):
//! the marks (`status_effects/mark_effects.dm`), the starting knowledge's grasp effects, the
//! blade upgrades, Void Chill (`status_effects/void_chill.dm`), the Void Pull and Realignment
//! spells (`magic/void_pull.dm`, `magic/realignment.dm`) and the Blade and Void ascensions.
//!
//! [`super::server`] calls in from the grasp ([`on_grasp`]), the blade ([`on_blade`]), a spell
//! button ([`cast`]), the ascension ([`on_ascend`]) and its tick ([`tick`]).
// shortcut: no mark underlays or chill overlays; the Blade mark does not lock its target in the
// area and the Lock mark does not deny access (no move/access hooks); the Moon mark's pacifism
// is not lifted by 15 damage; no disgust (Rust mark and blade); no ghouls (Flesh grasp), robotic
// limb rust (Rust grasp), jumpsuit unzipping (Lock grasp), star marks and cosmic fields (Cosmic),
// protective blades (Blade), offhand follow-ups (Blade upgrade) or Cosmic combos. Void Pull
// takes the range, not `view()`. The Void ascension's storm only chills, silences and blurs the
// crew around (no door/window damage, weather or projectile dodge); other ascensions only
// announce.

use bevy::prelude::*;
use ss13_shared::chat::Span;
use ss13_shared::{Facing, TilePos};

use super::knowledge::KnowledgeDef;
use super::server::{
    AshMark, Heretic, carbons_in_range, chat_to, hurt, is_heretic, now, rand, say, set_cooldown,
    spell_id, spend_charge, whisper,
};
use crate::health::{BodyPosition, DamageType, MoveSpeedModifier, Stat};
use crate::status_effects::{self, Incap, Timed};

const BLADE_START: &str = "/datum/heretic_knowledge/limited_amount/starting/base_blade";
const COSMIC_START: &str = "/datum/heretic_knowledge/limited_amount/starting/base_cosmic";
const FLESH_START: &str = "/datum/heretic_knowledge/limited_amount/starting/base_flesh";
const LOCK_START: &str = "/datum/heretic_knowledge/limited_amount/starting/base_lock";
const MOON_START: &str = "/datum/heretic_knowledge/limited_amount/starting/base_moon";
const RUST_START: &str = "/datum/heretic_knowledge/limited_amount/starting/base_rust";
const VOID_START: &str = "/datum/heretic_knowledge/limited_amount/starting/base_void";

const FLESH_UPGRADE: &str = "/datum/heretic_knowledge/blade_upgrade/flesh";
const LOCK_UPGRADE: &str = "/datum/heretic_knowledge/blade_upgrade/flesh/lock";
const MOON_UPGRADE: &str = "/datum/heretic_knowledge/blade_upgrade/moon";
const VOID_UPGRADE: &str = "/datum/heretic_knowledge/blade_upgrade/void";
const COSMIC_UPGRADE: &str = "/datum/heretic_knowledge/blade_upgrade/cosmic";

const BLADE_FINAL: &str = "/datum/heretic_knowledge/ultimate/blade_final";
const VOID_FINAL: &str = "/datum/heretic_knowledge/ultimate/void_final";

const VOID_PULL_ACTION: &str = "/datum/action/cooldown/spell/aoe/void_pull";
const VOID_PULL: &str = "/datum/heretic_knowledge/spell/void_pull";
const REALIGN_ACTION: &str = "/datum/action/cooldown/spell/realignment";
const REALIGN: &str = "/datum/heretic_knowledge/spell/realignment";

/// `/datum/status_effect/eldritch` `duration`.
const MARK_SECS: f64 = 15.0;
/// `/datum/status_effect/eldritch/lock` `duration`.
const LOCK_MARK_SECS: f64 = 10.0;
/// `/datum/status_effect/void_chill` `duration` and `stack_limit`.
const CHILL_SECS: f64 = 30.0;
const CHILL_LIMIT: u32 = 5;
/// `/datum/status_effect/realignment` `duration`.
const REALIGN_SECS: f64 = 8.0;
/// `spell_max_level` of Realignment.
const REALIGN_MAX_LEVEL: u32 = 10;

/// Which `/datum/status_effect/eldritch/*` a [`PathMark`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkKind {
    Flesh,
    Rust,
    Void,
    Blade,
    Cosmic,
    Lock,
    Moon,
}

/// A heretic mark other than Ash's ([`AshMark`]); one mark at a time (`STATUS_EFFECT_REPLACE` on
/// the `eldritch` base type).
#[derive(Component, Clone, Copy, Debug)]
pub struct PathMark {
    pub kind: MarkKind,
    pub until: f64,
    /// Cosmic: the cosmic diamond's tile (where the mark sends its owner back to).
    pub origin: Option<TilePos>,
}

/// `/datum/status_effect/void_chill`: stacks of cold and slowdown.
#[derive(Component, Clone, Copy, Debug)]
pub struct VoidChill {
    pub stacks: u32,
    pub until: f64,
    /// The slowdown this effect added to [`MoveSpeedModifier`], deciseconds per tile.
    pub slowdown: f32,
}

/// `/datum/status_effect/realignment`.
#[derive(Component, Clone, Copy, Debug)]
pub struct Realigning {
    pub until: f64,
}

/// Realignment's spell level (`level_spell`/`delevel_spell`) and the pending delevels.
#[derive(Component, Clone, Debug, Default)]
pub struct RealignLevel {
    pub level: u32,
    pub delevel_at: Vec<f64>,
}

/// The Void ascension's storm around the heretic (`void_final/on_life`).
#[derive(Component, Clone, Copy, Debug)]
pub struct VoidStorm {
    pub next_tick: f64,
}

fn mark_kind(h: &Heretic) -> Option<MarkKind> {
    [
        (FLESH_START, MarkKind::Flesh),
        (RUST_START, MarkKind::Rust),
        (VOID_START, MarkKind::Void),
        (BLADE_START, MarkKind::Blade),
        (COSMIC_START, MarkKind::Cosmic),
        (LOCK_START, MarkKind::Lock),
        (MOON_START, MarkKind::Moon),
    ]
    .into_iter()
    .find(|(path, _)| h.has(path))
    .map(|(_, kind)| kind)
}

/// Whether the heretic has researched a non-Ash starting knowledge (it triggers marks).
pub fn has_path_start(world: &World, mob: Entity) -> bool {
    world
        .get::<Heretic>(mob)
        .is_some_and(|h| mark_kind(h).is_some())
}

/// `create_mark`: the path's mark on a living target, replacing any other mark.
fn create_mark(world: &mut World, target: Entity, kind: MarkKind) {
    if world.get::<Stat>(target) == Some(&Stat::Dead) {
        return;
    }
    let t = now(world);
    let until = t + if kind == MarkKind::Lock {
        LOCK_MARK_SECS
    } else {
        MARK_SECS
    };
    let origin = world.get::<TilePos>(target).copied();
    world.entity_mut(target).remove::<AshMark>();
    world.entity_mut(target).insert(PathMark {
        kind,
        until,
        origin,
    });
    if kind == MarkKind::Moon {
        // `eldritch/moon/on_apply`: pacified for the mark's time.
        status_effects::set_timed(world, target, Timed::Pacify, MARK_SECS as f32, true);
        laugh(world, target);
        chat_to(
            world,
            target,
            Span::Warning,
            "Вы чувствуете, что и мышке не навредите!",
        );
    }
}

fn laugh(world: &mut World, mob: Entity) {
    let text = if rand(world, 2) == 0 {
        "хихикает."
    } else {
        "смеётся."
    };
    crate::mobs::defense::emote(world, mob, text);
}

/// `check_behind`: same tile, lying, or the target faces away from the source.
fn check_behind(world: &World, source: Entity, target: Entity) -> bool {
    let (Some(&from), Some(&to)) = (world.get::<TilePos>(target), world.get::<TilePos>(source))
    else {
        return false;
    };
    if from == to || world.get::<BodyPosition>(target).is_some_and(|b| b.lying()) {
        return true;
    }
    let Some(dir) = crate::mobs::ai::movement::dir_towards(from, to) else {
        return true;
    };
    world
        .get::<Facing>(target)
        .is_some_and(|f| f.0 as u8 & dir.reverse() as u8 != 0)
}

/// `COMSIG_HERETIC_MANSUS_GRASP_ATTACK` of a non-Ash starting knowledge: the mark, then the
/// path's `on_mansus_grasp`.
pub fn on_grasp(world: &mut World, mob: Entity, target: Entity) {
    let Some(kind) = world.get::<Heretic>(mob).and_then(mark_kind) else {
        return;
    };
    create_mark(world, target, kind);
    let carbon = world.get::<crate::human::HumanLook>(target).is_some();
    match kind {
        MarkKind::Void if carbon => {
            status_effects::adjust_timed(world, target, Timed::Silence, 10.0, None);
            apply_chill(world, target, 2);
        }
        MarkKind::Blade if check_behind(world, mob, target) => {
            status_effects::adjust(world, target, Incap::Paralyze, 1.5);
            hurt(world, target, DamageType::Brute, 10.0);
            chat_to(world, mob, Span::Warning, "Удар в спину!");
        }
        // shortcut: no `MAGIC_RESISTANCE_MOON` items, no `moon_grasp_hide` on the caster.
        MarkKind::Moon if carbon => {
            chat_to(
                world,
                target,
                Span::Danger,
                "Сверху доносится смех, отдающийся эхом.",
            );
            crate::hallucination::cause_hallucination(
                world,
                target,
                crate::hallucination::kinds::Kind::Delusion(
                    crate::hallucination::kinds::Delusion::Moon,
                ),
                "delusion/preset/moon hallucination caused by mansus grasp",
            );
            adjust_sanity(world, target, -30.0);
        }
        _ => {}
    }
}

pub(super) fn adjust_sanity(world: &mut World, mob: Entity, amount: f32) {
    if let Some(mut mood) = world.get_mut::<crate::mood::Mood>(mob) {
        mood.adjust_sanity(
            amount,
            crate::mood::SANITY_INSANE,
            crate::mood::SANITY_GREAT,
        );
    }
}

/// `trigger_mark` / `eldritch/*/on_effect` for a [`PathMark`]: false when there is none.
pub fn trigger_path_mark(world: &mut World, target: Entity) -> bool {
    let t = now(world);
    let Some(mark) = world
        .get::<PathMark>(target)
        .copied()
        .filter(|m| m.until > t)
    else {
        return false;
    };
    world.entity_mut(target).remove::<PathMark>();
    match mark.kind {
        MarkKind::Flesh => wound_random_limb(world, target, false),
        // shortcut: no disgust (`adjust_disgust(100)`).
        MarkKind::Rust => {
            status_effects::adjust_timed(world, target, Timed::Confusion, 10.0, None);
        }
        MarkKind::Void => {
            apply_chill(world, target, 3);
            status_effects::adjust_timed(world, target, Timed::Silence, 10.0, None);
        }
        MarkKind::Cosmic => {
            if let Some(origin) = mark.origin {
                world.entity_mut(target).insert(origin);
            }
            status_effects::adjust(world, target, Incap::Paralyze, 2.0);
        }
        MarkKind::Moon => {
            status_effects::set_timed(world, target, Timed::Pacify, 0.0, false);
            status_effects::adjust_timed(world, target, Timed::Confusion, 30.0, None);
            crate::medical::organs::adjust_organ_loss_up_to(
                world,
                target,
                crate::medical::organs::Slot::Brain,
                25.0,
                160.0,
            );
            laugh(world, target);
            crate::mood::add_event(world, target, "Moon Insanity", &MOON_INSANITY);
        }
        MarkKind::Blade | MarkKind::Lock => {}
    }
    if let Some(&pos) = world.get::<TilePos>(target) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, "sound/effects/magic/repulse.ogg", 75.0, true),
        );
    }
    true
}

/// `/datum/mood_event/moon_insanity`.
const MOON_INSANITY: crate::mood::MoodEvent = crate::mood::MoodEvent {
    id: "moon_insanity",
    description: "ЛУНА СУДИТ И СЧИТАЕТ МЕНЯ ЖАЖДУЩИМ!!!",
    mood_change: -3.0,
    timeout: 300.0,
    hidden: false,
};

/// A severe (or critical) slash wound on a random bodypart that is there.
fn wound_random_limb(world: &mut World, target: Entity, critical: bool) {
    use crate::medical::wounds::{Limbs, WoundKind, apply_wound};
    let Some(mut limbs) = world.get::<Limbs>(target).cloned() else {
        return;
    };
    let zones: Vec<crate::zone::BodyZone> = crate::zone::BodyZone::ALL
        .into_iter()
        .filter(|&z| !limbs.limb(z).missing)
        .collect();
    if zones.is_empty() {
        return;
    }
    let zone = zones[rand(world, zones.len())];
    let kind = if critical {
        WoundKind::SlashCritical
    } else {
        WoundKind::SlashSevere
    };
    apply_wound(limbs.limb_mut(zone), kind);
    world.entity_mut(target).insert(limbs);
}

/// `COMSIG_HERETIC_BLADE_ATTACK` after the marks: the blade upgrades and the Blade ascension.
pub fn on_blade(world: &mut World, mob: Entity, target: Entity) {
    if mob == target {
        return;
    }
    let Some(h) = world.get::<Heretic>(mob) else {
        return;
    };
    let (flesh, lock, moon, void, cosmic, blade_final) = (
        h.has(FLESH_UPGRADE),
        h.has(LOCK_UPGRADE),
        h.has(MOON_UPGRADE),
        h.has(VOID_UPGRADE),
        h.has(COSMIC_UPGRADE),
        h.has(BLADE_FINAL),
    );
    let carbon = world.get::<crate::human::HumanLook>(target).is_some();
    if flesh && carbon {
        wound_random_limb(world, target, false);
    }
    // `blade_upgrade/flesh/lock`: 35% for a critical slash.
    if lock && carbon && rand(world, 100) < 35 {
        wound_random_limb(world, target, true);
    }
    if void {
        apply_chill(world, target, 2);
    }
    if cosmic {
        // shortcut: the 5 burn of the hit only (no star mark, no combo).
        hurt(world, target, DamageType::Burn, 5.0);
    }
    if moon {
        use crate::hallucination::kinds::{Body, Kind};
        const BODIES: [Body; 5] = [
            Body::Husk,
            Body::Ghost,
            Body::Hole,
            Body::StaticGuy,
            Body::Alien,
        ];
        let body = BODIES[rand(world, BODIES.len())];
        crate::hallucination::cause_hallucination(
            world,
            target,
            Kind::Body(body),
            "upgraded path of moon blades",
        );
        laugh(world, target);
        adjust_sanity(world, target, -10.0);
        let awake = world
            .get::<Stat>(target)
            .is_some_and(|s| *s < Stat::SoftCrit);
        let brain = if awake && crate::mood::sanity(world, target) >= crate::mood::SANITY_NEUTRAL {
            10.0
        } else {
            25.0
        };
        crate::medical::organs::adjust_organ_loss(
            world,
            target,
            crate::medical::organs::Slot::Brain,
            brain,
        );
    }
    if blade_final {
        // `blade_final/on_eldritch_blade`: `clamp(30 - force, 0, 12)` extra brute with a sickly
        // blade's force of 20, half of it healed back while the target lives.
        let bonus = 10.0;
        hurt(world, target, DamageType::Brute, bonus);
        if world.get::<Stat>(target) != Some(&Stat::Dead) {
            hurt(world, mob, DamageType::Brute, -bonus / 2.0);
            hurt(world, mob, DamageType::Burn, -bonus / 2.0);
        }
    }
}

/// `apply_status_effect(/datum/status_effect/void_chill, stacks)`: not on heretics, silicons or
/// the magic-resistant; a refresh adds stacks.
pub fn apply_chill(world: &mut World, target: Entity, stacks: u32) {
    if world.get::<crate::human::HumanLook>(target).is_none()
        || is_heretic(world, target)
        || crate::spells::can_block_magic(world, target, crate::spells::MAGIC_RESISTANCE)
    {
        return;
    }
    let t = now(world);
    let mut chill = world
        .get::<VoidChill>(target)
        .copied()
        .unwrap_or(VoidChill {
            stacks: 0,
            until: t,
            slowdown: 0.0,
        });
    chill.stacks = (chill.stacks + stacks).min(CHILL_LIMIT);
    chill.until = t + CHILL_SECS;
    set_chill_slowdown(world, target, &mut chill);
    world.entity_mut(target).insert(chill);
}

/// `update_movespeed`: 0.5 deciseconds per tile and stack, as a delta on the shared modifier.
fn set_chill_slowdown(world: &mut World, target: Entity, chill: &mut VoidChill) {
    let wanted = 0.5 * chill.stacks as f32;
    let delta = wanted - chill.slowdown;
    chill.slowdown = wanted;
    if delta == 0.0 {
        return;
    }
    let current = world.get::<MoveSpeedModifier>(target).map_or(0.0, |m| m.0);
    world
        .entity_mut(target)
        .insert(MoveSpeedModifier(current + delta));
}

/// A spell of these paths: true when it is handled here.
pub fn cast(world: &mut World, client: Entity, mob: Entity, def: &'static KnowledgeDef) -> bool {
    match def.action {
        VOID_PULL_ACTION => cast_void_pull(world, mob),
        REALIGN_ACTION => cast_realignment(world, client, mob),
        _ => return false,
    }
    true
}

/// `void_pull`: every living non-heretic within 2 tiles takes 30 brute, three stacks of chill,
/// a knockdown and a short paralysis and is pulled three steps towards the caster.
fn cast_void_pull(world: &mut World, mob: Entity) {
    whisper(world, mob, "BR'NG F'RTH TH'M T' M'.");
    let Some(&center) = world.get::<TilePos>(mob) else {
        return;
    };
    for victim in carbons_in_range(world, center, 2) {
        if victim == mob
            || is_heretic(world, victim)
            || crate::spells::can_block_magic(world, victim, crate::spells::MAGIC_RESISTANCE)
            || world.get::<crate::inventory::CarriedBy>(victim).is_some()
        {
            continue;
        }
        hurt(world, victim, DamageType::Brute, 30.0);
        apply_chill(world, victim, 3);
        status_effects::adjust(world, victim, Incap::Knockdown, 3.0);
        status_effects::adjust(world, victim, Incap::Paralyze, 0.5);
        for _ in 0..3 {
            let Some(&from) = world.get::<TilePos>(victim) else {
                break;
            };
            if (from.x - center.x).abs().max((from.y - center.y).abs()) <= 1 {
                break;
            }
            let index = world.resource::<ss13_world::SpatialIndex>();
            let Some(dir) = crate::mobs::ai::movement::greedy_step(index, victim, from, center)
            else {
                break;
            };
            world.entity_mut(victim).insert(from.step(dir));
        }
    }
    let t = now(world);
    spend_charge(world, mob, VOID_PULL);
    set_cooldown(world, mob, &spell_id(VOID_PULL), t + 30.0);
}

/// `realignment`: 8 s of stamina and stun recovery while pacified; each cast levels the spell up
/// (6 s more cooldown per level), each level going back down after a while.
fn cast_realignment(world: &mut World, client: Entity, mob: Entity) {
    say(world, mob, "R'S'T.");
    let t = now(world);
    world.entity_mut(mob).insert(Realigning {
        until: t + REALIGN_SECS,
    });
    status_effects::set_timed(world, mob, Timed::Pacify, REALIGN_SECS as f32, true);
    crate::tools::tell(world, client, Span::Notice, "Мы начинаем выравнивать себя.");
    let mut level = world.get::<RealignLevel>(mob).cloned().unwrap_or_default();
    level.level = level.level.max(1);
    let cooldown = 6.0 * f64::from(level.level);
    if level.level < REALIGN_MAX_LEVEL {
        level.level += 1;
        // `max(cooldown_time * spell_max_level * 0.5, 1.5 MINUTES)` with the new cooldown.
        let reduction =
            (6.0 * f64::from(level.level) * f64::from(REALIGN_MAX_LEVEL) * 0.5).max(90.0);
        level.delevel_at.push(t + reduction);
    }
    world.entity_mut(mob).insert(level);
    spend_charge(world, mob, REALIGN);
    set_cooldown(world, mob, &spell_id(REALIGN), t + cooldown);
}

/// The ascension's own effects (after [`super::server`]'s announcement).
pub fn on_ascend(world: &mut World, mob: Entity, def: &'static KnowledgeDef) {
    match def.path {
        BLADE_FINAL => {
            let mut absorber = status_effects::StunAbsorber::new("Серебряный Вихрь");
            absorber.message = Some("%EFFECT_OWNER стряхивает оглушение!".into());
            absorber.self_message = Some("Вы стряхиваете оглушение!".into());
            absorber.examine_message = Some("%EFFECT_OWNER_THEYRE стоит непоколебимо.".into());
            absorber.max_secs = 45.0;
            absorber.delete_after_max = false;
            absorber.recharge_secs = 120.0;
            status_effects::add_stun_absorption(world, mob, absorber, None);
        }
        VOID_FINAL => {
            let t = now(world);
            world.entity_mut(mob).insert(VoidStorm { next_tick: t });
        }
        _ => {}
    }
}

/// Marks and chills running out, chill cold, Realignment, the Void storm.
pub fn tick(world: &mut World, t: f64) {
    let dt = world.resource::<Time>().delta_secs();
    let mut q = world.query::<(Entity, &PathMark)>();
    // The Moon mark's pacifism runs out with it (`set_timed` for the mark's time).
    let expired: Vec<Entity> = q
        .iter(world)
        .filter(|(_, m)| m.until <= t)
        .map(|(e, _)| e)
        .collect();
    for e in expired {
        world.entity_mut(e).remove::<PathMark>();
    }

    let mut q = world.query::<(Entity, &VoidChill)>();
    let chilled: Vec<(Entity, VoidChill)> = q.iter(world).map(|(e, c)| (e, *c)).collect();
    for (e, mut chill) in chilled {
        if chill.until <= t {
            chill.stacks = 0;
            set_chill_slowdown(world, e, &mut chill);
            world.entity_mut(e).remove::<VoidChill>();
            continue;
        }
        // `tick`: -12 K per stack and second.
        if let Some(mut r) = world.get_mut::<crate::breath::Respiration>(e) {
            r.bodytemperature -= 12.0 * f64::from(chill.stacks) * f64::from(dt);
        }
    }

    let mut q = world.query::<(Entity, &Realigning)>();
    let realigning: Vec<(Entity, f64)> = q.iter(world).map(|(e, r)| (e, r.until)).collect();
    for (e, until) in realigning {
        if until <= t {
            world.entity_mut(e).remove::<Realigning>();
            continue;
        }
        // `tick` every 0.2 s: -10 stamina and -1 s of every immobility.
        status_effects::adjust_stamina(world, e, -50.0 * dt);
        for kind in [
            Incap::Stun,
            Incap::Knockdown,
            Incap::Paralyze,
            Incap::Immobilize,
        ] {
            status_effects::adjust(world, e, kind, -5.0 * dt);
        }
    }

    let mut q = world.query::<(Entity, &mut RealignLevel)>();
    for (_, mut level) in q.iter_mut(world) {
        let before = level.delevel_at.len();
        level.delevel_at.retain(|&at| at > t);
        let gone = (before - level.delevel_at.len()) as u32;
        if gone > 0 {
            level.level = level.level.saturating_sub(gone).max(1);
        }
    }

    let mut q = world.query::<(Entity, &VoidStorm)>();
    let storms: Vec<Entity> = q
        .iter(world)
        .filter(|(_, s)| s.next_tick <= t)
        .map(|(e, _)| e)
        .collect();
    for mob in storms {
        // `on_life` every `SSmobs` tick (2 s).
        world
            .entity_mut(mob)
            .insert(VoidStorm { next_tick: t + 2.0 });
        if world.get::<Stat>(mob) == Some(&Stat::Dead) {
            world.entity_mut(mob).remove::<VoidStorm>();
            continue;
        }
        let Some(&center) = world.get::<TilePos>(mob) else {
            continue;
        };
        for e in carbons_in_range(world, center, 10) {
            if e == mob
                || is_heretic(world, e)
                || crate::spells::can_block_magic(world, e, crate::spells::MAGIC_RESISTANCE)
            {
                continue;
            }
            // `adjust_silence_up_to(2 SECONDS, 20 SECONDS)`.
            status_effects::adjust_timed(world, e, Timed::Silence, 2.0, Some(20.0));
            apply_chill(world, e, 1);
            let blur = rand(world, 3) as f32;
            crate::vision::set_eye_blur_if_lower(world, e, blur);
            if let Some(mut r) = world.get_mut::<crate::breath::Respiration>(e) {
                // `-30 * TEMPERATURE_DAMAGE_COEFFICIENT` (1.5).
                r.bodytemperature -= 45.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chill_stacks_cap_and_slow() {
        let mut world = World::new();
        let e = world.spawn(MoveSpeedModifier(1.0)).id();
        let mut chill = VoidChill {
            stacks: 7.min(CHILL_LIMIT),
            until: 0.0,
            slowdown: 0.0,
        };
        set_chill_slowdown(&mut world, e, &mut chill);
        assert_eq!(world.get::<MoveSpeedModifier>(e).unwrap().0, 3.5);
        chill.stacks = 0;
        set_chill_slowdown(&mut world, e, &mut chill);
        assert_eq!(world.get::<MoveSpeedModifier>(e).unwrap().0, 1.0);
    }
}
