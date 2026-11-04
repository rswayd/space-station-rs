//! tg `/datum/reagents` (`chemistry/holder/holder.dm`, `properties.dm`, `reactions.dm`) and
//! `/datum/equilibrium` (`chemistry/equilibrium.dm`).
//!
//! Reactions: [`Holder::handle_reactions`] starts every reaction whose reagents, catalysts,
//! container, temperature and pH allow it. `REACTION_INSTANT` ones (and those without results)
//! finish at once (`instant_react`); the others become an [`Equilibrium`] that
//! [`Holder::process`] steps (tg `SSreagents`, every 0.25 s) at the rate its temperature and pH
//! give, until the reagents run out or the conditions leave its range.
// shortcut: reagent `data` is only blood's type ([`Reagent::data`]; no DNA, viruses, taste data),
// no catalyst agents
// (`/datum/reagent/catalyst_agent` speed/pH/temperature modifiers), no `pre_reaction_other_checks`
// (those reactions are not converted), no explosions or other effects of reactions' own procs
// (`ReactionDef::special`): overheating and impurity only do tg's default (yield and purity loss).

use crate::{
    CHEMICAL_MAX_PH, CHEMICAL_MAXIMUM_TEMPERATURE, CHEMICAL_MIN_PH, CHEMICAL_NORMAL_PH,
    CHEMICAL_QUANTISATION_LEVEL, CHEMICAL_VOLUME_ROUNDING, Chemistry, NO_OVERHEAT, ReagentId,
    SPECIFIC_HEAT_DEFAULT, flags, is_under, reaction_flags, round_to,
};

/// One reagent in a holder (tg `/datum/reagent` instance).
#[derive(Clone, Debug, PartialEq)]
pub struct Reagent {
    pub id: ReagentId,
    pub volume: f32,
    pub purity: f32,
    pub creation_purity: f32,
    pub ph: f32,
    /// Mob holders only.
    pub metabolizing: bool,
    pub overdosed: bool,
    pub current_cycle: u32,
    /// Per instance: `on_mob_add` scales it by purity, some overdoses change the rate.
    pub overdose_threshold: f32,
    pub metabolization_rate: f32,
    /// tg `data[BLOOD_DATA_TYPE]` of blood: the `/datum/blood_type` path. Merging keeps the
    /// reagent's own (tg `on_merge` leaves the type alone).
    pub data: Option<String>,
}

/// What a holder belongs to (tg `my_atom`), for `required_container` and `mob_react`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Owner {
    /// The atom's type path; empty: no atom.
    pub path: String,
    /// `isliving(my_atom)`: reactions without `mob_react` do not happen, no mix messages, and
    /// added reagents run `on_mob_add`.
    pub living: bool,
}

/// tg `/datum/reagents`.
#[derive(Clone, Debug, PartialEq)]
pub struct Holder {
    pub reagents: Vec<Reagent>,
    pub total_volume: f32,
    pub maximum_volume: f32,
    pub chem_temp: f32,
    pub ph: f32,
    /// [`flags`] bits.
    pub flags: u32,
    pub owner: Owner,
    /// tg `reaction_list`.
    pub reactions: Vec<Equilibrium>,
    pub is_reacting: bool,
    /// Mix messages waiting for the owner's `audible_message` (drained by the game).
    pub messages: Vec<String>,
}

impl Holder {
    /// tg `create_reagents(max_vol, flags)`.
    pub fn new(maximum_volume: f32, flags: u32, owner: Owner) -> Self {
        Self {
            reagents: Vec::new(),
            total_volume: 0.0,
            maximum_volume,
            chem_temp: 150.0,
            ph: CHEMICAL_NORMAL_PH,
            flags,
            owner,
            reactions: Vec::new(),
            is_reacting: false,
            messages: Vec::new(),
        }
    }

    pub fn is_refillable(&self) -> bool {
        self.flags & flags::REFILLABLE != 0
    }
    pub fn is_drainable(&self) -> bool {
        self.flags & flags::DRAINABLE != 0
    }
    pub fn is_open_container(&self) -> bool {
        self.is_refillable() && self.is_drainable()
    }
    /// tg `is_injectable`.
    pub fn is_injectable(&self) -> bool {
        self.flags & (flags::INJECTABLE | flags::REFILLABLE) != 0
    }
    /// tg `is_drawable`.
    pub fn is_drawable(&self) -> bool {
        self.flags & (flags::DRAWABLE | flags::DRAINABLE) != 0
    }
    pub fn holder_full(&self) -> bool {
        self.total_volume >= self.maximum_volume
    }

