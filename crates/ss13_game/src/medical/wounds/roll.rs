//! Rolling wounds from a hit, applying and removing them, and dismemberment checks.

use super::*;

/// What a hit brings to the wound roll (tg `receive_damage` args).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub kind: DamageType,
    /// The damage the limb took (after the wound damage multiplier).
    pub damage: f32,
    pub sharpness: u32,
    pub wound_bonus: f32,
    pub exposed_wound_bonus: f32,
    /// The limb's damage before the hit (tg checks wounding before applying it).
    pub part_damage_before: f32,
}

/// The owner's traits the wound rolls read: `TRAIT_EASYDISMEMBER`, `TRAIT_NODISMEMBER`,
/// `TRAIT_EASILY_WOUNDED`, `TRAIT_HARDLY_WOUNDED`; and the `wound` armor of the clothing on the
/// hit bodypart (`crate::armor::worn_wound_armor`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WoundTraits {
    pub easy_dismember: bool,
    pub no_dismember: bool,
    pub easily_wounded: bool,
    pub hardly_wounded: bool,
    pub worn_wound_armor: f32,
}

impl WoundTraits {
    pub fn of(world: &World, mob: Entity) -> Self {
        use crate::species::{Trait, has_trait};
        Self {
            easy_dismember: has_trait(world, mob, Trait::EasyDismember),
            no_dismember: has_trait(world, mob, Trait::NoDismember),
            easily_wounded: super::super::bioware::easily_wounded(world, mob)
                || crate::quirks::has(world, mob, crate::quirks::Quirk::Frail),
            hardly_wounded: has_trait(world, mob, Trait::HardlyWounded),
            worn_wound_armor: 0.0,
        }
    }

    /// `check_wounding`'s damage: capped at `WOUND_MAX_CONSIDERED_DAMAGE` (×1.5 easily
    /// wounded, ÷2 hardly), ×1.1 for an easy dismember that is not `TRAIT_NODISMEMBER`.
    pub fn considered_damage(self, damage: f32) -> f32 {
        let mut cap = WOUND_MAX_CONSIDERED_DAMAGE;
        if self.easily_wounded && !self.hardly_wounded {
            cap *= 1.5;
        } else if self.hardly_wounded && !self.easily_wounded {
            cap /= 2.0;
        }
        let mut damage = damage.min(cap);
        if self.easy_dismember && !self.no_dismember {
            damage *= 1.1;
        }
        damage
    }
}

/// The wounding type and damage of a hit on a flesh-and-bone limb (tg `receive_damage`'s
/// wound handling up to `check_wounding`): `None` for damage that does not wound.
pub fn wounding_of(hit: Hit, limb: &Limb) -> Option<(WoundType, f32)> {
    wounding_of_with(hit, limb, false)
}

/// [`wounding_of`] of an owner with `TRAIT_EASYDISMEMBER` or not: an easy dismember passes
/// sharp hits on to the bone in full.
pub fn wounding_of_with(hit: Hit, limb: &Limb, easy_dismember: bool) -> Option<(WoundType, f32)> {
    let (brute, burn) = match hit.kind {
        DamageType::Brute => (hit.damage, 0.0),
        DamageType::Burn => (0.0, hit.damage),
        _ => return None,
    };
    let mut wounding = if brute > burn {
        WoundType::Blunt
    } else {
        WoundType::Burn
    };
    let mut damage = brute.max(burn);
    if wounding == WoundType::Blunt && hit.sharpness != 0 {
        if hit.sharpness & SHARP_EDGED != 0 {
            wounding = WoundType::Slash;
        } else if hit.sharpness & SHARP_POINTY != 0 {
            wounding = WoundType::Pierce;
        }
    }
    // A mangled skin passes sharp hits on to the bone, at 60% (edged) or 75% (pointy).
    let (exterior, interior) = limb.mangled();
    if exterior && !interior && hit.sharpness != 0 {
        if wounding == WoundType::Slash && !easy_dismember {
            damage *= 0.6;
        }
        if wounding == WoundType::Pierce && !easy_dismember {
            damage *= 0.75;
        }
        wounding = WoundType::Blunt;
    }
    Some((wounding, damage))
}

/// tg `wound_resistance` and `disabled_wound_penalty` of a human bodypart.
pub(super) fn resistance(zone: BodyZone) -> (f32, f32) {
    match zone {
        BodyZone::Head => (5.0, 25.0),
        BodyZone::Chest => (10.0, 15.0),
        _ => (0.0, 15.0),
    }
}

