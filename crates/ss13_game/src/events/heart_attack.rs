//! Random Heart Attack (`code/modules/events/heart_attack.dm`) and its status effect
//! (`code/datums/status_effects/debuffs/heart_attack.dm`).
//!
//! - Candidates: conscious crew humans with a client and an organic heart that beats, not in a
//!   heart attack; three times as likely after a lot of junk food (`satiety <= -60`). Without
//!   any the event cannot roll.
//! - [`HeartAttack`]: a 150 s countdown, one step a second. Chest beats and stamina loss first;
//!   from 100 coughing; from 70 chest pain, vomiting, weakness and confusion; from 35 blackouts,
//!   blur, blindness and falls; small oxygen damage throughout (none past 40). At 0 the heart
//!   stops ([`crate::medical::organs::set_heartattack`]) and the victim stays up for 4 s
//!   ([`HeartDesperation`]). Defibrillator shocks on a living patient push it back 50 s
//!   ([`defib_shock`]); past 160 the attack passes.
// shortcut: no athletics skill (no fitness protection), no exercised status, no admin quantity,
// no medhud or examine line, no stamina/electrocution relief (minor shocks, electrocution),
// no removal on full heal or heart removal (the next tick ends it without an organic heart),
// no heartbeat channel (the beats are one-shot sounds), no flash overlay, and the desperation
// window lacks `TRAIT_STABLEHEART`/no-crit traits (it only floors and silences).

use bevy::prelude::*;
use ss13_net::sound::server::SendSound;
use ss13_shared::chat::Span;
use ss13_shared::protocol::SoundEvent;

use super::controller::{EventRng, EventState, RoundEvent, pick_weight};
use crate::combat::CombatRng;
use crate::health::{DamageType, Health, Stat};
use crate::medical::diseases::{damage, emote, say_to, vomit};
use crate::medical::organs::{self, OrganHost, Slot};
use crate::medical::wounds::spt_prob;
use crate::server::PlayerMob;
use crate::status_effects::{self, Incap, Timed};

pub const NAME: &str = "Random Heart Attack";

/// `time_until_stoppage`'s start, `ATTACK_STAGE_TWO`/`THREE`/`FOUR`, `ATTACK_CURE_THRESHOLD`.
pub const START: f32 = 150.0;
const STAGE_TWO: f32 = 100.0;
const STAGE_THREE: f32 = 70.0;
const STAGE_FOUR: f32 = 35.0;
pub const CURE_THRESHOLD: f32 = 160.0;
/// `OXYLOSS_MAXIMUM`.
const OXYLOSS_MAXIMUM: f32 = 40.0;
/// The status effect's `tick_interval`.
const TICK_SECS: f64 = 1.0;
/// `/datum/status_effect/heart_desperation` `duration`.
const DESPERATION_SECS: f64 = 4.0;
/// `defib_shock`'s relief.
pub const DEFIB_RELIEF: f32 = 50.0;
const SINGLEBEAT: &str = "sound/effects/singlebeat.ogg";
const SLOWBEAT: &str = "sound/effects/health/slowbeat.ogg";

/// `/datum/status_effect/heart_attack`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct HeartAttack {
    /// `time_until_stoppage`, ticks.
    pub time_until_stoppage: f32,
    /// Made apparent (stage three reached).
    pub visible: bool,
    pub next_tick: f64,
}

/// `/datum/status_effect/heart_desperation`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct HeartDesperation {
    pub until: f64,
    pub next_tick: f64,
}

/// tg `can_heartattack()`: needs a heart (`needs_heart()`) and has an organic one.
pub fn can_heartattack(world: &World, mob: Entity) -> bool {
    use crate::species::{Trait, def_of, has_trait};
    world.get::<OrganHost>(mob).is_some()
        && def_of(world, mob).organs.heart.is_some()
        && !has_trait(world, mob, Trait::NoBlood)
        && !crate::medical::bioware::stable_heart(world, mob)
        && organs::state(world, mob, Slot::Heart).is_some_and(|h| !h.robotic)
}

/// `IS_UNCONSCIOUS_OR_CRIT`.
fn down(world: &World, mob: Entity) -> bool {
    world.get::<Stat>(mob) != Some(&Stat::Conscious) || status_effects::knocked_out(world, mob)
}

/// `generate_candidates()` with their weights.
pub fn candidates(world: &mut World) -> Vec<(Entity, i64)> {
    let mut query = world.query_filtered::<Entity, (
        With<PlayerMob>,
        With<crate::jobs::PlayerJob>,
        With<crate::human::HumanLook>,
    )>();
    let mut all: Vec<Entity> = query.iter(world).collect();
    all.sort();
    all.into_iter()
        .filter(|&mob| {
            !down(world, mob)
                && can_heartattack(world, mob)
                && world.get::<HeartAttack>(mob).is_none()
                && !organs::cardiac_arrest(world, mob)
        })
        .map(|mob| {
            let junk = world
                .get::<crate::hunger::Hunger>(mob)
                .is_some_and(|h| h.satiety <= -60.0);
            (mob, if junk { 3 } else { 1 })
        })
        .collect()
}