    fn index(&self, id: ReagentId) -> Option<usize> {
        self.reagents.iter().position(|r| r.id == id)
    }

    /// tg `has_reagent(type)` (exact type).
    pub fn has(&self, id: ReagentId) -> Option<&Reagent> {
        self.reagents.iter().find(|r| r.id == id)
    }

    /// Gives the reagent `id` this `data` unless it has some (tg `add_reagent(data = ...)` of a
    /// new reagent; an existing one keeps its own).
    pub fn set_data_if_none(&mut self, id: ReagentId, data: &str) {
        if let Some(i) = self.index(id)
            && self.reagents[i].data.is_none()
        {
            self.reagents[i].data = Some(data.to_owned());
        }
    }

    /// tg `get_master_reagent`: the largest volume, the first on a tie.
    pub fn master_reagent(&self) -> Option<&Reagent> {
        let mut main: Option<&Reagent> = None;
        for r in &self.reagents {
            if main.is_none_or(|m| r.volume > m.volume) {
                main = Some(r);
            }
        }
        main
    }

    /// tg `get_reagent_amount` (exact type).
    pub fn amount(&self, id: ReagentId) -> f32 {
        self.has(id).map_or(0.0, |r| r.volume)
    }

    /// tg `heat_capacity`.
    pub fn heat_capacity(&self, chem: &Chemistry) -> f32 {
        self.reagents
            .iter()
            .map(|r| chem.def(r.id).specific_heat * r.volume)
            .sum()
    }

    /// tg `set_temperature`.
    pub fn set_temperature(&mut self, temperature: f32) {
        self.chem_temp = temperature.clamp(0.0, CHEMICAL_MAXIMUM_TEMPERATURE);
    }

    /// tg `adjust_thermal_energy`.
    pub fn adjust_thermal_energy(&mut self, chem: &Chemistry, delta: f32, min: f32, max: f32) {
        let capacity = self.heat_capacity(chem);
        if capacity == 0.0 {
            return;
        }
        self.set_temperature((self.chem_temp + delta / capacity).clamp(min, max));
    }

    /// tg `get_average_purity`.
    pub fn average_purity(&self) -> f32 {
        let (total, weighted) = self.reagents.iter().fold((0.0, 0.0), |(t, w), r| {
            (t + r.volume, w + r.volume * r.purity)
        });
        weighted / total
    }

    /// tg `add_reagent`. `purity`/`ph` `None` (or a zero purity, as DM's `!added_purity`)
    /// take the reagent's defaults. Returns the amount added (negative: added as its inverse
    /// reagent).
    #[allow(clippy::too_many_arguments)]
    pub fn add_reagent(
        &mut self,
        chem: &Chemistry,
        id: ReagentId,
        amount: f32,
        reagtemp: f32,
        purity: Option<f32>,
        ph: Option<f32>,
        no_react: bool,
        override_base_ph: bool,
    ) -> f32 {
        if !amount.is_finite() {
            return 0.0;
        }
        let def = chem.def(id);
        let added_purity = purity.filter(|&p| p != 0.0).unwrap_or(def.creation_purity);
        let mut added_ph = ph.filter(|&p| p != 0.0).unwrap_or(def.ph);
        let mut amount = amount;
        if self.flags & flags::REAGENT_HOLDER_ALIVE != 0 {
            amount = self.process_mob_reagent_purity(chem, id, amount, added_purity);
            if amount <= 0.0 {
                return -amount;
            }
        }
        if self.total_volume + amount > self.maximum_volume {
            amount = self.maximum_volume - self.total_volume;
        }
        amount = round_to(amount, CHEMICAL_QUANTISATION_LEVEL);
        if amount <= 0.0 {
            return 0.0;
        }
        let cached_temp = self.chem_temp;
        let old_heat_capacity = if reagtemp != cached_temp {
            self.heat_capacity(chem)
        } else {
            0.0
        };
        let specific_heat = def.specific_heat;
        if let Some(i) = self.index(id) {
            let r = &mut self.reagents[i];
            if override_base_ph {
                added_ph = r.ph;
            }
            r.purity = (r.creation_purity * r.volume + added_purity * amount) / (r.volume + amount);
            r.creation_purity = r.purity;
            r.ph = (r.ph * r.volume + added_ph * amount) / (r.volume + amount);
            r.volume += amount;
            self.update_total();
            if reagtemp != cached_temp {
                self.equalize_temperature(
                    chem,
                    old_heat_capacity,
                    cached_temp,
                    specific_heat,
                    amount,
                    reagtemp,
                );
            }
            if !no_react && !self.is_reacting {
                self.handle_reactions(chem);
            }
            return amount;
        }
        if !self.is_reacting && amount < CHEMICAL_VOLUME_ROUNDING {
            return 0.0;
        }
        let mut overdose_threshold = def.overdose_threshold;
        if self.owner.living {
            // `on_mob_add`.
            overdose_threshold += (added_purity - def.purity) * overdose_threshold;
        }
        self.reagents.push(Reagent {
            id,
            volume: amount,
            purity: added_purity,
            creation_purity: added_purity,
            ph: added_ph,
            metabolizing: false,
            overdosed: false,
            current_cycle: 0,
            overdose_threshold,
            metabolization_rate: def.metabolization_rate,
            data: None,
        });
        self.update_total();
        if reagtemp != cached_temp {
            self.equalize_temperature(
                chem,
                old_heat_capacity,
                cached_temp,
                specific_heat,
                amount,
                reagtemp,
            );
        }
        if !no_react {
            self.handle_reactions(chem);
        }
        amount
    }

