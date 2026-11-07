//! `stage_act` and the per-tick disease processing.

use super::*;

/// What one `stage_act` asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Keep,
    Cure {
        resistance: bool,
    },
    /// The host is gone (gibbed).
    Gone,
}

/// The host's state `stage_act` reads.
#[derive(Clone, Copy, Debug)]
pub struct StageCtx {
    pub secs: f32,
    /// `has_cure`: 0 none; 1 all (an advanced disease: the remedied share of its symptoms).
    pub cure_status: f32,
    /// `TRAIT_VIRUS_RESISTANCE` (spaceacillin).
    pub virus_resistant: bool,
    pub malnourished: bool,
    pub has_required_organ: bool,
    /// `get_immunity_recovery`.
    pub immunity_recovery: f32,
    /// The mood's recovery bonus (`sanity_level`: +0.4 great ... -0.4 insane).
    pub mood_recovery: f32,
    /// Asleep and not malnourished: the sleeping bonuses (bed, bedsheet, pillow, deafness),
    /// then everything ×`DISEASE_SLEEPING_RECOVERY_MULTIPLIER`.
    pub sleep_bonus: Option<f32>,
}

/// `DISEASE_GOOD_SLEEPING_RECOVERY_BONUS`, `DISEASE_SLEEPING_RECOVERY_MULTIPLIER`.
pub(super) const GOOD_SLEEPING_RECOVERY_BONUS: f32 = 0.6;
pub(super) const SLEEPING_RECOVERY_MULTIPLIER: f32 = 6.0;

/// `stage_act`'s mood recovery bonus.
pub(super) fn mood_recovery(sanity_level: Option<u8>) -> f32 {
    use crate::mood::{
        SANITY_LEVEL_CRAZY, SANITY_LEVEL_GREAT, SANITY_LEVEL_INSANE, SANITY_LEVEL_NEUTRAL,
    };
    match sanity_level {
        Some(SANITY_LEVEL_GREAT) => 0.4,
        Some(SANITY_LEVEL_NEUTRAL) => 0.2,
        Some(SANITY_LEVEL_CRAZY) => -0.2,
        Some(SANITY_LEVEL_INSANE) => -0.4,
        _ => 0.0,
    }
}

/// `stage_act`'s sleeping bonuses for the paths on the sleeper's tile.
pub(super) fn sleep_recovery_bonus(tile: &[String], deaf: bool) -> f32 {
    use crate::obj_damage::is_under;
    let on = |root: &str| tile.iter().any(|p| is_under(p, root));
    let mut bonus = 0.0;
    if deaf {
        bonus += GOOD_SLEEPING_RECOVERY_BONUS;
    }
    if on("/obj/structure/bed") {
        bonus += GOOD_SLEEPING_RECOVERY_BONUS;
    } else if on("/obj/structure/table") {
        bonus += GOOD_SLEEPING_RECOVERY_BONUS / 2.0;
    }
    if on("/obj/item/bedsheet") {
        bonus += GOOD_SLEEPING_RECOVERY_BONUS;
    }
    if on("/obj/item/pillow") {
        bonus += GOOD_SLEEPING_RECOVERY_BONUS;
    }
    bonus
}