pub fn has_candidates(world: &mut World) -> bool {
    !candidates(world).is_empty()
}

/// `apply_status_effect(/datum/status_effect/heart_attack)` (unique): false without an organic
/// heart or when already having one.
pub fn apply(world: &mut World, mob: Entity) -> bool {
    if !can_heartattack(world, mob) || world.get::<HeartAttack>(mob).is_some() {
        return false;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    world.entity_mut(mob).insert(HeartAttack {
        time_until_stoppage: START,
        visible: false,
        next_tick: now + TICK_SECS,
    });
    true
}

/// `COMSIG_HEARTATTACK_DEFIB` from a defibrillator on a living patient: true if it had a heart
/// attack (the defib then reports the stabilized heart).
pub fn defib_shock(world: &mut World, mob: Entity, defib: &str) -> bool {
    {
        let Some(mut attack) = world.get_mut::<HeartAttack>(mob) else {
            return false;
        };
        attack.time_until_stoppage += DEFIB_RELIEF;
    }
    let name = crate::medical::name_of(world, mob);
    crate::medical::visible(
        world,
        mob,
        &format!(
            "The {defib} shocks your heart awake, and you feel the pain in your chest ease up!"
        ),
        &format!("{name} seems to be relieved of their pain as they're shocked by the {defib}!"),
        Span::Notice,
    );
    true
}

pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(HeartAttackEvent { quantity: 1 })
}

/// `/datum/round_event/heart_attack`.
struct HeartAttackEvent {
    quantity: u32,
}

impl RoundEvent for HeartAttackEvent {
    fn start(&mut self, world: &mut World, _: &mut EventState) {
        let mut victims = candidates(world);
        while self.quantity > 0 && !victims.is_empty() {
            // `attack_heart()`; the fitness protection roll is 0 without the athletics skill.
            let Some(winner) = pick_weight(&victims, &mut world.resource_mut::<EventRng>().0)
            else {
                break;
            };
            victims.retain(|&(mob, _)| mob != winner);
            if apply(world, winner) {
                info!("{NAME}: {winner}");
                self.quantity -= 1;
            }
        }
    }
}

fn beat(world: &mut World, mob: Entity, file: &str, volume: f32) {
    let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) else {
        return;
    };
    world.write_message(SendSound {
        client,
        sound: SoundEvent {
            file: file.to_owned(),
            volume,
            speed: 1.0,
            offset: None,
        },
    });
}

/// `adjust_eye_blur_up_to(secs, max)`.
fn blur_up_to(world: &mut World, mob: Entity, secs: f32, max: f32) {
    let current = world
        .get::<crate::vision::Vision>(mob)
        .map_or(0.0, |v| v.blur);
    if current < max {
        crate::vision::set_eye_blur_if_lower(world, mob, (current + secs).min(max));
    }
}