    fn equalize_temperature(
        &mut self,
        chem: &Chemistry,
        old_heat_capacity: f32,
        cached_temp: f32,
        specific_heat: f32,
        amount: f32,
        reagtemp: f32,
    ) {
        let new_heat_capacity = self.heat_capacity(chem);
        if new_heat_capacity != 0.0 {
            self.set_temperature(
                (old_heat_capacity * cached_temp + specific_heat * amount * reagtemp)
                    / new_heat_capacity,
            );
        } else {
            self.set_temperature(reagtemp);
        }
    }

    /// tg `process_mob_reagent_purity`: a reagent less pure than its `inverse_chem_val` goes in
    /// as its inverse reagent.
    fn process_mob_reagent_purity(
        &mut self,
        chem: &Chemistry,
        id: ReagentId,
        amount: f32,
        added_purity: f32,
    ) -> f32 {
        if added_purity == 1.0 {
            return amount;
        }
        let def = chem.def(id);
        let Some(inverse) = def.inverse_chem.as_deref().and_then(|p| chem.id(p)) else {
            return amount;
        };
        if def.inverse_chem_val <= added_purity {
            return amount;
        }
        // `get_inverse_purity(creation_purity)` of a fresh reagent.
        let inverse_purity = if def.inverse_chem_val == 0.0 {
            0.0
        } else {
            (1.0 - def.inverse_chem_val + def.creation_purity + 0.01).min(1.0)
        };
        let added = self.add_reagent(
            chem,
            inverse,
            amount,
            crate::DEFAULT_REAGENT_TEMPERATURE,
            Some(inverse_purity),
            None,
            false,
            false,
        );
        -added
    }

    /// tg `add_reagent_list` (map spawns: no reactions before the round runs).
    pub fn add_reagent_list(
        &mut self,
        chem: &Chemistry,
        list: &[(ReagentId, f32)],
        purity: Option<f32>,
        no_react: bool,
    ) {
        for &(id, amount) in list {
            self.add_reagent(
                chem,
                id,
                amount,
                crate::DEFAULT_REAGENT_TEMPERATURE,
                purity,
                None,
                no_react,
                false,
            );
        }
    }

    /// tg `remove_reagent` (exact type). Returns the amount removed.
    pub fn remove_reagent(&mut self, id: ReagentId, amount: f32) -> f32 {
        if !amount.is_finite() {
            return 0.0;
        }
        let amount = round_to(amount, CHEMICAL_QUANTISATION_LEVEL);
        if amount <= 0.0 {
            return 0.0;
        }
        let mut removed = 0.0;
        if let Some(i) = self.index(id) {
            let r = &mut self.reagents[i];
            removed = r.volume.min(amount);
            r.volume -= removed;
        }
        self.update_total();
        removed
    }

