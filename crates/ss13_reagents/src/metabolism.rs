//! Mob metabolism after tg `chemistry/holder/mob_life.dm` (`metabolize`,
//! `metabolize_reagent`), the stomach (`stomach/_stomach.dm` `on_life`), the liver's toxin
//! tolerance and `on_mob_life`/`overdose_process` of a starter set of reagents, with tg's
//! numbers.
//!
//! `metabolization_ratio` is tg's `REM * metabolized_volume`: 1 for a reagent at its own rate
//! over one `SSmobs` tick, so the per-second effects below are multiplied by it and by
//! `seconds_per_tick`.
// shortcut: reagents without an effect here only metabolize (their volume goes down at their
// rate); no organ damage (liver, eyes, lungs), no blood, stamina, traits, addictions, mood or
// inverse-reagent effects. Add per reagent with the systems they need.

use crate::{
    CHEMICAL_VOLUME_ROUNDING, Chemistry, Holder, REAGENTS_METABOLISM, Reagent, ReagentId, is_under,
    reagent_flags, round_to,
};

/// Every reagent under it feeds a human (`/datum/reagent/consumable/on_mob_life`).
pub const CONSUMABLE_ROOT: &str = "/datum/reagent/consumable";

/// tg `SSMOBS_DT`: seconds per `Life()`.
pub const SSMOBS_DT: f32 = 2.0;
/// tg `REM` (`REAGENTS_EFFECT_MULTIPLIER`).
pub const REM: f32 = 1.0 / (REAGENTS_METABOLISM * SSMOBS_DT);
/// tg `STOMACH_METABOLISM_CONSTANT`.
pub const STOMACH_METABOLISM_CONSTANT: f32 = 0.25;
/// `/obj/item/organ/stomach::metabolism_efficiency`.
pub const STOMACH_EFFICIENCY: f32 = 0.05;
/// `LIVER_DEFAULT_TOX_TOLERANCE`: units of a toxin the liver filters without effect.
pub const LIVER_DEFAULT_TOX_TOLERANCE: f32 = 3.0;
/// `ALCOHOL_RATE`, `ALCOHOL_THRESHOLD_MODIFIER`.
pub const ALCOHOL_RATE: f32 = 0.005;
pub const ALCOHOL_THRESHOLD_MODIFIER: f32 = 1.0;
/// `TIPSY_THRESHOLD` of `/datum/status_effect/inebriated`.
pub const TIPSY_THRESHOLD: f32 = 6.0;
/// tg `HEALTH_THRESHOLD_CRIT` (`crit_threshold` of a carbon).
pub const CRIT_THRESHOLD: f32 = 0.0;

/// Damage types the effects use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Damage {
    Brute,
    Burn,
    Tox,
    Oxy,
}

/// What metabolism reads and changes on the mob.
pub trait Body {
    fn health(&self) -> f32;
    fn oxy_loss(&self) -> f32;
    fn dead(&self) -> bool;
    /// tg `adjust_*_loss`: positive damages, negative heals (brute and burn over the body).
    fn adjust(&mut self, kind: Damage, amount: f32);
    /// tg `heal_bodypart_damage(brute, burn)`.
    fn heal_bodypart(&mut self, brute: f32, burn: f32);
    /// tg `losebreath`.
    fn losebreath(&mut self) -> &mut f32;
    /// tg `/datum/status_effect/inebriated::drunk_value` (0: sober).
    fn drunk(&mut self) -> &mut f32;
    /// A line to the mob's own chat (`to_chat`, danger style).
    fn tell(&mut self, text: String);
    /// tg `SPT_PROB(percent, seconds)`.
    fn spt_prob(&mut self, percent: f32, seconds: f32) -> bool;
    /// tg `adjust_nutrition` of a human (a body without hunger ignores it).
    fn adjust_nutrition(&mut self, _amount: f32) {}
    /// tg `metabolism_efficiency` (the stomach's `handle_hunger` sets it from nutrition).
    fn metabolism_efficiency(&self) -> f32 {
        1.0
    }
    /// The factor of ethanol's `boozepwr` on this mob (tg `TRAIT_ALCOHOL_TOLERANCE` ×0.7,
    /// `TRAIT_LIGHT_DRINKER` ×2).
    fn booze_power_multiplier(&self) -> f32 {
        1.0
    }
}

