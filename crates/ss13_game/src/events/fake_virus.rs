//! Fake Virus (`code/modules/events/fake_virus.dm`): 1-3 conscious crewmembers get three minutes
//! of hypochondria (`/datum/status_effect/fake_virus`: fake symptom lines, later coughs and
//! sneezes, no harm), 1-5 others cough, sniff or feel a headache once, 3-15 s later.
// shortcut: no `TRAIT_VIRUSIMMUNE`, no ghost notification.

use bevy::prelude::*;
use ss13_shared::chat::Span;

use super::controller::{EventRng, EventState, RoundEvent};
use crate::combat::CombatRng;
use crate::health::Stat;
use crate::medical::diseases::{emote, say_to};

pub const NAME: &str = "Fake Virus";

/// `/datum/status_effect/fake_virus` `duration`, `tick_interval`.
pub const DURATION_SECS: f64 = 180.0;
pub const TICK_SECS: f64 = 0.2;

/// `/datum/status_effect/fake_virus` on a mob.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct FakeVirus {
    pub until: f64,
    pub next_tick: f64,
    /// `msg_stage`: ticks so far.
    pub stage: u32,
}

/// One delayed symptom of a light victim (the event's `addtimer`).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct FakeCough {
    pub at: f64,
    /// A headache line, else this emote.
    pub line: Option<&'static str>,
    pub emote: &'static str,
}

pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(FakeVirusEvent)
}

struct FakeVirusEvent;

/// `IS_UNCONSCIOUS_OR_CRIT`.
fn awake(world: &World, mob: Entity) -> bool {
    world.get::<Stat>(mob) == Some(&Stat::Conscious)
        && !crate::status_effects::knocked_out(world, mob)
}

/// Conscious crew humans with a client.
fn victims(world: &mut World) -> Vec<Entity> {
    let mut query = world.query_filtered::<Entity, (
        With<crate::server::PlayerMob>,
        With<crate::jobs::PlayerJob>,
        With<crate::human::HumanLook>,
    )>();
    let mut all: Vec<Entity> = query.iter(world).collect();
    all.retain(|&mob| awake(world, mob));
    all.sort();
    all
}