    /// tg `remove_all(amount)`: `amount` units spread over the reagents by volume.
    pub fn remove_all(&mut self, amount: f32) -> f32 {
        if self.total_volume == 0.0 || !amount.is_finite() {
            return 0.0;
        }
        let amount = round_to(amount.min(self.total_volume), CHEMICAL_QUANTISATION_LEVEL);
        if amount <= 0.0 {
            return 0.0;
        }
        let part = amount / self.total_volume;
        let mut removed = 0.0;
        for r in &mut self.reagents {
            let take = r.volume * part;
            r.volume -= take;
            removed += take;
        }
        self.update_total();
        round_to(removed, CHEMICAL_QUANTISATION_LEVEL)
    }

    /// tg `clear_reagents`.
    pub fn clear(&mut self) {
        for r in &mut self.reagents {
            r.volume = 0.0;
        }
        self.update_total();
    }

    /// tg `update_total`: volumes rounded to [`CHEMICAL_QUANTISATION_LEVEL`], reagents below
    /// [`CHEMICAL_VOLUME_ROUNDING`] (or at 0 while reacting) removed, the total rounded to
    /// [`CHEMICAL_VOLUME_ROUNDING`] and capped at the maximum, pH the volume-weighted mean.
    pub fn update_total(&mut self) {
        let reacting = self.is_reacting;
        let mut total = 0.0f32;
        let mut total_ph = 0.0f32;
        self.reagents.retain_mut(|r| {
            let volume = round_to(r.volume, CHEMICAL_QUANTISATION_LEVEL);
            if volume <= 0.0 || (!reacting && volume < CHEMICAL_VOLUME_ROUNDING) {
                return false;
            }
            total += volume;
            total_ph += r.ph * volume;
            r.volume = volume;
            true
        });
        self.total_volume = round_to(total, CHEMICAL_VOLUME_ROUNDING).min(self.maximum_volume);
        self.ph = if self.total_volume == 0.0 {
            CHEMICAL_NORMAL_PH
        } else {
            (total_ph / self.total_volume).clamp(CHEMICAL_MIN_PH, CHEMICAL_MAX_PH)
        };
    }

    /// tg `trans_to` into `target`: `amount` units in proportion to every reagent (or only
    /// `target_id`), times `multiplier`, keeping purity, pH and the temperature. Reactions in the
    /// target run once after every reagent moved, unless `no_react`. Returns the amount the
    /// target got.
    // shortcut: `methods` exposure (`expose_mob` on INGEST/INJECT/TOUCH) is not run: tg's
    // default `penetrates_skin` (VAPOR) adds nothing, and the reagent-specific `expose_*` procs
    // are not ported.
    pub fn trans_to(
        &mut self,
        chem: &Chemistry,
        target: &mut Holder,
        amount: f32,
        multiplier: f32,
        target_id: Option<ReagentId>,
        no_react: bool,
        copy_only: bool,
    ) -> f32 {
        if self.total_volume == 0.0 || !amount.is_finite() {
            return 0.0;
        }
        let amount = round_to(
            amount
                .min(self.total_volume)
                .min(target.maximum_volume - target.total_volume),
            CHEMICAL_QUANTISATION_LEVEL,
        );
        if amount <= 0.0 {
            return 0.0;
        }
        let part = if target_id.is_none() {
            amount / self.total_volume
        } else {
            1.0
        };
        let mut total = 0.0;
        for i in 0..self.reagents.len() {
            let r = self.reagents[i].clone();
            let transfer_amount = match target_id {
                Some(id) if r.id == id => amount.min(r.volume),
                Some(_) => continue,
                None => r.volume * part,
            };
            let added = target.add_reagent(
                chem,
                r.id,
                transfer_amount * multiplier,
                self.chem_temp,
                Some(r.purity),
                Some(r.ph),
                true,
                false,
            );
            if added == 0.0 {
                continue;
            }
            if let Some(data) = &r.data {
                target.set_data_if_none(r.id, data);
            }
            total += added;
            if !copy_only {
                self.reagents[i].volume -= transfer_amount;
            }
            if target_id.is_some() {
                break;
            }
        }
        if !copy_only {
            self.update_total();
        }
        if !no_react {
            target.handle_reactions(chem);
        }
        total
    }