/// One `tick()`; false when the effect is over.
fn tick(world: &mut World, mob: Entity, attack: &mut HeartAttack, rng: &mut CombatRng) -> bool {
    let secs = TICK_SECS as f32;
    if !can_heartattack(world, mob) {
        return false;
    }
    let t = attack.time_until_stoppage;
    if t > CURE_THRESHOLD {
        let name = crate::medical::name_of(world, mob);
        crate::medical::visible(
            world,
            mob,
            "The pain in your chest has subsided. You're cured!",
            &format!("{name} relaxes their body and stops clutching at their chest!"),
            Span::Notice,
        );
        return false;
    }
    let mut oxyloss = 0.0;
    if t > STAGE_THREE && spt_prob(rng, 5.0, secs) {
        beat(world, mob, SINGLEBEAT, 25.0);
        status_effects::adjust_stamina(world, mob, 5.0);
    }
    if t <= STAGE_TWO && t > STAGE_THREE && spt_prob(rng, 10.0, secs) {
        emote(world, mob, "cough");
        status_effects::adjust_stamina(world, mob, 10.0);
        oxyloss += 4.0;
    }
    if t <= STAGE_THREE {
        attack.visible = true;
        if spt_prob(rng, 15.0, secs) {
            say_to(
                world,
                mob,
                Span::Danger,
                "You feel a sharp pain in your chest!",
            );
            if spt_prob(rng, 15.0, secs) {
                vomit(world, mob, 95.0, true, false, 1);
            }
            emote(world, mob, "cough");
            oxyloss += 1.0;
        }
        if spt_prob(rng, 8.0, secs) {
            say_to(world, mob, Span::Danger, "You feel very weak and dizzy...");
            if let Some(mut effects) = status_effects::effects_mut(world, mob) {
                effects.adjust_confusion_up_to(6.0, 10.0);
            }
            status_effects::adjust_stamina(world, mob, 20.0);
            emote(world, mob, "cough");
            oxyloss += 8.0;
        }
    }
    if t <= STAGE_FOUR {
        if spt_prob(rng, 5.0, secs) {
            say_to(
                world,
                mob,
                Span::BoldWarning,
                "It feels like you're shutting down...",
            );
            status_effects::adjust_timed(world, mob, Timed::Dizziness, 4.0, Some(10.0));
            blur_up_to(world, mob, 4.0, 20.0);
            status_effects::adjust_stamina(world, mob, 20.0);
        }
        if spt_prob(rng, 5.0, secs) {
            emote(world, mob, "cough");
            if spt_prob(rng, 5.0, secs) {
                say_to(
                    world,
                    mob,
                    Span::BoldWarning,
                    "You cough. Everything goes dark. You're going to die soon.",
                );
                crate::vision::adjust_temp_blindness(world, mob, 10.0);
            } else {
                say_to(
                    world,
                    mob,
                    Span::BoldWarning,
                    "As you cough, your chest surges in pain and darkness closes in around your \
                     sight.",
                );
                crate::vision::adjust_temp_blindness(world, mob, 2.0);
                blur_up_to(world, mob, 4.0, 20.0);
            }
            oxyloss += 8.0;
            status_effects::apply(world, mob, Incap::Paralyze, 1.0);
        }
        oxyloss += 3.0;
    }
    if world
        .get::<Health>(mob)
        .is_some_and(|h| h.oxy < OXYLOSS_MAXIMUM)
        && oxyloss > 0.0
    {
        damage(world, mob, DamageType::Oxy, oxyloss);
    }
    if t <= 0.0 {
        if !down(world, mob) {
            say_to(
                world,
                mob,
                Span::BoldWarning,
                "You feel a terrible pain in your chest, as if your heart has stopped!",
            );
        }
        let blur = world
            .get::<crate::vision::Vision>(mob)
            .map_or(0.0, |v| v.blur);
        crate::vision::set_eye_blur_if_lower(world, mob, blur + 20.0);
        organs::set_heartattack(world, mob, true);
        let now = world.resource::<Time>().elapsed_secs_f64();
        world.entity_mut(mob).insert(HeartDesperation {
            until: now + DESPERATION_SECS,
            next_tick: now,
        });
        return false;
    }
    if t <= STAGE_TWO {
        if t <= STAGE_FOUR {
            beat(world, mob, SINGLEBEAT, 100.0);
        } else {
            beat(world, mob, SLOWBEAT, 25.0);
        }
    }
    attack.time_until_stoppage -= 1.0;
    true
}

/// The heart attacks' and the desperation windows' ticks.
pub fn process_heart_attacks(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut query = world.query::<(Entity, &HeartAttack)>();
    let attacks: Vec<(Entity, HeartAttack)> =
        query.iter(world).map(|(e, a)| (e, a.clone())).collect();
    let mut query = world.query::<(Entity, &HeartDesperation)>();
    let desperate: Vec<(Entity, HeartDesperation)> =
        query.iter(world).map(|(e, d)| (e, d.clone())).collect();
    if attacks.is_empty() && desperate.is_empty() {
        return;
    }
    let mut rng = crate::medical::fork_rng(world);
    for (mob, mut attack) in attacks {
        if attack.next_tick > now {
            continue;
        }
        attack.next_tick += TICK_SECS;
        if tick(world, mob, &mut attack, &mut rng) {
            world.entity_mut(mob).insert(attack);
        } else {
            world.entity_mut(mob).remove::<HeartAttack>();
        }
    }
    for (mob, mut window) in desperate {
        if window.until <= now {
            world.entity_mut(mob).remove::<HeartDesperation>();
            continue;
        }
        if window.next_tick > now {
            continue;
        }
        window.next_tick += TICK_SECS;
        // `on_apply` and every `tick()`.
        status_effects::knockdown(world, mob, 2.0, 2.0);
        status_effects::set_timed(world, mob, Timed::Silence, 2.0, true);
        world.entity_mut(mob).insert(window);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_are_ordered() {
        const { assert!(CURE_THRESHOLD > START) };
        const { assert!(START > STAGE_TWO && STAGE_TWO > STAGE_THREE && STAGE_THREE > STAGE_FOUR) };
        // Four defib shocks save anyone who has not yet arrested (three are not enough at 1 s).
        const { assert!(1.0 + 4.0 * DEFIB_RELIEF > CURE_THRESHOLD) };
    }
}