/// `apply_status_effect(/datum/status_effect/fake_virus)` (`STATUS_EFFECT_REPLACE`).
pub fn apply(world: &mut World, mob: Entity) -> bool {
    if !awake(world, mob) {
        return false;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    world.entity_mut(mob).insert(FakeVirus {
        until: now + DURATION_SECS,
        next_tick: now + TICK_SECS,
        stage: 0,
    });
    true
}

impl RoundEvent for FakeVirusEvent {
    fn start(&mut self, world: &mut World, _: &mut EventState) {
        let mut rng = CombatRng(world.resource_mut::<EventRng>().0.next_u64());
        let mut pool = victims(world);
        let hard = pool.len().min(3);
        if hard == 0 {
            return;
        }
        for _ in 0..rng.range(1, hard as u32) {
            let mob = pool.swap_remove(rng.range(0, pool.len() as u32 - 1) as usize);
            apply(world, mob);
        }
        let light = pool.len().min(5);
        if light == 0 {
            return;
        }
        let now = world.resource::<Time>().elapsed_secs_f64();
        for _ in 0..rng.range(1, light as u32) {
            let mob = pool.swap_remove(rng.range(0, pool.len() as u32 - 1) as usize);
            let (line, emote) = if rng.chance(25.0) {
                let line = if rng.range(0, 1) == 0 {
                    "Your head hurts."
                } else {
                    "Your head pounds."
                };
                (Some(line), "")
            } else {
                (
                    None,
                    if rng.range(0, 1) == 0 {
                        "cough"
                    } else {
                        "sniff"
                    },
                )
            };
            let at = now + f64::from(rng.range(30, 150)) / 10.0;
            world.entity_mut(mob).insert(FakeCough { at, line, emote });
        }
    }
}

fn pick(rng: &mut CombatRng, lines: &[&'static str]) -> &'static str {
    lines[rng.range(0, lines.len() as u32 - 1) as usize]
}

/// One `tick()` at `msg_stage`: a line (with its span) or an emote.
pub fn tick_effect(
    rng: &mut CombatRng,
    stage: u32,
) -> Option<Result<(Span, &'static str), &'static str>> {
    match stage {
        0..=300 => {
            if !rng.chance(1.0) {
                return None;
            }
            let groups: [&[&str]; 6] = [
                &["Your head hurts.", "Your head pounds."],
                &[
                    "You're having difficulty breathing.",
                    "Your breathing becomes heavy.",
                ],
                &["You feel dizzy.", "Your head spins."],
                &["You swallow excess mucus.", "You lightly cough."],
                &["Your head hurts.", "Your mind blanks for a moment."],
                &["Your throat hurts.", "You clear your throat."],
            ];
            let group = groups[rng.range(0, 5) as usize];
            Some(Ok((Span::Warning, pick(rng, group))))
        }
        301..=600 => {
            if !rng.chance(2.0) {
                return None;
            }
            let line = match rng.range(0, 4) {
                0 => pick(
                    rng,
                    &["Your head hurts a lot.", "Your head pounds incessantly."],
                ),
                1 => pick(
                    rng,
                    &[
                        "Your windpipe feels like a straw.",
                        "Your breathing becomes tremendously difficult.",
                    ],
                ),
                2 => pick(
                    rng,
                    &[
                        "You feel very dizzy.",
                        "You feel very woozy.",
                        "You feel very faint.",
                    ],
                ),
                3 => pick(rng, &["You hear a ringing in your ear.", "Your ears pop."]),
                _ => "You nod off for a moment.",
            };
            Some(Ok((Span::Warning, line)))
        }
        _ => {
            if !rng.chance(3.0) {
                return None;
            }
            if rng.chance(50.0) {
                Some(Ok(match rng.range(0, 2) {
                    0 => (
                        Span::BoldWarning,
                        pick(
                            rng,
                            &[
                                "Your head hurts!",
                                "You feel a burning knife inside your brain!",
                                "A wave of pain fills your head!",
                            ],
                        ),
                    ),
                    1 => (
                        Span::BoldWarning,
                        pick(rng, &["Your lungs hurt!", "It hurts to breathe!"]),
                    ),
                    _ => (
                        Span::Warning,
                        pick(
                            rng,
                            &[
                                "You feel nauseated.",
                                "You feel like you're going to throw up!",
                            ],
                        ),
                    ),
                }))
            } else if rng.chance(40.0) {
                Some(Err("cough"))
            } else {
                Some(Err("sneeze"))
            }
        }
    }
}

/// The status effect's ticks and the delayed single symptoms.
pub fn process_fake_viruses(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut query = world.query::<(Entity, &FakeVirus)>();
    let sick: Vec<(Entity, FakeVirus)> = query.iter(world).map(|(e, f)| (e, f.clone())).collect();
    let mut query = world.query::<(Entity, &FakeCough)>();
    let coughs: Vec<(Entity, FakeCough)> = query.iter(world).map(|(e, f)| (e, f.clone())).collect();
    if sick.is_empty() && coughs.is_empty() {
        return;
    }
    let mut rng = crate::medical::fork_rng(world);
    for (mob, mut virus) in sick {
        while virus.next_tick <= now && virus.next_tick <= virus.until {
            virus.next_tick += TICK_SECS;
            match tick_effect(&mut rng, virus.stage) {
                Some(Ok((span, line))) => say_to(world, mob, span, line),
                Some(Err(key)) => emote(world, mob, key),
                None => {}
            }
            virus.stage += 1;
        }
        if virus.until <= now {
            world.entity_mut(mob).remove::<FakeVirus>();
        } else {
            world.entity_mut(mob).insert(virus);
        }
    }
    for (mob, cough) in coughs {
        if cough.at > now {
            continue;
        }
        world.entity_mut(mob).remove::<FakeCough>();
        match cough.line {
            Some(line) => say_to(world, mob, Span::Warning, line),
            None => emote(world, mob, cough.emote),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_escalate() {
        let mut rng = CombatRng(3);
        let mut early = 0;
        let mut late_emotes = 0;
        for _ in 0..20_000 {
            if let Some(Ok((span, _))) = tick_effect(&mut rng, 10) {
                assert_eq!(span, Span::Warning);
                early += 1;
            }
            if let Some(Err(key)) = tick_effect(&mut rng, 700) {
                assert!(key == "cough" || key == "sneeze");
                late_emotes += 1;
            }
        }
        // 1% a tick early; 3% * 50% emotes late.
        assert!((120..=290).contains(&early), "{early}");
        assert!((180..=420).contains(&late_emotes), "{late_emotes}");
    }
}
