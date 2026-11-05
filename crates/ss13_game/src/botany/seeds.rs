//! Seeds (tg `/obj/item/seeds`, `seeds.dm`): the live stats and genes of a seed packet, of the
//! plant in a tray and of a grown item's seed, with tg's setters and their bounds, `mutate`,
//! `Copy`, the reagent amounts of `prepare_result`, `get_reagent_purity` and `getYield`.

use bevy::prelude::*;
use ss13_mapload::plants::SeedDef;
use ss13_reagents::is_under;

use super::Plants;
use super::genes::{
    FUNGAL, Gene, PLANT_GENE_MUTATABLE, SOIL_LOVER, TRAIT_HALVES_YIELD, TRAIT_NO_POLLINATION,
    reagent_can_add, trait_can_add, try_upgrade_gene,
};
use crate::combat::CombatRng;

/// `MAX_PLANT_YIELD`, `MAX_PLANT_LIFESPAN`, `MAX_PLANT_ENDURANCE`, `MAX_PLANT_PRODUCTION`,
/// `MAX_PLANT_POTENCY`, `MAX_PLANT_INSTABILITY`, `MAX_PLANT_WEEDRATE`, `MAX_PLANT_WEEDCHANCE`,
/// `MIN_PLANT_ENDURANCE`, `FUNGAL_METAB_YIELD_MIN`.
pub const MAX_PLANT_YIELD: f32 = 10.0;
pub const MAX_PLANT_LIFESPAN: f32 = 100.0;
pub const MAX_PLANT_ENDURANCE: f32 = 100.0;
pub const MAX_PLANT_PRODUCTION: f32 = 10.0;
pub const MAX_PLANT_POTENCY: f32 = 100.0;
pub const MAX_PLANT_INSTABILITY: f32 = 100.0;
pub const MAX_PLANT_WEEDRATE: f32 = 10.0;
pub const MAX_PLANT_WEEDCHANCE: f32 = 67.0;
pub const MIN_PLANT_ENDURANCE: f32 = 10.0;
pub const FUNGAL_METAB_YIELD_MIN: f32 = 1.0;
/// `SOIL_LOVER_HYDRO_YIELD_MALUS`, `SOIL_LOVER_HYDRO_POTENCY_MIN`/`MAX`.
pub const SOIL_LOVER_HYDRO_YIELD_MALUS: f32 = 0.7;
pub const SOIL_LOVER_HYDRO_POTENCY_MIN: f32 = 0.5;
pub const SOIL_LOVER_HYDRO_POTENCY_MAX: f32 = 0.8;
/// `MUTATE_EARLY`, `NO_PLANTING` (`seed_flags`).
pub const MUTATE_EARLY: u32 = 1 << 0;
pub const NO_PLANTING: u32 = 1 << 1;
/// `/obj/item/seeds/random`, the strange seed.
pub const STRANGE_SEED: &str = "/obj/item/seeds/random";
const PLANT_TYPE: &str = "/datum/plant_gene/trait/plant_type";
/// Tray `SOIL` flag.
pub const SOIL: u32 = 1 << 1;
/// `BITE_SIZE_POTENCY_MULTIPLIER`, `BITE_SIZE_VOLUME_MULTIPLIER`.
const BITE_SIZE_POTENCY_MULTIPLIER: f32 = 0.05;
const BITE_SIZE_VOLUME_MULTIPLIER: f32 = 0.01;

/// DM `round(x)` with one argument: a floor.
pub fn dm_round(x: f32) -> f32 {
    x.floor()
}

/// DM `rand`/`prob`/`pick`.
pub trait Dice {
    /// `rand(low, high)` on integers, both ends included (BYOND swaps reversed bounds).
    fn rand(&mut self, low: i32, high: i32) -> i32;
    /// `prob(percent)`.
    fn prob(&mut self, percent: f32) -> bool;
    /// `pick(list)` of a list of `len` (> 0): the index.
    fn pick(&mut self, len: usize) -> usize {
        self.rand(0, len as i32 - 1) as usize
    }
    /// `RANDOM_DECIMAL(low, high)`.
    fn decimal(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.rand(0, 10_000) as f32 / 10_000.0
    }
}

impl Dice for CombatRng {
    fn rand(&mut self, low: i32, high: i32) -> i32 {
        let (low, high) = if low <= high {
            (low, high)
        } else {
            (high, low)
        };
        low + self.range(0, (high - low) as u32) as i32
    }