/// tg `check_wounding` on a human bodypart: the wound to apply, if any.
/// Without clothing (`WoundTraits::default`).
pub fn check_wounding(
    rng: &mut CombatRng,
    limb: &Limb,
    zone: BodyZone,
    wounding: WoundType,
    damage: f32,
    hit: Hit,
) -> Option<WoundKind> {
    match roll_wounding(rng, limb, zone, wounding, damage, hit, false)? {
        Rolled::Wound(kind) => Some(kind),
        Rolled::Dismember => None,
    }
}

/// What [`roll_wounding`] decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rolled {
    Wound(WoundKind),
    /// The roll beat `WOUND_DISMEMBER_OUTRIGHT_THRESH`: the limb comes off outright.
    Dismember,
}

/// tg `check_wounding` with the outright dismemberment of a limb that `can_dismember`.
pub fn roll_wounding(
    rng: &mut CombatRng,
    limb: &Limb,
    zone: BodyZone,
    wounding: WoundType,
    damage: f32,
    hit: Hit,
    can_dismember: bool,
) -> Option<Rolled> {
    roll_wounding_with(
        rng,
        limb,
        zone,
        wounding,
        damage,
        hit,
        can_dismember,
        WoundTraits::default(),
    )
}

/// [`roll_wounding`] of an owner with `traits`.
#[allow(clippy::too_many_arguments)]
pub fn roll_wounding_with(
    rng: &mut CombatRng,
    limb: &Limb,
    zone: BodyZone,
    wounding: WoundType,
    damage: f32,
    hit: Hit,
    can_dismember: bool,
    traits: WoundTraits,
) -> Option<Rolled> {
    let damage = traits.considered_damage(damage);
    let top = (damage.powf(WOUND_DAMAGE_EXPONENT)).round().max(1.0) as u32;
    let base_roll = rng.range(1, top) as f32;
    let injury_roll = woundings_mods(limb, zone, base_roll, hit, traits.worn_wound_armor);
    if can_dismember
        && injury_roll > WOUND_DISMEMBER_OUTRIGHT_THRESH
        && rng.chance(hit.part_damage_before / zone.max_damage() * 100.0)
    {
        return Some(Rolled::Dismember);
    }
    let mut series_mods = [0.0f32; 4];
    for wound in &limb.wounds {
        series_mods[wound.def().series as usize] += wound.def().series_threshold_penalty;
    }
    let mut possible: Vec<WoundKind> = WoundKind::ALL
        .into_iter()
        .filter(|kind| {
            let def = kind.def();
            def.threshold_minimum <= injury_roll + series_mods[def.series as usize]
                && can_be_applied(limb, zone, *kind, wounding)
        })
        .collect();
    // `WOUND_COMPETITION_OVERPOWER_LESSERS`: the worst severity wins.
    let worst = possible.iter().map(|k| k.def().severity).max()?;
    possible.retain(|k| k.def().severity == worst);
    // Every candidate weighs `WOUND_DEFAULT_WEIGHT`; one per series and severity remains.
    let pick = rng.range(0, possible.len() as u32 - 1) as usize;
    Some(Rolled::Wound(possible[pick]))
}

/// tg `try_dismember` of a limb with mangled skin and bone: the hit's wounding damage plus half
/// the limb's damage share, +15 per critical blunt wound.
pub(super) fn try_dismember(
    rng: &mut CombatRng,
    limb: &Limb,
    zone: BodyZone,
    damage: f32,
    hit: Hit,
) -> bool {
    if damage < DISMEMBER_MINIMUM_DAMAGE {
        return false;
    }
    let mut chance = damage + hit.part_damage_before / zone.max_damage() * 50.0;
    for wound in &limb.wounds {
        let def = wound.def();
        if def.wounding == WoundType::Blunt && def.severity >= 3 {
            chance += CRITICAL_BLUNT_DISMEMBER_BONUS;
        }
    }
    rng.chance(chance)
}

/// tg `check_woundings_mods`; `worn` is the clothing's `wound` armor on the bodypart.
pub fn woundings_mods(limb: &Limb, zone: BodyZone, roll: f32, hit: Hit, worn: f32) -> f32 {
    let (resist, disabled_penalty) = resistance(zone);
    let mut armor = worn;
    let mut injury = roll + hit.wound_bonus;
    for wound in &limb.wounds {
        injury += wound.threshold_penalty;
    }
    let (exterior, interior) = limb.mangled();
    if !exterior && !interior {
        armor += resist;
    }
    if hit.part_damage_before >= zone.max_damage() {
        injury += disabled_penalty;
    }
    if armor == 0.0 {
        injury += hit.exposed_wound_bonus;
    } else {
        injury *= (100.0 - armor) / 100.0;
    }
    injury
}