/// tg `/datum/reagent/consumable/get_nutriment_factor`: nutrition per metabolized unit and
/// second, before `0.5 * metabolization_ratio`; 0 for other reagents.
pub fn nutriment_factor(chem: &Chemistry, reagent: &Reagent) -> f32 {
    let def = chem.def(reagent.id);
    if !is_under(&def.path, CONSUMABLE_ROOT) {
        return 0.0;
    }
    def.nutriment_factor * REAGENTS_METABOLISM * reagent.purity * 2.0
}

/// tg `/mob/living/carbon/get_fullness(only_consumable)`: `nutrition` plus what the blood and
/// the stomach will still give; with `only_consumable` other stomach contents take up space too.
pub fn fullness(
    chem: &Chemistry,
    nutrition: f32,
    blood: &Holder,
    stomach: &Holder,
    only_consumable: bool,
) -> f32 {
    let mut fullness = nutrition;
    for r in blood
        .reagents
        .iter()
        .filter(|r| r.metabolization_rate > 0.0)
    {
        fullness += nutriment_factor(chem, r) * r.volume / r.metabolization_rate;
    }
    for r in stomach
        .reagents
        .iter()
        .filter(|r| r.metabolization_rate > 0.0)
    {
        if is_under(&chem.def(r.id).path, CONSUMABLE_ROOT) {
            fullness += nutriment_factor(chem, r) * r.volume / r.metabolization_rate;
        } else if only_consumable {
            // tg: "not food takes up space" (only for the hunger bar's call).
            fullness += 0.6 * r.volume / r.metabolization_rate;
        }
    }
    fullness
}

/// Effects with numbers, by reagent family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Effect {
    Water,
    Nutriment,
    Libital,
    Aiuri,
    Multiver,
    Epinephrine,
    Salbutamol,
    Convermol,
    Toxin,
    Ethanol,
}

fn effect_of(path: &str) -> Option<Effect> {
    Some(match path {
        "/datum/reagent/water" => Effect::Water,
        "/datum/reagent/medicine/c2/libital" => Effect::Libital,
        "/datum/reagent/medicine/c2/aiuri" => Effect::Aiuri,
        "/datum/reagent/medicine/c2/multiver" => Effect::Multiver,
        "/datum/reagent/medicine/epinephrine" => Effect::Epinephrine,
        "/datum/reagent/medicine/salbutamol" => Effect::Salbutamol,
        "/datum/reagent/medicine/c2/convermol" => Effect::Convermol,
        // Subtypes that keep the parent's `on_mob_life` (most call `..()` first).
        p if is_under(p, "/datum/reagent/toxin") => Effect::Toxin,
        p if is_under(p, "/datum/reagent/consumable/ethanol") => Effect::Ethanol,
        p if is_under(p, "/datum/reagent/consumable/nutriment") => Effect::Nutriment,
        _ => return None,
    })
}

/// tg `/obj/item/organ/stomach/on_life`: every reagent that metabolizes drips into the body,
/// `round(efficiency × volume, 0.05) + max(rate, 0.25)` units per second, at most its volume.
pub fn digest(chem: &Chemistry, stomach: &mut Holder, body: &mut Holder, seconds: f32) {
    let ids: Vec<(ReagentId, f32)> = stomach.reagents.iter().map(|r| (r.id, r.volume)).collect();
    for (id, volume) in ids {
        let rate = chem.def(id).metabolization_rate;
        if rate <= 0.0 {
            continue;
        }
        let rate_min = rate.max(STOMACH_METABOLISM_CONSTANT);
        let amount =
            ((round_to(STOMACH_EFFICIENCY * volume, 0.05) + rate_min) * seconds).min(volume);
        if amount <= 0.0 {
            continue;
        }
        stomach.trans_to(chem, body, amount, 1.0, Some(id), false, false);
    }
}