    fn prob(&mut self, percent: f32) -> bool {
        self.chance(percent)
    }
}

/// A seed's live data. On seed packets (the packet itself), grown items (their `seed`) and in
/// trays ([`super::tray::Tray::myseed`]).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Seed {
    /// The seed type: product, icons and flags come from its [`SeedDef`].
    pub path: String,
    pub name: String,
    pub plantname: String,
    pub lifespan: f32,
    pub endurance: f32,
    pub maturation: f32,
    pub production: f32,
    /// `yield`; `-1`: never harvested.
    pub yield_: f32,
    pub potency: f32,
    pub instability: f32,
    pub weed_rate: f32,
    pub weed_chance: f32,
    pub genes: Vec<Gene>,
    pub mutatelist: Vec<String>,
    /// `grafts_taken`: cuttings taken off this plant (not copied).
    pub grafts_taken: u32,
}

/// The `mutate()` ranges (`rand(-x, x)` each; `traitmut` is a chance).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mutation {
    pub lifemut: i32,
    pub endmut: i32,
    pub productmut: i32,
    pub yieldmut: i32,
    pub potmut: i32,
    pub wrmut: i32,
    pub wcmut: i32,
    pub traitmut: f32,
    pub stabmut: i32,
}

impl Mutation {
    /// `/obj/machinery/hydroponics/proc/mutate` defaults.
    pub const MUTATE: Self = Self {
        lifemut: 2,
        endmut: 5,
        productmut: 1,
        yieldmut: 2,
        potmut: 25,
        wrmut: 2,
        wcmut: 5,
        traitmut: 0.0,
        stabmut: 3,
    };
    /// `hardmutate` defaults.
    pub const HARD: Self = Self {
        lifemut: 4,
        endmut: 10,
        productmut: 2,
        yieldmut: 4,
        potmut: 50,
        wrmut: 4,
        wcmut: 10,
        traitmut: 0.0,
        stabmut: 4,
    };
    /// `traitmutate(traitmut)`: only the trait/reagent roll.
    pub fn traits(traitmut: f32) -> Self {
        Self {
            lifemut: 0,
            endmut: 0,
            productmut: 0,
            yieldmut: 0,
            potmut: 0,
            wrmut: 0,
            wcmut: 0,
            traitmut,
            stabmut: 0,
        }
    }
}

impl Seed {
    /// `new seed_type()`: its vars, its trait genes and a reagent gene per `reagents_add`.
    pub fn new(def: &SeedDef, plants: &Plants) -> Self {
        let mut genes: Vec<Gene> = def
            .genes
            .iter()
            .filter_map(|p| plants.trait_def(p).map(Gene::new_trait))
            .collect();
        genes.extend(def.reagents.iter().map(|(p, r)| Gene::new_reagent(p, *r)));
        Self {
            path: def.path.clone(),
            name: def.name.clone(),
            plantname: def.plantname.clone(),
            lifespan: def.lifespan,
            endurance: def.endurance,
            maturation: def.maturation,
            production: def.production,
            yield_: def.yield_,
            potency: def.potency,
            instability: def.instability,
            weed_rate: def.weed_rate,
            weed_chance: def.weed_chance,
            genes,
            mutatelist: def.mutatelist.clone(),
            grafts_taken: 0,
        }
    }

    /// `Copy()`: a new seed of the type (its own `mutatelist`) with these stats and genes.
    pub fn copy(&self, plants: &Plants) -> Self {
        Self {
            mutatelist: plants
                .seed_def(&self.path)
                .map_or_else(|| self.mutatelist.clone(), |d| d.mutatelist.clone()),
            grafts_taken: 0,
            ..self.clone()
        }
    }

    /// `get_gene(typepath)`: a trait gene of that type or a subtype.
    pub fn has_gene(&self, root: &str) -> bool {
        self.genes
            .iter()
            .any(|g| matches!(g, Gene::Trait { path, .. } if is_under(path, root)))
    }

    fn any_trait_flag(&self, flag: u32) -> bool {
        self.genes
            .iter()
            .any(|g| matches!(g, Gene::Trait { flags, .. } if flags & flag != 0))
    }

