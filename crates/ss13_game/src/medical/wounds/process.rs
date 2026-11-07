//! The wounds' per-tick processing.

use super::*;

/// Something a wound tick asks of the body.
#[derive(Clone, Debug, PartialEq)]
pub enum TickEffect {
    Tox(f32),
    /// `take_bodypart_damage(brute)` + stamina (bone regrowth pains).
    Pain {
        brute: f32,
        stamina: f32,
    },
    Tell(Span, String),
}

/// The body around a wound tick.
#[derive(Clone, Copy, Debug)]
pub struct TickCtx {
    pub secs: f32,
    pub lying: bool,
    pub bodytemperature: f64,
    pub nutrition: f32,
    pub burn_damage: f32,
    pub has_blood: bool,
}

/// tg `handle_process` of every wound on `limb` (one `Life()` tick). Wounds that heal are
/// removed; returns the effects on the body.
pub fn process(
    rng: &mut CombatRng,
    limb: &mut Limb,
    zone: BodyZone,
    ctx: TickCtx,
) -> Vec<TickEffect> {
    let mut effects = Vec::new();
    let mut i = 0;
    while i < limb.wounds.len() {
        let def = limb.wounds[i].def();
        let keep = match def.series {
            Series::Slash => process_slash(limb, i, ctx, &mut effects, zone),
            Series::Pierce => process_pierce(limb, i, ctx, &mut effects, zone),
            Series::Bone => process_bone(rng, limb, i, ctx, &mut effects),
            Series::Burn => process_burn(rng, limb, i, ctx, &mut effects),
        };
        if keep {
            i += 1;
        }
    }
    effects
}

/// tg `seep_gauze`: true when there is a gauze (it may have fallen apart).
pub fn seep_gauze(
    limb: &mut Limb,
    amount: f32,
    zone: BodyZone,
    effects: &mut Vec<TickEffect>,
) -> bool {
    let Some(gauze) = &mut limb.gauze else {
        return false;
    };
    gauze.absorption_capacity -= amount;
    if gauze.absorption_capacity <= 0.0 {
        effects.push(TickEffect::Tell(
            Span::Warning,
            format!(
                "{} на вашей {} разваливается в лохмотья.",
                gauze.name,
                zone_prepositional(zone)
            ),
        ));
        limb.gauze = None;
    }
    true
}

/// `adjust_blood_flow` of the slash at `i`: clamps it, demotes or closes it. False when it is
/// no longer at `i` (closed, or a replacement went to the end).
pub fn adjust_slash(
    limb: &mut Limb,
    i: usize,
    by: f32,
    zone: BodyZone,
    effects: &mut Vec<TickEffect>,
) -> bool {
    let def = limb.wounds[i].def();
    let wound = &mut limb.wounds[i];
    wound.blood_flow = (wound.blood_flow + by).clamp(0.0, WOUND_MAX_BLOODFLOW);
    if wound.blood_flow >= def.minimum_flow {
        return true;
    }
    match def.demotes_to {
        // `replace_wound`: the milder cut keeps the flow if it is higher than its own.
        Some(lesser) => apply_wound(limb, lesser),
        None => {
            remove_wound(limb, i);
            effects.push(TickEffect::Tell(
                Span::Notice,
                format!(
                    "Порез на вашей {} перестал кровоточить!",
                    zone_prepositional(zone)
                ),
            ));
        }
    }
    false
}

pub(super) fn process_slash(
    limb: &mut Limb,
    i: usize,
    ctx: TickCtx,
    effects: &mut Vec<TickEffect>,
    zone: BodyZone,
) -> bool {
    let def = limb.wounds[i].def();
    if ctx.has_blood
        && def.clot_rate > 0.0
        && !adjust_slash(limb, i, -def.clot_rate * ctx.secs, zone, effects)
    {
        return false;
    }
    let rate = limb.gauze.as_ref().map_or(0.0, |g| g.absorption_rate);
    if rate > 0.0 {
        seep_gauze(limb, rate * ctx.secs, zone, effects);
        return adjust_slash(limb, i, -rate * ctx.secs, zone, effects);
    }
    true
}

/// `/datum/wound/pierce/bleed/adjust_blood_flow`: gone at 0.
pub fn adjust_pierce(limb: &mut Limb, i: usize, by: f32) -> bool {
    let wound = &mut limb.wounds[i];
    wound.blood_flow = (wound.blood_flow + by).clamp(0.0, WOUND_MAX_BLOODFLOW);
    if wound.blood_flow <= 0.0 {
        remove_wound(limb, i);
        return false;
    }
    true
}

pub(super) fn process_pierce(
    limb: &mut Limb,
    i: usize,
    ctx: TickCtx,
    effects: &mut Vec<TickEffect>,
    zone: BodyZone,
) -> bool {
    let def = limb.wounds[i].def();
    if ctx.has_blood
        && ctx.bodytemperature < BODYTEMP_NORMAL - 10.0
        && !adjust_pierce(limb, i, -0.1 * ctx.secs)
    {
        return false;
    }
    let rate = limb.gauze.as_ref().map_or(0.0, |g| g.absorption_rate);
    if rate > 0.0 {
        seep_gauze(limb, rate * ctx.secs, zone, effects);
        return adjust_pierce(
            limb,
            i,
            -def.clot_rate * ctx.secs - rate * def.gauzed_clot_rate * ctx.secs,
        );
    }
    if ctx.has_blood {
        return adjust_pierce(limb, i, -def.clot_rate * ctx.secs);
    }
    true
}