/// `MAX_TOXIN_LIVER_DAMAGE`: liver damage per second from toxins at most.
pub const MAX_TOXIN_LIVER_DAMAGE: f32 = 2.0;

/// Reagent types with `self_consuming = TRUE` (they work without a liver), subtypes included.
const SELF_CONSUMING: [&str; 21] = [
    "/datum/reagent/consumable/tinlux",
    "/datum/reagent/cryostylane",
    "/datum/reagent/determination",
    "/datum/reagent/eldritch",
    "/datum/reagent/fuel/unholywater",
    "/datum/reagent/gravitum",
    "/datum/reagent/inverse/corazargh",
    "/datum/reagent/inverse/healing/syriniver",
    "/datum/reagent/inverse/healing/tirimol",
    "/datum/reagent/inverse/penthrite",
    "/datum/reagent/luminescent_fluid",
    "/datum/reagent/medicine/cordiolis_hepatico",
    "/datum/reagent/medicine/higadrite",
    "/datum/reagent/medicine/neurine",
    "/datum/reagent/napalm",
    "/datum/reagent/phlogiston",
    "/datum/reagent/pyrosium",
    "/datum/reagent/teslium",
    "/datum/reagent/toxin/acid",
    "/datum/reagent/toxin/carnivorousblood",
    "/datum/reagent/water/holywater",
];

/// The liver `metabolize` works with (tg `/obj/item/organ/liver`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Liver {
    /// A working liver: `(maxHealth - damage) / maxHealth`.
    Working { health: f32 },
    /// Missing or failing: the `liverless` metabolism, where only `self_consuming` reagents act.
    Liverless,
}

/// tg `metabolize(owner, seconds_per_tick, can_overdose = TRUE)` of a carbon with a healthy
/// liver; `stomach` is only read (the liver counts a toxin there too). `body_temperature`
/// first warms the holder (`expose_temperature(bodytemperature, 0.25)`).
pub fn metabolize(
    chem: &Chemistry,
    holder: &mut Holder,
    stomach: &Holder,
    body: &mut dyn Body,
    body_temperature: f32,
    seconds: f32,
) {
    metabolize_with(
        chem,
        holder,
        stomach,
        body,
        body_temperature,
        seconds,
        Liver::Working { health: 1.0 },
    );
}

/// [`metabolize`] with `liver`: its tolerance scales with its health, toxins damage it
/// (`(amount / 15) * toxpwr` a second, at most [`MAX_TOXIN_LIVER_DAMAGE`]); without one only
/// `self_consuming` reagents metabolize. Returns the liver damage of this tick.
// shortcut: every toxin has `liver_damage_multiplier` 1 and is painful (`silent_toxin`), and
// none skips a liver type (`affected_organ_flags`); the generated data lacks those vars.
pub fn metabolize_with(
    chem: &Chemistry,
    holder: &mut Holder,
    stomach: &Holder,
    body: &mut dyn Body,
    body_temperature: f32,
    seconds: f32,
    liver: Liver,
) -> f32 {
    holder.expose_temperature(chem, body_temperature, 0.25);
    let dead = body.dead();
    let (liverless, tolerance) = match liver {
        Liver::Working { health } => (false, LIVER_DEFAULT_TOX_TOLERANCE * health),
        Liver::Liverless => (true, 0.0),
    };
    let mut liver_damage = 0.0;
    // tg iterates the live list; reagents removed by an effect are skipped.
    let order: Vec<ReagentId> = holder.reagents.iter().map(|r| r.id).collect();
    for id in order {
        let Some(i) = holder.reagents.iter().position(|r| r.id == id) else {
            continue;
        };
        let def = chem.def(id);
        let is_toxin = is_under(&def.path, "/datum/reagent/toxin");
        let amount = holder.reagents[i].volume + stomach.amount(id);
        if is_toxin && !liverless && !dead {
            // `toxTolerance * liver_health_percent`.
            if amount <= tolerance * def.liver_tolerance_multiplier {
                let rate = holder.reagents[i].metabolization_rate;
                holder.remove_reagent(id, rate * body.metabolism_efficiency() * seconds);
                continue;
            }
        }
        let self_consuming = SELF_CONSUMING.iter().any(|root| is_under(&def.path, root));
        if liverless && !self_consuming {
            continue;
        }
        metabolize_reagent(chem, holder, i, body, seconds, dead);
        if is_toxin && !liverless && !dead {
            // "a 15u syringe is a nice baseline to scale lethality by".
            liver_damage += (amount / 15.0) * def.toxpwr;
        }
    }
    (liver_damage * seconds).min(MAX_TOXIN_LIVER_DAMAGE * seconds)
}