    /// `reagents_add` (`reagents_from_genes`): every reagent gene in order.
    pub fn reagents_add(&self) -> Vec<(String, f32)> {
        self.genes
            .iter()
            .filter_map(|g| match g {
                Gene::Reagent { path, rate, .. } => Some((path.clone(), *rate)),
                Gene::Trait { .. } => None,
            })
            .collect()
    }

    fn yield_bounds(&self) -> (f32, f32) {
        let max = if self.any_trait_flag(TRAIT_HALVES_YIELD) {
            dm_round(MAX_PLANT_YIELD / 2.0)
        } else {
            MAX_PLANT_YIELD
        };
        let min = if self.has_gene(FUNGAL) {
            FUNGAL_METAB_YIELD_MIN
        } else {
            0.0
        };
        (min, max)
    }

    pub fn set_yield(&mut self, value: f32) {
        if self.yield_ == -1.0 {
            return; // unharvestable stays unharvestable
        }
        let (min, max) = self.yield_bounds();
        self.yield_ = value.clamp(min, max);
    }

    pub fn adjust_yield(&mut self, amount: f32) {
        self.set_yield(self.yield_ + amount);
    }

    pub fn set_lifespan(&mut self, value: f32) {
        self.lifespan = value.clamp(10.0, MAX_PLANT_LIFESPAN);
    }

    pub fn adjust_lifespan(&mut self, amount: f32) {
        self.set_lifespan(self.lifespan + amount);
    }

    pub fn set_endurance(&mut self, value: f32) {
        self.endurance = value.clamp(MIN_PLANT_ENDURANCE, MAX_PLANT_ENDURANCE);
    }

    pub fn adjust_endurance(&mut self, amount: f32) {
        self.set_endurance(self.endurance + amount);
    }

    pub fn set_production(&mut self, value: f32) {
        if self.yield_ == -1.0 {
            return;
        }
        self.production = value.clamp(1.0, MAX_PLANT_PRODUCTION);
    }

    pub fn adjust_production(&mut self, amount: f32) {
        self.set_production(self.production + amount);
    }

    pub fn set_potency(&mut self, value: f32) {
        if self.potency == -1.0 {
            return;
        }
        self.potency = value.clamp(0.0, MAX_PLANT_POTENCY);
    }

    pub fn adjust_potency(&mut self, amount: f32) {
        self.set_potency(self.potency + amount);
    }

    pub fn set_instability(&mut self, value: f32) {
        if self.instability == -1.0 {
            return;
        }
        self.instability = value.clamp(0.0, MAX_PLANT_INSTABILITY);
    }

    pub fn adjust_instability(&mut self, amount: f32) {
        self.set_instability(self.instability + amount);
    }

    pub fn set_weed_rate(&mut self, value: f32) {
        self.weed_rate = value.clamp(0.0, MAX_PLANT_WEEDRATE);
    }

    pub fn adjust_weed_rate(&mut self, amount: f32) {
        self.set_weed_rate(self.weed_rate + amount);
    }

    pub fn set_weed_chance(&mut self, value: f32) {
        self.weed_chance = value.clamp(0.0, MAX_PLANT_WEEDCHANCE);
    }

    pub fn adjust_weed_chance(&mut self, amount: f32) {
        self.set_weed_chance(self.weed_chance + amount);
    }

    /// `mutate()`: every stat by `rand(-x, x)`, then the trait/reagent roll.
    pub fn mutate(&mut self, m: Mutation, plants: &Plants, dice: &mut impl Dice) {
        {
            let mut roll = |x: i32| dice.rand(-x, x) as f32;
            self.adjust_lifespan(roll(m.lifemut));
            self.adjust_endurance(roll(m.endmut));
            self.adjust_production(roll(m.productmut));
            self.adjust_yield(roll(m.yieldmut));
            self.adjust_potency(roll(m.potmut));
            self.adjust_instability(roll(m.stabmut));
            self.adjust_weed_rate(roll(m.wrmut));
            self.adjust_weed_chance(roll(m.wcmut));
        }
        if dice.prob(m.traitmut) {
            if dice.prob(50.0) {
                self.add_random_traits(1, 1, plants, dice);
            } else {
                self.add_random_reagents(1, 1, plants, dice);
            }
        }
    }