/// tg `/datum/wound_pregen_data/can_be_applied_to` with `random_roll`.
pub(super) fn can_be_applied(
    limb: &Limb,
    zone: BodyZone,
    kind: WoundKind,
    wounding: WoundType,
) -> bool {
    let def = kind.def();
    if def.wounding != wounding {
        return false;
    }
    if def.jointed_only && matches!(zone, BodyZone::Head | BodyZone::Chest) {
        return false;
    }
    !limb
        .wounds
        .iter()
        .any(|w| w.def().series == def.series && w.def().severity >= def.severity)
}

/// What happened to a limb's wounds, for the caller's messages and side effects.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HitOutcome {
    /// A new or worse wound (tg `apply_wound` without `demoted`).
    pub gained: Option<WoundKind>,
    /// Blood lost at once by internal bleeding, with the victim's message.
    pub bled: Vec<(f32, &'static str)>,
    /// A bone wound wants the item in this hand dropped.
    pub drop_hand: Option<usize>,
    /// The limb comes off (tg `/datum/wound/loss/apply_dismember`): the wounding type and
    /// whether outright; nothing else happened to it.
    pub dismember: Option<(WoundType, bool)>,
}

/// tg `apply_wound` (or `replace_wound` of a milder wound of the series) on `limb`.
pub fn apply_wound(limb: &mut Limb, kind: WoundKind) {
    let def = kind.def();
    let old = limb
        .wounds
        .iter()
        .position(|w| w.def().series == def.series)
        .map(|i| remove_wound(limb, i));
    let mut wound = Wound::new(kind);
    // `set_limb`: states overridden by what surgery did are left out.
    for (st, over) in [
        (state::SKIN_CUT, state::SKIN_OPEN),
        (state::VESSELS_UNCLAMPED, state::VESSELS_CLAMPED),
        (state::BONE_SAWED, state::BONE_DRILLED),
        (state::BONE_DRILLED, state::BONE_SAWED),
    ] {
        if limb.surgery & over != 0 {
            wound.surgery_states &= !st;
        }
    }
    limb.surgery |= wound.surgery_states;
    // `wound_injury`.
    match def.series {
        Series::Slash => {
            wound.blood_flow = old
                .as_ref()
                .map_or(def.initial_flow, |o| o.blood_flow.max(def.initial_flow));
        }
        Series::Pierce => wound.blood_flow = def.initial_flow,
        _ => {}
    }
    // A new cut or puncture through a gauze soaks 80% of it; a burn destroys it.
    if old.is_none() {
        match def.series {
            Series::Slash | Series::Pierce => {
                if let Some(gauze) = &mut limb.gauze {
                    gauze.absorption_capacity -= gauze_initial_capacity(gauze) * 0.8;
                    if gauze.absorption_capacity <= 0.0 {
                        limb.gauze = None;
                    }
                }
            }
            Series::Burn => limb.gauze = None,
            Series::Bone => {}
        }
    }
    if def.disabling {
        wound.disabling = !limb.is_splinted();
    }
    limb.wounds.push(wound);
}

/// The wrap's original capacity: tg reads `initial(absorption_capacity)` of its type; every
/// wrap here starts at its type's value, so the remaining capacity rounded up stands in.
// shortcut: the initial capacity is not kept per wrap; gauze types start at 5 or 4.
pub(super) fn gauze_initial_capacity(gauze: &Gauze) -> f32 {
    gauze.absorption_capacity.max(4.0).ceil()
}

/// tg `update_inefficiencies` after a wrap went on or off: splinted fractures stop disabling.
pub fn refresh_splints(limb: &mut Limb) {
    let splinted = limb.is_splinted();
    for wound in &mut limb.wounds {
        if wound.def().disabling {
            wound.disabling = !splinted;
        }
    }
}

/// tg `remove_wound`: the wound's own surgery states leave the limb with it.
pub fn remove_wound(limb: &mut Limb, index: usize) -> Wound {
    let wound = limb.wounds.remove(index);
    if wound.surgery_states != 0 {
        limb.surgery &= !wound.surgery_states;
    }
    wound
}