pub(super) fn process_bone(
    rng: &mut CombatRng,
    limb: &mut Limb,
    i: usize,
    ctx: TickCtx,
    effects: &mut Vec<TickEffect>,
) -> bool {
    let wound = &mut limb.wounds[i];
    let severity = f32::from(wound.def().severity);
    // Gel and tape knit a fracture of a flesh limb.
    if !(wound.gelled && wound.taped) {
        return true;
    }
    wound.regen_ticks += 1;
    if ctx.lying && spt_prob(rng, 30.0, ctx.secs) {
        wound.regen_ticks += 1;
    }
    if spt_prob(rng, severity * 1.5, ctx.secs) {
        let brute = rng.range(1, (severity * 2.0) as u32) as f32;
        let stamina = rng.range(2, (severity * 2.5) as u32) as f32;
        effects.push(TickEffect::Pain { brute, stamina });
        if rng.chance(33.0) {
            effects.push(TickEffect::Tell(
                Span::Danger,
                "Вы чувствуете острую боль во всём теле, пока ваши кости срастаются!".into(),
            ));
        }
    }
    if wound.regen_ticks > wound.regen_ticks_needed {
        remove_wound(limb, i);
        return false;
    }
    true
}

pub(super) fn process_burn(
    rng: &mut CombatRng,
    limb: &mut Limb,
    i: usize,
    ctx: TickCtx,
    effects: &mut Vec<TickEffect>,
) -> bool {
    let secs = ctx.secs;
    if limb.wounds[i].strikes_to_lose_limb == 0 {
        effects.push(TickEffect::Tox(0.25 * secs));
        return true;
    }
    let mut dummy = Vec::new();
    let gauzed = limb.gauze.is_some();
    let bandage = limb
        .gauze
        .as_ref()
        .map(|g| g.burn_cleanliness_bonus)
        .filter(|b| *b > 0.0)
        .unwrap_or(1.0);
    if gauzed {
        seep_gauze(
            limb,
            WOUND_BURN_SANITIZATION_RATE * secs,
            BodyZone::Chest,
            &mut dummy,
        );
    }
    let wound = &mut limb.wounds[i];
    if wound.flesh_healing > 0.0 {
        wound.flesh_damage = (wound.flesh_damage - 0.5 * secs).max(0.0);
        wound.flesh_healing = (wound.flesh_healing - 0.5 * bandage * secs).max(0.0);
    }
    if wound.infection <= WOUND_INFECTION_MODERATE
        && ctx.burn_damage < 5.0
        && ctx.nutrition >= NUTRITION_LEVEL_FED
    {
        wound.flesh_healing += 0.2;
    }
    if wound.flesh_damage <= 0.0 && wound.infection <= WOUND_INFECTION_MODERATE {
        remove_wound(limb, i);
        return false;
    }
    if wound.sanitization > 0.0 {
        wound.infection = (wound.infection - WOUND_BURN_SANITIZATION_RATE * secs).max(0.0);
        wound.sanitization =
            (wound.sanitization - WOUND_BURN_SANITIZATION_RATE * bandage * secs).max(0.0);
        return true;
    }
    wound.infection += wound.def().infection_rate * secs;
    let infection = wound.infection;
    if infection <= WOUND_INFECTION_MODERATE {
        return true;
    }
    if infection <= WOUND_INFECTION_SEVERE {
        if spt_prob(rng, 15.0, secs) {
            effects.push(TickEffect::Tox(0.2));
        }
    } else if infection <= WOUND_INFECTION_CRITICAL {
        if !wound.disabling {
            if spt_prob(rng, 1.0, secs) {
                wound.disabling = true;
                return true;
            }
        } else if spt_prob(rng, 4.0, secs) {
            wound.disabling = false;
            return true;
        }
        if spt_prob(rng, 10.0, secs) {
            effects.push(TickEffect::Tox(0.5));
        }
    } else if infection <= WOUND_INFECTION_SEPTIC {
        if !wound.disabling {
            if spt_prob(rng, 1.5, secs) {
                wound.disabling = true;
                return true;
            }
        } else if spt_prob(rng, 1.5, secs) {
            wound.disabling = false;
            return true;
        }
        if spt_prob(rng, 2.48, secs) {
            effects.push(TickEffect::Tox(if rng.chance(20.0) { 0.75 } else { 1.0 }));
        }
    } else if spt_prob(rng, 0.5 * infection, secs) {
        wound.strikes_to_lose_limb = wound.strikes_to_lose_limb.saturating_sub(1);
        if wound.strikes_to_lose_limb == 0 {
            wound.threshold_penalty *= 2.0;
            wound.disabling = true;
            effects.push(TickEffect::Tell(
                Span::Danger,
                "Инфекция полностью поглотила конечность, она больше не двигается.".into(),
            ));
        } else {
            effects.push(TickEffect::Tell(
                Span::Danger,
                "Вы едва чувствуете поражённую конечность, по ней ползёт гниль!".into(),
            ));
        }
    }
    true
}

/// `ru_plaintext_zone[PREPOSITIONAL]`.
pub fn zone_prepositional(zone: BodyZone) -> &'static str {
    match zone {
        BodyZone::Head => "голове",
        BodyZone::Chest => "груди",
        BodyZone::LArm => "левой руке",
        BodyZone::RArm => "правой руке",
        BodyZone::LLeg => "левой ноге",
        BodyZone::RLeg => "правой ноге",
    }
}