    /// tg `expose_temperature(temperature, coeff)`: moves `chem_temp` towards `temperature` by
    /// `coeff` of the difference, at least 1 K, rounded; then reactions.
    pub fn expose_temperature(&mut self, chem: &Chemistry, temperature: f32, coeff: f32) {
        if self.flags & flags::NO_REACT != 0 {
            return;
        }
        let delta = (temperature - self.chem_temp) * coeff;
        if delta > 0.0 {
            self.chem_temp = (self.chem_temp + delta.max(1.0)).min(temperature);
        } else {
            self.chem_temp = (self.chem_temp + delta.min(-1.0)).max(temperature);
        }
        self.set_temperature(self.chem_temp.round());
        self.handle_reactions(chem);
    }

    /// tg `handle_reactions`: starts what can start. Returns how many instant reactions ran.
    pub fn handle_reactions(&mut self, chem: &Chemistry) -> u32 {
        if self.reagents.is_empty() {
            return 0;
        }
        if self.flags & flags::NO_REACT != 0 {
            if self.is_reacting {
                self.force_stop_reacting(chem);
            }
            return 0;
        }
        let cached: Vec<(ReagentId, f32)> =
            self.reagents.iter().map(|r| (r.id, r.volume)).collect();
        let present = |id: ReagentId| {
            cached
                .iter()
                .find(|(c, _)| *c == id)
                .map_or(0.0, |(_, v)| *v)
        };
        let mut reacted = 0;
        for &(reagent, _) in &cached {
            for &index in chem.reactions_of(reagent) {
                let reaction = &chem.reactions[index as usize];
                let def = &reaction.def;
                if self.reactions.iter().any(|e| e.reaction == index) {
                    continue;
                }
                let granularity = if def.reaction_flags & reaction_flags::REACTION_INSTANT != 0 {
                    1.0
                } else {
                    CHEMICAL_QUANTISATION_LEVEL
                };
                // `required_reagents | required_catalysts`: a catalyst also listed as a
                // reagent keeps the reagent's amount.
                let requirements = reaction.required.iter().chain(
                    reaction
                        .catalysts
                        .iter()
                        .filter(|(c, _)| !reaction.required.iter().any(|(r, _)| r == c)),
                );
                let mut missing = false;
                for &(id, needed) in requirements {
                    let have = present(id);
                    if have == 0.0 || have < needed * granularity {
                        missing = true;
                        break;
                    }
                }
                if missing {
                    continue;
                }
                if !self.owner.path.is_empty() {
                    if let Some(container) = &def.required_container {
                        let fits = if def.required_container_accepts_subtypes {
                            is_under(&self.owner.path, container)
                        } else {
                            self.owner.path == *container
                        };
                        if !fits {
                            continue;
                        }
                    }
                    if self.owner.living && !def.mob_react {
                        continue;
                    }
                } else if def.required_container.is_some() {
                    continue;
                }
                // tg's precedence: `(required_temp && cold && too hot) || (!cold && too cold)`.
                if (def.required_temp != 0.0
                    && def.is_cold_recipe
                    && self.chem_temp > def.required_temp)
                    || (!def.is_cold_recipe && self.chem_temp < def.required_temp)
                {
                    continue;
                }
                // tg's pH check here is `< min - range && > max + range`, never true; the
                // equilibrium checks the range when it starts.
                if def.reaction_flags & reaction_flags::REACTION_INSTANT != 0
                    || self.flags & flags::REAGENT_HOLDER_INSTANT_REACT != 0
                    || reaction.results.is_empty()
                {
                    self.instant_react(chem, index);
                    reacted += 1;
                } else if let Some(eq) = Equilibrium::new(chem, index, self) {
                    self.reactions.push(eq);
                    self.is_reacting = true;
                    let last = self.reactions.len() - 1;
                    self.step_equilibrium(chem, last, 1.0);
                }
            }
        }
        self.is_reacting = !self.reactions.is_empty();
        reacted
    }

    /// tg `instant_react`.
    fn instant_react(&mut self, chem: &Chemistry, index: u32) {
        let reaction = &chem.reactions[index as usize];
        let mut multiplier = f32::INFINITY;
        for &(id, needed) in &reaction.required {
            multiplier = multiplier.min(self.amount(id) / needed);
        }
        let multiplier = round_to(multiplier, CHEMICAL_QUANTISATION_LEVEL);
        if multiplier == 0.0 || !multiplier.is_finite() {
            return;
        }
        let average_purity = self.average_purity();
        for &(id, needed) in &reaction.required {
            self.remove_reagent(id, needed * multiplier);
        }
        for &(id, made) in &reaction.results {
            let temp = self.chem_temp;
            self.add_reagent(
                chem,
                id,
                made * multiplier * average_purity,
                temp,
                Some(average_purity),
                None,
                false,
                false,
            );
        }
        if !self.owner.path.is_empty() && !self.owner.living {
            self.messages.push(reaction.def.mix_message.clone());
        }
    }