    /// `add_random_traits(lower, upper)`: `rand(lower, upper)` picks of any trait type, kept when
    /// it is mutatable and the seed can take it.
    pub fn add_random_traits(
        &mut self,
        lower: i32,
        upper: i32,
        plants: &Plants,
        dice: &mut impl Dice,
    ) {
        if plants.data.traits.is_empty() {
            return;
        }
        for _ in 0..dice.rand(lower, upper) {
            let def = &plants.data.traits[dice.pick(plants.data.traits.len())];
            if def.mutability_flags & PLANT_GENE_MUTATABLE != 0 && trait_can_add(def, self) {
                self.genes.push(Gene::new_trait(def));
            }
        }
    }

    /// `add_random_reagents(lower, upper)`: a random reagent at 4-15%, added or upgrading the
    /// seed's own gene of it.
    pub fn add_random_reagents(
        &mut self,
        lower: i32,
        upper: i32,
        plants: &Plants,
        dice: &mut impl Dice,
    ) {
        let pool = &plants.data.random_reagents;
        if pool.is_empty() {
            return;
        }
        for _ in 0..dice.rand(lower, upper) {
            let rate = dice.rand(4, 15) as f32 * 0.01;
            let mut gene = Gene::new_reagent(&pool[dice.pick(pool.len())], rate);
            if reagent_can_add(&gene, self) && !try_upgrade_gene(&mut gene, self) {
                self.genes.push(gene);
            }
        }
    }

    /// `/obj/item/seeds/random/Initialize`: `randomize_stats()`, then 60% one to three random
    /// reagents, 50% one or two random traits, `add_random_plant_type(35)`.
    pub fn randomize_strange(&mut self, plants: &Plants, dice: &mut impl Dice) {
        self.set_lifespan(dice.rand(25, 60) as f32);
        self.set_endurance(dice.rand(15, 35) as f32);
        self.set_production(dice.rand(2, 10) as f32);
        self.set_yield(dice.rand(1, 10) as f32);
        self.set_potency(dice.rand(10, 35) as f32);
        self.set_weed_rate(dice.rand(1, 10) as f32);
        self.set_weed_chance(dice.rand(5, 100) as f32);
        self.maturation = dice.rand(6, 12) as f32;
        if dice.prob(60.0) {
            self.add_random_reagents(1, 3, plants, dice);
        }
        if dice.prob(50.0) {
            self.add_random_traits(1, 2, plants, dice);
        }
        if dice.prob(35.0) {
            let types: Vec<&ss13_mapload::plants::TraitDef> = plants
                .data
                .traits
                .iter()
                .filter(|t| is_under(&t.path, PLANT_TYPE) && t.path != PLANT_TYPE)
                .collect();
            if !types.is_empty() {
                let def = types[dice.pick(types.len())];
                if trait_can_add(def, self) {
                    self.genes.push(Gene::new_trait(def));
                }
            }
        }
    }

    /// `get_reagent_purity()`: 50% plus lifespan and endurance, +-instability at random.
    pub fn reagent_purity(&self, dice: &mut impl Dice) -> f32 {
        let instability = self.instability as i32;
        let roll = dice.rand(-instability, instability) as f32;
        (0.5 + self.lifespan / 400.0 + self.endurance / 400.0 + roll / 400.0).clamp(0.0, 1.0)
    }

    /// `prepare_result()`: per reagent gene `max(1, round(volume * potency% * rate *
    /// min(1, 1 / sum of rates)))`.
    pub fn prepare_amounts(&self, volume: f32) -> Vec<(String, f32)> {
        let adds = self.reagents_add();
        let sum: f32 = adds.iter().map(|(_, r)| r).sum();
        adds.into_iter()
            .map(|(path, rate)| {
                let amount =
                    dm_round(volume * (self.potency / 100.0) * rate * (1.0 / sum).min(1.0));
                (path, amount.max(1.0))
            })
            .collect()
    }

    /// `/obj/item/food/grown/Initialize`: `bite_consumption = 1 + round(max(potency * 0.05, 1)
    /// * (max_volume * 0.01) * bite_consumption_mod)`.
    pub fn bite_size(&self, volume: f32, bite_mod: f32) -> f32 {
        1.0 + dm_round(
            (self.potency * BITE_SIZE_POTENCY_MULTIPLIER).max(1.0)
                * (volume * BITE_SIZE_VOLUME_MULTIPLIER)
                * bite_mod,
        )
    }