/// tg `metabolize_reagent` (+ `compute_metabolization`, the overdose checks and the reagent's
/// own `metabolize_reagent`).
fn metabolize_reagent(
    chem: &Chemistry,
    holder: &mut Holder,
    i: usize,
    body: &mut dyn Body,
    seconds: f32,
    dead: bool,
) {
    let id = holder.reagents[i].id;
    let def = chem.def(id);
    if dead && def.chemical_flags & reagent_flags::REAGENT_DEAD_PROCESS == 0 {
        return;
    }
    let efficiency = body.metabolism_efficiency();
    let reagent = &mut holder.reagents[i];
    // `compute_metabolization`.
    let mut metabolized = reagent.metabolization_rate * seconds;
    if def.chemical_flags & reagent_flags::REAGENT_UNAFFECTED_BY_METABOLISM == 0 {
        if def.chemical_flags & reagent_flags::REAGENT_REVERSE_METABOLISM != 0 {
            metabolized /= efficiency;
        } else {
            metabolized *= efficiency;
        }
    }
    let metabolized = metabolized.min(reagent.volume);
    reagent.metabolizing = true;
    let ratio = REM * metabolized;
    if reagent.overdose_threshold > 0.0
        && reagent.volume >= reagent.overdose_threshold
        && !reagent.overdosed
    {
        reagent.overdosed = true;
        // `overdose_start`.
        body.tell(format!("Кажется, вы приняли слишком много {}!", def.name));
    }
    let overdosed = reagent.overdosed;
    let effect = effect_of(&def.path);
    if overdosed {
        overdose_process(chem, holder, i, effect, body, seconds, ratio);
    }
    holder.reagents[i].current_cycle += 1;
    on_mob_life(chem, holder, i, effect, body, seconds, ratio);
    // The effect may have removed this or other reagents.
    if holder.has(id).is_none() {
        return;
    }
    if effect == Some(Effect::Epinephrine)
        && let Some(lexorin) = chem.id("/datum/reagent/toxin/lexorin")
        && holder.has(lexorin).is_some()
    {
        // tg: "REM is intentional here".
        holder.remove_reagent(lexorin, REM * metabolized * seconds);
        holder.remove_reagent(id, 0.5 * REM * metabolized * seconds);
    }
    let Some(i) = holder.reagents.iter().position(|r| r.id == id) else {
        return;
    };
    holder.reagents[i].volume -= metabolized;
    holder.update_total();
}

/// tg `normalise_creation_purity()`.
fn normalised_purity(chem: &Chemistry, holder: &Holder, i: usize) -> f32 {
    let r = &holder.reagents[i];
    r.creation_purity / chem.def(r.id).purity
}