    /// Steps `self.reactions[i]` by `seconds`; the list may grow meanwhile (the products start
    /// new reactions), entries are only removed by [`Self::process`].
    fn step_equilibrium(&mut self, chem: &Chemistry, i: usize, seconds: f32) {
        let mut eq = self.reactions[i].clone();
        eq.react_timestep(chem, self, seconds);
        self.reactions[i] = eq;
    }

    /// tg `/datum/reagents/process` (`SSreagents`, every 0.25 s while reacting): steps every
    /// running reaction, ends the finished ones with their mix messages.
    pub fn process(&mut self, chem: &Chemistry, seconds: f32) {
        if !self.is_reacting {
            self.force_stop_reacting(chem);
            return;
        }
        let mut mix: Vec<String> = Vec::new();
        let count = self.reactions.len();
        let mut i = 0;
        let mut seen = 0;
        while seen < count && i < self.reactions.len() {
            self.step_equilibrium(chem, i, seconds);
            seen += 1;
            if self.reactions[i].to_delete {
                let message = self.end_reaction(chem, i);
                if !mix.contains(&message) {
                    mix.push(message);
                }
                continue;
            }
            i += 1;
        }
        if !mix.is_empty() {
            self.messages.push(mix.concat());
        }
        if self.reactions.is_empty() {
            self.is_reacting = false;
            self.update_total();
        } else {
            self.handle_reactions(chem);
        }
    }

    /// tg `end_reaction`: `reaction_finish`, then the mix message.
    fn end_reaction(&mut self, chem: &Chemistry, i: usize) -> String {
        let eq = self.reactions.remove(i);
        let reaction = &chem.reactions[eq.reaction as usize];
        if reaction.def.reaction_flags & reaction_flags::REACTION_CLEAR_INVERSE != 0 {
            let mut inverse = Vec::new();
            for r in &mut self.reagents {
                let def = chem.def(r.id);
                let Some(inv) = def.inverse_chem.as_deref().and_then(|p| chem.id(p)) else {
                    continue;
                };
                if !reaction.results.iter().any(|(id, _)| *id == r.id)
                    || r.purity == 1.0
                    || r.purity > def.inverse_chem_val
                {
                    continue;
                }
                let purity = if def.inverse_chem_val == 0.0 {
                    0.0
                } else {
                    (1.0 - def.inverse_chem_val + r.purity + 0.01).min(1.0)
                };
                inverse.push((inv, r.volume, purity));
                r.volume = 0.0;
            }
            self.update_total();
            let temp = self.chem_temp;
            for (id, volume, purity) in inverse {
                self.add_reagent(chem, id, volume, temp, Some(purity), None, false, false);
            }
        }
        self.update_total();
        reaction.def.mix_message.clone()
    }

    /// tg `force_stop_reacting`.
    pub fn force_stop_reacting(&mut self, chem: &Chemistry) {
        let mut mix = Vec::new();
        while !self.reactions.is_empty() {
            mix.push(self.end_reaction(chem, 0));
        }
        if !mix.is_empty() {
            self.messages.push(mix.concat());
        }
        self.is_reacting = false;
        self.update_total();
    }

    /// tg `adjust_specific_reagent_ph`.
    fn adjust_reagent_ph(&mut self, id: ReagentId, value: f32) {
        if let Some(i) = self.index(id) {
            let r = &mut self.reagents[i];
            r.ph = (r.ph + value).clamp(CHEMICAL_MIN_PH, CHEMICAL_MAX_PH);
        }
    }
}

/// tg `/datum/equilibrium`: one step reaction in progress.
#[derive(Clone, Debug, PartialEq)]
pub struct Equilibrium {
    /// Index into [`Chemistry::reactions`].
    pub reaction: u32,
    multiplier: f32,
    product_ratio: f32,
    target_vol: f32,
    step_target_vol: f32,
    reacted_vol: f32,
    pub to_delete: bool,
    min_rate: f32,
    time_deficit: f32,
    /// tg `reaction_quality`: the last step's purity (UI only).
    pub reaction_quality: f32,
}