    /// `getYield()` in a tray with `tray_flags` and `yieldmod`.
    pub fn harvest_yield(&self, tray_flags: u32, yieldmod: f32) -> f32 {
        let mut y = self.yield_;
        if tray_flags & SOIL == 0 && self.has_gene(SOIL_LOVER) {
            y *= SOIL_LOVER_HYDRO_YIELD_MALUS;
        }
        if self.any_trait_flag(TRAIT_NO_POLLINATION) {
            return y; // tg returns before rounding here
        }
        if yieldmod == 0.0 {
            y = y.min(1.0);
        } else {
            y *= yieldmod;
        }
        dm_round(y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::botany::{Script, test_plants};

    fn tomato() -> Seed {
        let plants = test_plants();
        Seed::new(plants.seed_def("/obj/item/seeds/tomato").unwrap(), &plants)
    }

    #[test]
    fn strange_seeds_roll_their_stats() {
        let plants = test_plants();
        let mut s = tomato();
        let mut dice = Script {
            rolls: vec![40, 20, 5, 7, 30, 3, 50, 9, 0, 1, 0, 0, 0, 0],
            probs: vec![false, false, true],
        };
        s.randomize_strange(&plants, &mut dice);
        assert_eq!(
            (s.lifespan, s.endurance, s.production, s.yield_, s.potency),
            (40.0, 20.0, 5.0, 7.0, 30.0)
        );
        assert_eq!((s.weed_rate, s.weed_chance, s.maturation), (3.0, 50.0, 9.0));
        // The plant type roll: the first plant type of the test traits (fungal).
        assert!(s.has_gene(FUNGAL));
    }

    #[test]
    fn new_seed_has_trait_and_reagent_genes() {
        let s = tomato();
        assert!(s.has_gene(crate::botany::genes::REPEATED_HARVEST));
        assert_eq!(
            s.reagents_add(),
            vec![
                ("/datum/reagent/consumable/nutriment/vitamin".into(), 0.04),
                ("/datum/reagent/consumable/nutriment".into(), 0.1)
            ]
        );
    }

    #[test]
    fn prepare_result_scales_with_potency() {
        let mut s = tomato();
        // A tomato (100 units) at potency 10: 1 vitamin (0.4 -> at least 1), 1 nutriment.
        assert_eq!(s.prepare_amounts(100.0)[1].1, 1.0);
        s.set_potency(50.0);
        assert_eq!(s.prepare_amounts(100.0)[0].1, 2.0);
        assert_eq!(s.prepare_amounts(100.0)[1].1, 5.0);
        s.set_potency(100.0);
        assert_eq!(s.prepare_amounts(100.0)[1].1, 10.0);
        // Rates over 100% in total are scaled down.
        s.genes.push(Gene::new_reagent("/datum/reagent/x", 1.0));
        let sum = 1.14;
        assert_eq!(
            s.prepare_amounts(100.0)[1].1,
            (100.0 * 0.1 / sum as f32).floor()
        );
        // Bite size: 1 + round(max(potency * 0.05, 1) * volume / 100).
        assert_eq!(s.bite_size(100.0, 1.0), 6.0);
        s.set_potency(10.0);
        assert_eq!(s.bite_size(100.0, 1.0), 2.0);
    }

    #[test]
    fn setters_keep_tg_bounds() {
        let mut s = tomato();
        s.adjust_yield(100.0);
        assert_eq!(s.yield_, MAX_PLANT_YIELD);
        s.adjust_yield(-100.0);
        assert_eq!(s.yield_, 0.0);
        s.adjust_lifespan(-100.0);
        assert_eq!(s.lifespan, 10.0);
        s.adjust_endurance(500.0);
        assert_eq!(s.endurance, MAX_PLANT_ENDURANCE);
        s.adjust_endurance(-500.0);
        assert_eq!(s.endurance, MIN_PLANT_ENDURANCE);
        s.adjust_production(-5.0);
        assert_eq!(s.production, 1.0);
        s.adjust_potency(1000.0);
        assert_eq!(s.potency, 100.0);
        s.adjust_instability(-100.0);
        assert_eq!(s.instability, 0.0);
        s.adjust_weed_chance(1000.0);
        assert_eq!(s.weed_chance, MAX_PLANT_WEEDCHANCE);
        // Densified chemicals halve the maximum yield.
        s.genes.push(Gene::Trait {
            path: crate::botany::genes::MAXCHEM.into(),
            rate: 2.0,
            mutability: 7,
            ids: 0,
            flags: TRAIT_HALVES_YIELD,
        });
        s.set_yield(10.0);
        assert_eq!(s.yield_, 5.0);
        // An unharvestable plant stays so.
        s.yield_ = -1.0;
        s.adjust_yield(3.0);
        s.adjust_production(3.0);
        assert_eq!(s.yield_, -1.0);
    }

    #[test]
    fn mutation_stays_within_its_range_and_bounds() {
        let plants = test_plants();
        let mut rng = CombatRng(7);
        for _ in 0..500 {
            let mut s = tomato();
            let before = s.clone();
            s.mutate(Mutation::HARD, &plants, &mut rng);
            assert!((s.lifespan - before.lifespan).abs() <= 4.0);
            assert!((s.potency - before.potency).abs() <= 50.0);
            assert!((0.0..=100.0).contains(&s.potency));
            assert!((10.0..=100.0).contains(&s.endurance));
            assert!((0.0..=10.0).contains(&s.yield_));
            assert!((1.0..=10.0).contains(&s.production));
            assert!((0.0..=100.0).contains(&s.instability));
            assert!((0.0..=67.0).contains(&s.weed_chance));
            assert_eq!(s.genes, before.genes, "traitmut 0 adds nothing");
        }
        // Exact: rand() rolls in order lifespan, endurance, production, yield, potency,
        // instability, weed rate, weed chance.
        let mut s = tomato();
        let mut dice = Script {
            rolls: vec![2, -5, 1, -2, 25, 3, 0, 5],
            probs: vec![],
        };
        s.mutate(Mutation::MUTATE, &plants, &mut dice);
        assert_eq!(
            (
                s.lifespan,
                s.endurance,
                s.production,
                s.yield_,
                s.potency,
                s.instability,
                s.weed_rate,
                s.weed_chance
            ),
            (27.0, 10.0, 7.0, 1.0, 35.0, 28.0, 1.0, 10.0)
        );
    }

    #[test]
    fn trait_mutation_adds_a_mutatable_trait_once() {
        let plants = test_plants();
        let mut s = tomato();
        s.genes.retain(|g| matches!(g, Gene::Reagent { .. }));
        // prob(traitmut) and prob(50): a random trait; the pick is the first trait type.
        let mut dice = Script {
            rolls: vec![0, 0, 0, 0, 0, 0, 0, 0, 1, 0],
            probs: vec![true, true],
        };
        s.mutate(Mutation::traits(100.0), &plants, &mut dice);
        let traits: Vec<&str> = s
            .genes
            .iter()
            .filter(|g| matches!(g, Gene::Trait { .. }))
            .map(Gene::path)
            .collect();
        assert_eq!(traits, [plants.data.traits[0].path.as_str()]);
        // The same trait again is refused (`can_add`).
        let mut dice = Script {
            rolls: vec![0, 0, 0, 0, 0, 0, 0, 0, 1, 0],
            probs: vec![true, true],
        };
        s.mutate(Mutation::traits(100.0), &plants, &mut dice);
        assert_eq!(
            s.genes
                .iter()
                .filter(|g| matches!(g, Gene::Trait { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn copy_keeps_stats_and_resets_mutatelist() {
        let plants = test_plants();
        let mut s = tomato();
        s.set_potency(77.0);
        s.mutatelist.clear();
        let c = s.copy(&plants);
        assert_eq!(c.potency, 77.0);
        assert_eq!(c.mutatelist, ["/obj/item/seeds/tomato/blue"]);
    }

    #[test]
    fn yield_follows_get_yield() {
        let s = tomato();
        assert_eq!(s.harvest_yield(1, 1.0), 3.0);
        assert_eq!(s.harvest_yield(1, 0.0), 1.0);
        assert_eq!(s.harvest_yield(1, 1.5), 4.0);
    }

    #[test]
    fn purity_follows_lifespan_endurance_and_instability() {
        let s = tomato();
        let mut dice = Script {
            rolls: vec![0],
            probs: vec![],
        };
        // 0.5 + 25/400 + 15/400.
        assert!((s.reagent_purity(&mut dice) - 0.6).abs() < 1e-6);
        let mut dice = Script {
            rolls: vec![25],
            probs: vec![],
        };
        assert!((s.reagent_purity(&mut dice) - 0.6625).abs() < 1e-6);
    }
}