/// The base `stage_act`: `Keep` with `true` when the symptoms act this tick.
// shortcut: `bad_immune` is always 1 (no immunodeficiency).
pub fn stage_act(disease: &mut Disease, ctx: &StageCtx, rng: &mut CombatRng) -> (Outcome, bool) {
    let def = disease.def();
    let slowdown = if ctx.virus_resistant { 0.5 } else { 1.0 };
    if !ctx.has_required_organ {
        return (Outcome::Cure { resistance: false }, false);
    }
    if ctx.cure_status > 0.0 {
        let mut cure_mod = disease.cure_chance;
        if disease.advance.is_some() {
            cure_mod *= 2.0 * ctx.cure_status;
        }
        if def.disease_flags & CURABLE != 0 && spt_prob(rng, cure_mod, ctx.secs) {
            return (Outcome::Cure { resistance: true }, false);
        }
    }
    if disease.stage == disease.max_stages {
        disease.stage_peaked = true;
    }
    if spt_prob(rng, disease.stage_prob * slowdown * 2.0, ctx.secs) {
        if disease.half_stage {
            disease.half_stage = false;
            let next = (disease.stage + 1).min(disease.max_stages);
            disease.update_stage(next);
        } else if disease.stage <= disease.max_stages {
            disease.half_stage = true;
        }
    }
    if def.disease_flags & CURABLE != 0 && !def.bypasses_immunity {
        let cycles = match disease.severity {
            Severity::Positive if slowdown >= 1.0 && !ctx.malnourished => {
                return (Outcome::Keep, !disease.carrier);
            }
            severity => severity.cycles(),
        };
        let cycles_to_beat = cycles.max(RECOVERY_SCALING);
        disease.peaked_cycles += f32::from(disease.stage) / f32::from(disease.max_stages.max(1));
        let mut recovery = disease.peaked_cycles / (cycles_to_beat / RECOVERY_SCALING);
        if disease.stage_peaked {
            recovery *= PEAKED_RECOVERY_MULTIPLIER;
        }
        if slowdown < 1.0 {
            recovery += ((1.0 - slowdown)
                * (SLOWDOWN_RECOVERY_BONUS * 2.0)
                * ((SLOWDOWN_RECOVERY_BONUS_DURATION - disease.chemical_offsets)
                    / SLOWDOWN_RECOVERY_BONUS_DURATION))
                .clamp(0.0, SLOWDOWN_RECOVERY_BONUS);
            disease.chemical_offsets =
                (disease.chemical_offsets + 1.0).min(SLOWDOWN_RECOVERY_BONUS_DURATION);
        }
        if ctx.malnourished {
            recovery -= MALNUTRITION_RECOVERY_PENALTY;
        }
        recovery += ctx.mood_recovery;
        recovery += ctx.immunity_recovery;
        if let Some(bonus) = ctx.sleep_bonus {
            recovery += bonus;
            recovery *= SLEEPING_RECOVERY_MULTIPLIER;
        }
        let recovery = recovery.clamp(0.0, 100.0);
        if recovery > 0.0 {
            let failure = 1.0 - disease.recovery_failure_chance() / 100.0;
            if spt_prob(rng, recovery * failure, ctx.secs) {
                if disease.stage == 1
                    && rng.chance(disease.cure_chance * FINAL_CURE_CHANCE_MULTIPLIER)
                {
                    if disease.severity == Severity::Positive && slowdown < 1.0 {
                        return (Outcome::Keep, !disease.carrier);
                    }
                    if !ctx.malnourished {
                        return (Outcome::Cure { resistance: true }, false);
                    }
                    if !disease.stage_peaked {
                        return (Outcome::Cure { resistance: false }, false);
                    }
                    if rng.chance(50.0) {
                        return (Outcome::Cure { resistance: true }, false);
                    }
                }
                let lower = disease.stage.saturating_sub(1).max(1);
                disease.update_stage(lower);
            }
        }
        // Spaceacillin (and sleep) hold the symptoms back, less and less over time.
        if slowdown != 1.0 {
            let held = (100.0 * (disease.symptom_offsets / SYMPTOM_OFFSET_DURATION))
                .min(100.0 - disease.cure_chance * FINAL_CURE_CHANCE_MULTIPLIER);
            if rng.chance(100.0 - held) {
                disease.symptom_offsets =
                    (disease.symptom_offsets + 1.0).min(SYMPTOM_OFFSET_DURATION);
                return (Outcome::Keep, false);
            }
        }
    }
    (Outcome::Keep, !disease.carrier)
}