impl Equilibrium {
    /// tg `reacted_vol`: product made so far (UI).
    pub fn reacted_vol(&self) -> f32 {
        self.reacted_vol
    }

    /// tg `target_vol`: product expected at the end (UI).
    pub fn target_vol(&self) -> f32 {
        self.target_vol
    }

    /// tg `New` -> `check_inital_conditions`; `None` when it cannot start.
    fn new(chem: &Chemistry, reaction: u32, holder: &Holder) -> Option<Self> {
        let r = &chem.reactions[reaction as usize];
        let mut multiplier = f32::INFINITY;
        for &(id, needed) in &r.required {
            multiplier = multiplier.min(holder.amount(id) / needed);
        }
        let multiplier = round_to(multiplier, CHEMICAL_QUANTISATION_LEVEL);
        if multiplier == 0.0 || !multiplier.is_finite() {
            return None;
        }
        let def = &r.def;
        if holder.ph < def.optimal_ph_min - def.determin_ph_range
            || holder.ph > def.optimal_ph_max + def.determin_ph_range
        {
            return None;
        }
        let product_ratio = if r.results.is_empty() {
            1.0
        } else {
            r.results.iter().map(|(_, n)| n).sum()
        };
        Some(Self {
            reaction,
            multiplier,
            product_ratio,
            target_vol: 0.0,
            step_target_vol: f32::INFINITY,
            reacted_vol: 0.0,
            to_delete: false,
            min_rate: product_ratio * (CHEMICAL_VOLUME_ROUNDING / 2.0),
            time_deficit: 0.0,
            reaction_quality: 1.0,
        })
    }

    /// tg `calculate_yield`.
    fn calculate_yield(&mut self, chem: &Chemistry, holder: &Holder) -> bool {
        let r = &chem.reactions[self.reaction as usize];
        let mut multiplier = f32::INFINITY;
        for &(id, needed) in &r.required {
            multiplier = multiplier.min(holder.amount(id) / needed);
        }
        self.multiplier = round_to(multiplier, CHEMICAL_QUANTISATION_LEVEL);
        if self.multiplier == 0.0 || !self.multiplier.is_finite() {
            return false;
        }
        if r.results.is_empty() {
            self.step_target_vol = r
                .required
                .iter()
                .map(|(_, n)| self.multiplier * n)
                .fold(f32::INFINITY, f32::min);
            return true;
        }
        self.step_target_vol = 0.0;
        self.reacted_vol = 0.0;
        for &(id, made) in &r.results {
            self.step_target_vol += self.multiplier * made;
            self.reacted_vol += holder.amount(id);
        }
        self.target_vol = self.reacted_vol + self.step_target_vol;
        true
    }

    /// tg `deal_with_time`: a step covers at most 1 s; the rest is paid back 0.25 s per step.
    fn deal_with_time(&mut self, seconds: f32) -> f32 {
        if seconds > 1.0 {
            self.time_deficit += seconds - 1.0;
            1.0
        } else if self.time_deficit > 0.0 {
            if self.time_deficit < 0.25 {
                let s = seconds + self.time_deficit;
                self.time_deficit = 0.0;
                s
            } else {
                self.time_deficit -= 0.25;
                seconds + 0.25
            }
        } else {
            seconds
        }
    }