/// tg `METABOLIZE_FREE_CONSTANT(value)` of the reagent's current rate.
fn metabolize_free(holder: &Holder, i: usize, value: f32) -> f32 {
    value * REAGENTS_METABOLISM / holder.reagents[i].metabolization_rate
}

#[allow(clippy::too_many_arguments)]
fn on_mob_life(
    chem: &Chemistry,
    holder: &mut Holder,
    i: usize,
    effect: Option<Effect>,
    body: &mut dyn Body,
    seconds: f32,
    ratio: f32,
) {
    let def = chem.def(holder.reagents[i].id);
    // `/datum/reagent/consumable/on_mob_life` (every subtype calls `..()`).
    let factor = nutriment_factor(chem, &holder.reagents[i]);
    if factor > 0.0 {
        body.adjust_nutrition(0.5 * factor * ratio * seconds);
    }
    let Some(effect) = effect else {
        return;
    };
    let purity = normalised_purity(chem, holder, i);
    match effect {
        // shortcut: liver healing and blood restoration need organs and blood.
        Effect::Water => {
            let drunk = body.drunk();
            *drunk = (*drunk + 0.5 * -0.25 * ratio * seconds).max(0.0);
        }
        Effect::Nutriment => {
            if body.spt_prob(30.0, seconds) {
                let heal = 0.5 * def.brute_heal * ratio;
                body.heal_bodypart(heal, heal);
            }
        }
        Effect::Libital => body.adjust(Damage::Brute, -3.0 * purity * ratio * seconds),
        Effect::Aiuri => body.adjust(Damage::Burn, -2.0 * purity * ratio * seconds),
        Effect::Multiver => {
            let mut medibonus = holder
                .reagents
                .iter()
                .filter(|r| chem.is_type(r.id, "/datum/reagent/medicine"))
                .count() as f32;
            if holder.reagents[i].creation_purity >= 1.0 {
                medibonus += 1.0;
            }
            body.adjust(
                Damage::Tox,
                -0.5 * medibonus.min(6.0 * purity) * ratio * seconds,
            );
            let anacea = chem.id("/datum/reagent/toxin/anacea");
            if anacea.is_none_or(|a| holder.has(a).is_none()) {
                let me = holder.reagents[i].id;
                let others: Vec<(ReagentId, f32)> = holder
                    .reagents
                    .iter()
                    .filter(|r| r.id != me)
                    .filter(|r| {
                        !(medibonus >= 3.0 && chem.is_type(r.id, "/datum/reagent/medicine"))
                    })
                    .map(|r| (r.id, chem.def(r.id).purge_multiplier))
                    .collect();
                for (other, multiplier) in others {
                    holder.remove_reagent(other, 3.0 * multiplier * ratio * seconds);
                }
            }
        }
        Effect::Epinephrine => {
            if let Some(lexorin) = chem.id("/datum/reagent/toxin/lexorin")
                && holder.has(lexorin).is_some()
            {
                if body.spt_prob(10.0, seconds)
                    && let Some(histamine) = chem.id("/datum/reagent/toxin/histamine")
                {
                    let temp = holder.chem_temp;
                    holder.add_reagent(chem, histamine, 4.0, temp, None, None, false, false);
                }
                return;
            }
            if body.health() <= CRIT_THRESHOLD {
                let heal = -ratio * seconds;
                for kind in [Damage::Tox, Damage::Brute, Damage::Burn, Damage::Oxy] {
                    body.adjust(kind, heal);
                }
            }
            let lose = body.losebreath();
            *lose = (*lose - 4.0 * ratio * seconds).max(0.0);
            // shortcut: no stamina or stuns yet (`adjust_stamina_loss`, `AdjustAllImmobility`).
        }
        Effect::Salbutamol => {
            body.adjust(Damage::Oxy, -6.0 * ratio * seconds);
            let lose = body.losebreath();
            *lose = (*lose - 4.0 * ratio * seconds).max(0.0);
        }
        Effect::Convermol => {
            // `CONVERMOL_RATIO`.
            const RATIO: f32 = 5.0;
            let r = &holder.reagents[i];
            let mut oxycalc = 2.5 * ratio * (r.current_cycle as f32 - 1.0);
            if !r.overdosed {
                oxycalc = oxycalc.min(body.oxy_loss() + 0.5);
            }
            body.adjust(Damage::Oxy, -2.0 * oxycalc * purity * ratio * seconds);
            body.adjust(Damage::Tox, 2.0 * oxycalc * ratio * seconds / RATIO);
            if body.spt_prob((r.current_cycle as f32 - 1.0) / 2.0, seconds) {
                let lose = body.losebreath();
                if *lose > 0.0 {
                    *lose -= 1.0;
                }
            }
        }
        Effect::Toxin => {
            if def.toxpwr != 0.0 && body.health() > def.health_required {
                let amount =
                    metabolize_free(holder, i, 0.5) * def.toxpwr * purity * ratio * seconds;
                body.adjust(Damage::Tox, amount);
            }
        }
        // shortcut: drunkenness is only its value and toxin damage (`inebriated` stages); no
        // slurring, confusion, dizziness, blur, vomiting or blackouts, and no liver damage.
        Effect::Ethanol => {
            let volume = holder.reagents[i].volume;
            let drunk = *body.drunk();
            if drunk < volume * def.boozepwr * ALCOHOL_THRESHOLD_MODIFIER || def.boozepwr < 0.0 {
                let water = chem
                    .id("/datum/reagent/water")
                    .map_or(0.0, |w| holder.amount(w));
                let alcohol: f32 = holder
                    .reagents
                    .iter()
                    .filter(|r| chem.is_type(r.id, "/datum/reagent/consumable/ethanol"))
                    .map(|r| r.volume)
                    .sum();
                let mut power = def.boozepwr * body.booze_power_multiplier();
                if alcohol + water > 0.0 {
                    power *= alcohol / (alcohol + water);
                }
                let drunk = body.drunk();
                *drunk = (*drunk + volume.sqrt() * power * ALCOHOL_RATE * ratio * seconds).max(0.0);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn overdose_process(
    _chem: &Chemistry,
    holder: &mut Holder,
    i: usize,
    effect: Option<Effect>,
    body: &mut dyn Body,
    seconds: f32,
    ratio: f32,
) {
    match effect {
        Some(Effect::Epinephrine) if body.spt_prob(18.0, seconds) => {
            body.adjust(Damage::Tox, 2.0 * ratio);
            *body.losebreath() += 1.0;
        }
        Some(Effect::Convermol) => {
            holder.reagents[i].metabolization_rate += 2.5 * REAGENTS_METABOLISM;
        }
        _ => {}
    }
}

/// tg `/datum/status_effect/inebriated/tick` (every 2 s while alive): the value decays by
/// 0.01 + 4%; the drunk stage (from [`TIPSY_THRESHOLD`]) poisons from 81.
pub fn inebriation_tick(body: &mut dyn Body) {
    if body.dead() || *body.drunk() <= 0.0 {
        return;
    }
    let drunk = body.drunk();
    *drunk -= 0.01 + *drunk * 0.04;
    let value = *drunk;
    if value <= 0.0 {
        *drunk = 0.0;
        return;
    }
    if value < TIPSY_THRESHOLD {
        return;
    }
    let mut tox = 0.0;
    if value >= 81.0 {
        tox += 1.0;
    }
    if value >= 91.0 {
        tox += 1.0;
    }
    if value >= 101.0 {
        tox += 2.0;
    }
    if tox > 0.0 {
        body.adjust(Damage::Tox, tox);
    }
}

/// Units tg shows for a reagent volume (`round(volume, CHEMICAL_VOLUME_ROUNDING)`).
pub fn shown_volume(volume: f32) -> f32 {
    round_to(volume, CHEMICAL_VOLUME_ROUNDING)
}