/// The wound part of tg `receive_damage` on a human bodypart: the dismemberment roll of a
/// mangled limb that `can_dismember`, the wound roll, then every wound's `receive_damage` (the
/// new one included).
pub fn receive_damage(
    rng: &mut CombatRng,
    limb: &mut Limb,
    zone: BodyZone,
    hit: Hit,
    dead: bool,
    has_blood: bool,
    can_dismember: bool,
) -> HitOutcome {
    receive_damage_with(
        rng,
        limb,
        zone,
        hit,
        dead,
        has_blood,
        can_dismember,
        WoundTraits::default(),
    )
}

/// [`receive_damage`] of an owner with `traits`.
#[allow(clippy::too_many_arguments)]
pub fn receive_damage_with(
    rng: &mut CombatRng,
    limb: &mut Limb,
    zone: BodyZone,
    hit: Hit,
    dead: bool,
    has_blood: bool,
    can_dismember: bool,
    traits: WoundTraits,
) -> HitOutcome {
    let mut out = HitOutcome::default();
    if limb.robotic {
        return out;
    }
    let Some((wounding, damage)) = wounding_of_with(hit, limb, traits.easy_dismember) else {
        return out;
    };
    let (exterior, interior) = limb.mangled();
    if can_dismember && exterior && interior && try_dismember(rng, limb, zone, damage, hit) {
        out.dismember = Some((wounding, false));
        return out;
    }
    let rolled = if damage >= WOUND_MINIMUM_DAMAGE && hit.wound_bonus != CANT_WOUND {
        roll_wounding_with(
            rng,
            limb,
            zone,
            wounding,
            damage,
            hit,
            can_dismember,
            traits,
        )
    } else {
        None
    };
    if rolled == Some(Rolled::Dismember) {
        out.dismember = Some((wounding, true));
        return out;
    }
    if let Some(Rolled::Wound(kind)) = rolled {
        let def = kind.def();
        apply_wound(limb, kind);
        out.gained = Some(kind);
        // Bone `wound_injury`: the hand of a broken arm may drop what it holds.
        if def.series == Series::Bone {
            let hand = match zone {
                BodyZone::LArm => Some(0),
                BodyZone::RArm => Some(1),
                _ => None,
            };
            let disabling = limb.wounds.last().is_some_and(|w| w.disabling);
            if hand.is_some() && (disabling || rng.chance(30.0 * f32::from(def.severity))) {
                out.drop_hand = hand;
            }
        }
    }
    let splint = limb.splint_factor();
    for wound in &mut limb.wounds {
        let def = wound.def();
        match def.series {
            Series::Slash => {
                if !dead && hit.wound_bonus != CANT_WOUND && wounding == WoundType::Slash {
                    wound.blood_flow = (wound.blood_flow + WOUND_SLASH_DAMAGE_FLOW_COEFF * damage)
                        .min(WOUND_MAX_BLOODFLOW);
                }
            }
            Series::Pierce => {
                if dead
                    || damage < 5.0
                    || !has_blood
                    || !rng.chance(def.internal_bleeding_chance + damage)
                {
                    continue;
                }
                let factor = [0.75, 1.0, 1.25, 1.5][rng.range(0, 3) as usize];
                let bled = damage.sqrt() * def.internal_bleeding_coefficient * splint * factor;
                let text = if bled >= 12.0 {
                    "Вы выплевываете струю крови из-за удара!"
                } else if bled >= 8.0 {
                    "Вы откашливаетесь, выплевывая немного крови из-за удара."
                } else {
                    ""
                };
                out.bled.push((bled, text));
            }
            Series::Bone => {
                if damage < WOUND_MINIMUM_DAMAGE
                    || !has_blood
                    || zone != BodyZone::Chest
                    || !rng.chance(def.internal_bleeding_chance + damage)
                {
                    continue;
                }
                let factor = if def.severity == 3 { 2.0 } else { 1.5 };
                let bled = rng.range(1, ((damage * factor) as u32).max(1)) as f32;
                let text = match bled as u32 {
                    0..=6 => "",
                    7..=13 => "Вы откашливаете кровь после удара в грудь.",
                    14..=19 => "Вы сплевываете струйкой крови после удара в грудь!",
                    _ => "Вы задыхаетесь от струи крови после удара в грудь!",
                };
                out.bled.push((bled, text));
            }
            Series::Burn => {}
        }
    }
    out
}