    /// tg `react_timestep`.
    fn react_timestep(&mut self, chem: &Chemistry, holder: &mut Holder, seconds: f32) {
        if self.to_delete {
            return;
        }
        let r = &chem.reactions[self.reaction as usize];
        let def = &r.def;
        // `check_reagent_properties`: still reagents, and every catalyst present.
        let catalysts_present = r
            .catalysts
            .iter()
            .filter(|(c, _)| holder.has(*c).is_some())
            .count();
        if holder.reagents.is_empty() || catalysts_present != r.catalysts.len() {
            self.to_delete = true;
            return;
        }
        if !self.calculate_yield(chem, holder) {
            self.to_delete = true;
            return;
        }
        let seconds = self.deal_with_time(seconds);
        let (ph, temp) = (holder.ph, holder.chem_temp);
        let delta_ph = if ph >= def.optimal_ph_min && ph <= def.optimal_ph_max {
            1.0
        } else if ph < def.optimal_ph_min {
            let acceptable = def.optimal_ph_min - def.determin_ph_range;
            if ph < acceptable {
                0.0
            } else {
                ((ph - acceptable) / def.determin_ph_range).powf(def.ph_exponent_factor)
            }
        } else {
            let acceptable = def.optimal_ph_max + def.determin_ph_range;
            if ph > acceptable {
                0.0
            } else {
                ((acceptable - ph) / def.determin_ph_range).powf(def.ph_exponent_factor)
            }
        };
        let delta_t = if !def.is_cold_recipe {
            if temp < def.optimal_temp && temp >= def.required_temp {
                ((temp - def.required_temp) / (def.optimal_temp - def.required_temp))
                    .powf(def.temp_exponent_factor)
            } else if temp >= def.optimal_temp {
                1.0
            } else {
                self.to_delete = true;
                return;
            }
        } else if temp > def.optimal_temp && temp <= def.required_temp {
            ((def.required_temp - temp) / (def.required_temp - def.optimal_temp))
                .powf(def.temp_exponent_factor)
        } else if temp <= def.optimal_temp {
            1.0
        } else {
            self.to_delete = true;
            return;
        };
        let purity = delta_ph * holder.average_purity();
        let mut factor = def.rate_up_lim * delta_t * seconds;
        if factor > self.step_target_vol {
            factor = self.step_target_vol;
        } else if factor < self.min_rate {
            factor = self.min_rate;
        }
        let factor = round_to(factor / self.product_ratio, CHEMICAL_VOLUME_ROUNDING);
        if factor <= 0.0 {
            self.to_delete = true;
            return;
        }
        let constant_ph = def.reaction_flags & reaction_flags::REACTION_PH_VOL_CONSTANT != 0;
        for &(id, needed) in &r.required {
            if holder.remove_reagent(id, factor * needed) == 0.0 {
                self.to_delete = true;
                return;
            }
            let adjust = if constant_ph {
                (factor * needed / self.target_vol) * def.h_ion_release
            } else {
                factor * needed * def.h_ion_release
            };
            holder.adjust_reagent_ph(id, adjust);
        }
        let mut total_step_added = 0.0;
        for &(id, made) in &r.results {
            let added = holder.add_reagent(
                chem,
                id,
                factor * made,
                temp,
                Some(purity),
                None,
                false,
                true,
            );
            if added == 0.0 {
                self.to_delete = true;
                return;
            }
            let adjust = if constant_ph {
                (added / self.target_vol) * def.h_ion_release
            } else {
                added * def.h_ion_release
            };
            holder.adjust_reagent_ph(id, adjust);
            self.reacted_vol += added;
            total_step_added += added;
        }
        self.reaction_quality = purity;
        let heat = def.thermic_constant * total_step_added;
        if def.reaction_flags & reaction_flags::REACTION_HEAT_ARBITARY != 0 {
            holder.set_temperature(
                (holder.chem_temp + heat).clamp(0.0, CHEMICAL_MAXIMUM_TEMPERATURE),
            );
        } else {
            holder.adjust_thermal_energy(
                chem,
                heat * SPECIFIC_HEAT_DEFAULT,
                0.0,
                CHEMICAL_MAXIMUM_TEMPERATURE,
            );
        }
        // shortcut: tg's 5% chance per step of repeating the mix message is left out.
        // `check_fail_states` with the default `overheated`/`overly_impure`.
        let overheated = if def.is_cold_recipe {
            holder.chem_temp < def.overheat_temp && def.overheat_temp != NO_OVERHEAT
        } else {
            holder.chem_temp > def.overheat_temp
        };
        if overheated {
            for reagent in &mut holder.reagents {
                if r.results.iter().any(|(id, _)| *id == reagent.id) {
                    reagent.volume *= 0.98;
                }
            }
            holder.update_total();
        }
        for &(product, _) in &r.results {
            if holder
                .has(product)
                .is_some_and(|p| p.purity < def.purity_min)
            {
                for &(id, _) in r.results.iter().chain(&r.required) {
                    if let Some(i) = holder.index(id) {
                        let p = &mut holder.reagents[i];
                        p.purity = (p.purity - 0.01).clamp(0.0, 1.0);
                    }
                }
            }
        }
        if holder.reagents.is_empty() {
            self.to_delete = true;
            return;
        }
        if total_step_added >= self.step_target_vol && holder.reactions.len() == 1 {
            self.to_delete = true;
        }
    }
}