/// tg `has_cure` of a simple disease: all its cures (subtypes count) are in the blood.
pub(super) fn has_cure(world: &World, mob: Entity, disease: &Disease) -> f32 {
    let def = disease.def();
    if def.disease_flags & CURABLE == 0 {
        return 0.0;
    }
    if let Some(advance) = &disease.advance {
        return advance.cure_status();
    }
    let all = !def.cures.is_empty() && def.cures.iter().all(|c| blood_has(world, mob, c, true));
    if all { 1.0 } else { 0.0 }
}

/// Every disease of `mob` for one `SSmobs` fire of `secs` (tg `on_life` and `on_breath`).
// Cost: every 2 s, per human with diseases (few), a handful of rolls per disease; airborne
// spread is one radius-2 index query per breath of an airborne host.
pub fn life(world: &mut World, mob: Entity, secs: f32) {
    let Some(all) = world.get::<Diseases>(mob) else {
        return;
    };
    if all.list.is_empty() {
        return;
    }
    let ids: Vec<String> = all.list.iter().map(Disease::id).collect();
    let dead = world.get::<Stat>(mob) == Some(&Stat::Dead);
    let mut rng = super::super::fork_rng(world);
    // `on_breath`: `COMSIG_CARBON_PRE_BREATHE` fires on the breathing fires (every 4th).
    let breathes = !dead
        && world
            .get_resource::<crate::breath::MobsClock>()
            .is_none_or(|c| c.times_fired.is_multiple_of(4));
    for id in ids {
        if world.get_entity(mob).is_err() {
            return;
        }
        let Some(mut disease) = of_mob(world, mob).iter().find(|d| d.id() == id).cloned() else {
            continue;
        };
        if breathes
            && disease.spread_flags & SPREAD_AIRBORNE != 0
            && spt_prob(&mut rng, INFECTIVITY * 4.0, secs)
        {
            airborne_spread(world, mob, &disease, 2, true, false, &mut rng);
        }
        if dead && !disease.def().process_dead {
            continue;
        }
        let malnourished = world
            .get::<crate::hunger::Hunger>(mob)
            .is_some_and(|h| h.nutrition <= crate::hunger::NUTRITION_LEVEL_STARVING);
        let sleeping = world
            .get::<crate::status_effects::StatusEffects>(mob)
            .is_some_and(|e| e.sleeping > 0.0);
        let sleep_bonus = (sleeping && !malnourished).then(|| {
            sleep_recovery_bonus(
                &crate::status_effects::tile_paths(world, mob),
                !crate::sound::can_hear(world, mob),
            )
        });
        let ctx = StageCtx {
            secs,
            cure_status: has_cure(world, mob, &disease),
            virus_resistant: virus_resistant(world, mob),
            malnourished,
            mood_recovery: mood_recovery(
                world.get::<crate::mood::Mood>(mob).map(|m| m.sanity_level),
            ),
            sleep_bonus,
            has_required_organ: disease
                .def()
                .required_organ
                .is_none_or(|slot| has_organ(world, mob, slot)),
            immunity_recovery: disease.advance.as_ref().map_or(0.0, |a| {
                let resisted = world
                    .get::<Diseases>(mob)
                    .map_or(&[][..], |d| d.symptom_resistances.as_slice());
                a.immunity_recovery(resisted)
            }),
        };
        let (mut outcome, act) = stage_act(&mut disease, &ctx, &mut rng);
        if act {
            outcome = match disease.kind {
                Kind::Advance => symptoms::act(world, mob, &mut disease, &mut rng),
                _ => kind_act(world, mob, &mut disease, secs, &mut rng),
            };
        }
        if outcome == Outcome::Gone || world.get_entity(mob).is_err() {
            return;
        }
        let Some(index) = of_mob(world, mob).iter().position(|d| d.id() == id) else {
            continue;
        };
        match outcome {
            Outcome::Cure { resistance } => cure(world, mob, index, resistance),
            _ => {
                if let Some(mut all) = world.get_mut::<Diseases>(mob)
                    && all.list[index] != disease
                {
                    all.list[index] = disease;
                }
            }
        }
    }
}
