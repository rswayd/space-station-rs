//! Hydroponics trays (tg `/obj/machinery/hydroponics`, `hydroponics.dm`,
//! `hydroponics_chemreact.dm`): the tray state with tg's setters and their bounds, the growth
//! cycle of `process()`, the reagents' `on_hydroponics_apply`, mutations (`mutate`,
//! `hardmutate`, `traitmutate`, `mutatespecie`, `mutateweed`, `weedinvasion`), cross-pollination
//! and the sprite (`update_overlays`, `get_tray_overlay`).
//!
//! The cycle is a pure function of the tray, its nutrient holder, the light and the dice, so it
//! is tested without a world.
// shortcut: no snails, soil sacks, somatoray, bees, plumbing or circuits; `mutatepest` warns without spawning
// spiderlings, strange reagent spawns no living plant.

use ss13_reagents::{Chemistry, Holder, is_under};

use super::Plants;
use super::genes::{
    CARNIVORY, FUNGAL, NEVER_MUTATE, SAFE_INSTABILITY, SEMIAQUATIC, STABLE_STATS, TOX_RESISTANCE,
    TOXIN_ADAPTATION, WEED_HARDY, perform_reagent_pollination,
};
use super::seeds::{Dice, Mutation, SOIL, Seed, dm_round};

/// `MAX_TRAY_TOXINS`, `MAX_TRAY_PESTS`, `MAX_TRAY_WEEDS`, `WEED_HARDY_YIELD_MIN`,
/// `CARNIVORY_POTENCY_MIN`, `SEMIAQUATIC_SOIL_WEED_MALUS`, `SUPER_WATER_MODIFIER`.
pub const MAX_TRAY_TOXINS: f32 = 100.0;
pub const MAX_TRAY_PESTS: f32 = 10.0;
pub const MAX_TRAY_WEEDS: f32 = 10.0;
pub const WEED_HARDY_YIELD_MIN: f32 = 3.0;
pub const CARNIVORY_POTENCY_MIN: f32 = 30.0;
pub const SEMIAQUATIC_SOIL_WEED_MALUS: f32 = 1.5;
pub const SUPER_WATER_MODIFIER: f32 = 0.5;
/// Tray flags: `SUPERWATER`, `SLOW_RELEASE`.
pub const SUPERWATER: u32 = 1 << 6;
pub const SLOW_RELEASE: u32 = 1 << 7;
/// `HYDROTRAY_CYCLE_DELAY`, seconds.
pub const HYDROTRAY_CYCLE_DELAY: f64 = 20.0;
const NITROGEN: &str = "/datum/reagent/nitrogen";
const WATER: &str = "/datum/reagent/water";
const KUDZU: &str = "/obj/item/seeds/kudzu";
const BLOOD: &str = "/datum/reagent/blood";
const AMATOXIN: &str = "/datum/reagent/toxin/amatoxin";
const PLASMA: &str = "/datum/reagent/toxin/plasma";
const HOLY_WATER: &str = "/datum/reagent/water/holywater";

/// `plant_status` (`HYDROTRAY_NO_PLANT`, `_PLANT_GROWING`, `_PLANT_DEAD`, `_PLANT_HARVESTABLE`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlantStatus {
    #[default]
    Missing,
    Growing,
    Dead,
    Harvestable,
}

/// A tray's live state. The nutrients are its `Reagents` holder (`create_reagents(maxnutri)`).
#[derive(bevy::prelude::Component, Clone, Debug, Default, PartialEq)]
pub struct Tray {
    pub waterlevel: f32,
    pub maxwater: f32,
    pub maxnutri: f32,
    /// `1 / rating`.
    pub nutridrain: f32,
    pub rating: f32,
    pub pestlevel: f32,
    pub weedlevel: f32,
    pub toxic: f32,
    pub yieldmod: f32,
    pub age: f32,
    pub lastproduce: f32,
    pub plant_status: PlantStatus,
    pub plant_health: f32,
    pub myseed: Option<Seed>,
    pub tray_flags: u32,
    pub being_pollinated: bool,
    /// `world.time` of the last cycle, seconds of game time.
    pub lastcycle: f64,
    pub plant_offset_y: i16,
    pub alt_tray: bool,
    pub status_lights: bool,
    /// tg `self_sustaining` (autogrow, ctrl+click): the tray refills its water and clears weeds
    /// and pests while powered, drains less nutrients and never mutates species or gets
    /// overtaken by weeds. Set through [`super::set_self_sustaining`].
    pub self_sustaining: bool,
}

/// What a cycle asks the world to do next.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CycleOutcome {
    /// `visible_message`s of the tray.
    pub messages: Vec<String>,
    /// `pollinate()` ran: the neighbours get averaged ([`pollinate_pair`]).
    pub pollinate: bool,
    /// `update_appearance()`.
    pub update: bool,
}

/// What a cycle reads besides the tray.
pub struct CycleEnv<'a> {
    pub chem: &'a Chemistry,
    pub plants: &'a Plants,
    /// The tile's `get_static_lumcount()`; `None`: no lighting object (never dark).
    pub lumcount: Option<f32>,
}

impl Tray {
    /// `Initialize(mapload)`-filled tray of a type.
    pub fn new(def: &ss13_mapload::plants::TrayDef) -> Self {
        Self {
            maxwater: def.maxwater,
            maxnutri: def.maxnutri,
            rating: def.rating,
            nutridrain: 1.0 / def.rating,
            yieldmod: 1.0,
            tray_flags: def.tray_flags,
            plant_offset_y: def.plant_offset_y,
            alt_tray: def.alt_tray,
            status_lights: def.status_lights,
            ..Self::default()
        }
    }

    fn has(&self, gene: &str) -> bool {
        self.myseed.as_ref().is_some_and(|s| s.has_gene(gene))
    }

    /// `adjust_waterlevel(amt)`: returns the change. tg's toxin dilution in `set_waterlevel`
    /// reads the level after setting it and never applies.
    pub fn adjust_waterlevel(&mut self, amt: f32) -> f32 {
        let before = self.waterlevel;
        self.waterlevel = (self.waterlevel + amt).clamp(0.0, self.maxwater);
        self.waterlevel - before
    }

    /// `set_plant_health(value, forced)`: nothing without a living plant unless forced.
    pub fn set_plant_health(&mut self, value: f32, forced: bool) {
        if (self.myseed.is_none() || self.plant_status == PlantStatus::Dead) && !forced {
            return;
        }
        self.plant_health = value;
    }

    /// `adjust_plant_health(amt)`: clamped to `0..endurance`.
    pub fn adjust_plant_health(&mut self, amt: f32) {
        let max = self.myseed.as_ref().map_or(0.0, |s| s.endurance);
        self.set_plant_health((self.plant_health + amt).clamp(0.0, max), false);
    }

    pub fn adjust_toxic(&mut self, amt: f32) {
        self.toxic = (self.toxic + amt).clamp(0.0, MAX_TRAY_TOXINS);
    }

    pub fn adjust_pestlevel(&mut self, amt: f32) {
        self.pestlevel = (self.pestlevel + amt).clamp(0.0, MAX_TRAY_PESTS);
    }

    /// `adjust_weedlevel(amt)`: semiaquatic plants out of soil grow weeds faster.
    pub fn adjust_weedlevel(&mut self, amt: f32) {
        let modifier = if self.tray_flags & SOIL == 0 && self.has(SEMIAQUATIC) {
            SEMIAQUATIC_SOIL_WEED_MALUS
        } else {
            1.0
        };
        self.weedlevel = (self.weedlevel + amt * modifier).clamp(0.0, MAX_TRAY_WEEDS);
    }

    /// `set_seed(new_seed)`: growing (or empty) from age 0.
    pub fn set_seed(&mut self, seed: Option<Seed>) {
        self.plant_status = if seed.is_some() {
            PlantStatus::Growing
        } else {
            PlantStatus::Missing
        };
        self.myseed = seed;
        self.age = 0.0;
    }

    /// `plantdies()`.
    pub fn plantdies(&mut self) {
        self.set_plant_health(0.0, true);
        self.plant_status = PlantStatus::Dead;
        self.pestlevel = 0.0;
        self.lastproduce = 0.0;
    }

    /// `remove_plant()`: kills it, clears the weeds, empties the tray and stops autogrow ("no
    /// reason to pay for an empty tray").
    pub fn remove_plant(&mut self) {
        if self.myseed.is_none() {
            return;
        }
        self.plantdies();
        self.weedlevel = 0.0;
        self.self_sustaining = false;
        self.set_seed(None);
    }

    /// The self-sustaining part of `process(seconds_per_tick)` while powered: `rand(1, 2) *
    /// seconds * 0.5` water, `0.5 * seconds` fewer weeds and pests.
    pub fn self_sustain(&mut self, seconds: f32, dice: &mut impl Dice) {
        if !self.self_sustaining {
            return;
        }
        self.adjust_waterlevel(dice.rand(1, 2) as f32 * seconds * 0.5);
        self.adjust_weedlevel(-0.5 * seconds);
        self.adjust_pestlevel(-0.5 * seconds);
    }

    /// `propagate_plant` after its checks: the seed in, full health, a fresh cycle.
    pub fn plant(&mut self, seed: Seed, now: f64) {
        let endurance = seed.endurance;
        self.set_seed(Some(seed));
        self.set_plant_health(endurance, false);
        self.lastcycle = now;
    }

    fn seed_mutate(&mut self, m: Mutation, plants: &Plants, dice: &mut impl Dice) {
        if let Some(seed) = &mut self.myseed {
            seed.mutate(m, plants, dice);
        }
    }

    /// A new seed of `path` replacing the plant (`set_seed(new path)`), hard-mutated, at full
    /// health, weeds gone (`mutatespecie`, `mutateweed`, `weedinvasion` share this).
    fn replace_plant(&mut self, path: &str, plants: &Plants) -> bool {
        let Some(def) = plants.seed_def(path) else {
            return false;
        };
        self.set_seed(Some(Seed::new(def, plants)));
        true
    }

    /// `mutatespecie()`: into one of `mutatelist`.
    pub fn mutatespecie(&mut self, env: &CycleEnv, dice: &mut impl Dice, out: &mut CycleOutcome) {
        let Some(seed) = &self.myseed else {
            return;
        };
        if self.plant_status == PlantStatus::Dead || seed.mutatelist.is_empty() {
            return;
        }
        let old = seed.plantname.clone();
        let path = seed.mutatelist[dice.pick(seed.mutatelist.len())].clone();
        if !self.replace_plant(&path, env.plants) {
            return;
        }
        self.seed_mutate(Mutation::HARD, env.plants, dice);
        let endurance = self.myseed.as_ref().map_or(0.0, |s| s.endurance);
        self.set_plant_health(endurance, false);
        self.weedlevel = 0.0;
        let new = self.plantname();
        out.messages
            .push(format!("{old} suddenly mutates into {new}!"));
        out.update = true;
    }

    /// `mutateweed()`: the weeds become a weed plant.
    pub fn mutateweed(&mut self, env: &CycleEnv, dice: &mut impl Dice, out: &mut CycleOutcome) {
        if self.weedlevel <= 5.0 {
            out.messages.push(
                "The few weeds in the tray seem to react, but only for a moment...".to_owned(),
            );
            return;
        }
        let weeds = ss13_mapload::plants::MUTATED_WEEDS;
        self.set_seed(None);
        if !self.replace_plant(weeds[dice.pick(weeds.len())], env.plants) {
            return;
        }
        self.seed_mutate(Mutation::HARD, env.plants, dice);
        let endurance = self.myseed.as_ref().map_or(0.0, |s| s.endurance);
        self.set_plant_health(endurance, false);
        self.weedlevel = 0.0;
        let name = self.plantname();
        out.messages
            .push(format!("The mutated weeds in the tray spawn some {name}!"));
        out.update = true;
    }

    /// `mutatepest()`.
    // shortcut: tg spawns three hunter spiderlings over 5 pests; spiders are not modelled.
    pub fn mutatepest(&mut self, out: &mut CycleOutcome) {
        out.messages.push(if self.pestlevel <= 5.0 {
            "The pests seem to behave oddly in the tray, but quickly settle down...".to_owned()
        } else {
            "The pests seem to behave oddly...".to_owned()
        });
    }

    /// `weedinvasion()`: a random weed takes the tray.
    pub fn weedinvasion(&mut self, env: &CycleEnv, dice: &mut impl Dice, out: &mut CycleOutcome) {
        let old = self
            .myseed
            .as_ref()
            .map_or_else(|| "empty tray".to_owned(), |s| s.plantname.clone());
        let roll = dice.rand(1, 18) as u32;
        let Some(&(path, _)) = ss13_mapload::plants::WEED_INVASION
            .iter()
            .find(|(_, low)| roll >= *low)
        else {
            return;
        };
        if !self.replace_plant(path, env.plants) {
            return;
        }
        let endurance = self.myseed.as_ref().map_or(0.0, |s| s.endurance);
        self.set_plant_health(endurance, false);
        self.weedlevel = 0.0;
        self.pestlevel = 0.0;
        let name = self.plantname();
        out.messages
            .push(format!("The {old} is overtaken by some {name}!"));
        out.update = true;
    }

    /// `radioactive_exposure(modifier)`.
    pub fn radioactive_exposure(
        &mut self,
        modifier: f32,
        env: &CycleEnv,
        dice: &mut impl Dice,
        out: &mut CycleOutcome,
    ) {
        let Some(seed) = &mut self.myseed else {
            return;
        };
        if dice.prob((25.0 * modifier).min(75.0)) {
            seed.adjust_instability(dm_round(2.0 * modifier));
            self.adjust_toxic(dm_round(1.5 * modifier));
            return;
        }
        let name = seed.plantname.clone();
        match dice.rand(0, 50) {
            41..=50 => {
                self.adjust_plant_health(dm_round(-5.0 * modifier));
                out.messages
                    .push(format!("The {name} starts to wilt and burn!"));
            }
            21..=40 => out
                .messages
                .push(format!("The {name} appears unusually reactive...")),
            11..=20 => {
                if modifier >= 0.5 {
                    self.mutateweed(env, dice, out);
                } else {
                    self.adjust_weedlevel(dm_round(modifier).max(1.0));
                }
            }
            _ => {
                if modifier >= 0.5 {
                    self.mutatepest(out);
                } else {
                    self.adjust_pestlevel(dm_round(modifier).max(1.0));
                }
            }
        }
    }

    pub fn plantname(&self) -> String {
        self.myseed
            .as_ref()
            .map(|s| s.plantname.clone())
            .unwrap_or_default()
    }
}

/// `apply_chemicals()`: every nutrient of at least 1 unit acts on the tray.
pub fn apply_chemicals(
    tray: &mut Tray,
    nutrients: &mut Holder,
    env: &CycleEnv,
    dice: &mut impl Dice,
    out: &mut CycleOutcome,
) {
    if let Some(seed) = tray.myseed.as_mut() {
        on_chem_reaction(seed, nutrients, env.chem, dice);
    }
    let list: Vec<(u16, f32, f32)> = nutrients
        .reagents
        .iter()
        .filter(|r| r.volume >= 1.0)
        .map(|r| (r.id, r.volume, r.creation_purity))
        .collect();
    for (id, volume, creation_purity) in list {
        let def = env.chem.def(id);
        let normalised = if def.purity > 0.0 {
            creation_purity / def.purity
        } else {
            1.0
        };
        let path = def.path.clone();
        on_hydroponics_apply(
            tray, nutrients, id, &path, volume, normalised, env, dice, out,
        );
    }
}

/// `/obj/item/seeds/kudzu/on_chem_reaction` (the only seed with one) on the tray's
/// nutrients: blood and amatoxin shift its production, plasma and holy water its potency.
// shortcut: sterilizine, welding fuel and phenol remove no vine mutations (no space vines yet).
pub fn on_chem_reaction(
    seed: &mut Seed,
    nutrients: &Holder,
    chem: &Chemistry,
    dice: &mut impl Dice,
) {
    if !is_under(&seed.path, KUDZU) {
        return;
    }
    let has = |path: &str, amount: f32| {
        chem.id(path)
            .is_some_and(|id| nutrients.amount(id) >= amount)
    };
    if has(BLOOD, 15.0) {
        seed.adjust_production(dice.rand(15, -5) as f32);
    }
    if has(AMATOXIN, 5.0) {
        seed.adjust_production(dice.rand(5, -15) as f32);
    }
    if has(PLASMA, 5.0) {
        seed.adjust_potency(dice.rand(5, -15) as f32);
    }
    if has(HOLY_WATER, 10.0) {
        seed.adjust_potency(dice.rand(15, -5) as f32);
    }
}

/// tg `/datum/reagent/<type>/on_hydroponics_apply`, the most specific override first.
#[allow(clippy::too_many_arguments)]
fn on_hydroponics_apply(
    tray: &mut Tray,
    nutrients: &mut Holder,
    id: u16,
    path: &str,
    volume: f32,
    normalised_purity: f32,
    env: &CycleEnv,
    dice: &mut impl Dice,
    out: &mut CycleOutcome,
) {
    let r = dm_round;
    let under = |root: &str| is_under(path, root);
    let seed = |tray: &mut Tray, f: &mut dyn FnMut(&mut Seed)| {
        if let Some(s) = &mut tray.myseed {
            f(s);
        }
    };
    if under("/datum/reagent/medicine/adminordrazine") {
        tray.adjust_waterlevel(r(volume));
        tray.adjust_plant_health(r(volume));
        tray.adjust_pestlevel(-dice.rand(1, 5) as f32);
        tray.adjust_weedlevel(-dice.rand(1, 5) as f32);
        if volume < 3.0 {
            return;
        }
        match dice.rand(0, 100) {
            66..=100 => tray.mutatespecie(env, dice, out),
            33..=65 => tray.mutateweed(env, dice, out),
            1..=32 => tray.mutatepest(out),
            _ => {
                if dice.prob(20.0) {
                    out.messages.push("Nothing happens...".to_owned());
                }
            }
        }
    } else if under("/datum/reagent/medicine/cryoxadone") {
        tray.adjust_plant_health(r(volume * 3.0));
        tray.adjust_toxic(-r(volume * 3.0));
    } else if under("/datum/reagent/medicine/c2/multiver") {
        tray.adjust_toxic(-(r(volume * 2.0) * normalised_purity));
    } else if under("/datum/reagent/stabilizing_agent") {
        seed(tray, &mut |s| s.adjust_instability(-r(volume)));
    } else if under("/datum/reagent/napalm") {
        // `resistance_flags & FIRE_PROOF` of the seed: only the fire resistance trait sets it.
        if !tray.has("/datum/plant_gene/trait/fire_resistance") {
            tray.adjust_plant_health(-r(volume * 6.0));
            tray.adjust_toxic(r(volume * 7.0));
        }
        tray.adjust_weedlevel(-dice.rand(5, 9) as f32);
    } else if under("/datum/reagent/consumable/nutriment") {
        tray.adjust_plant_health(r(volume * 0.2));
    } else if under("/datum/reagent/consumable/sugar") {
        tray.adjust_weedlevel(dice.rand(1, 2) as f32);
        tray.adjust_pestlevel(dice.rand(1, 2) as f32);
    } else if under("/datum/reagent/consumable/virus_food") {
        tray.adjust_plant_health(-r(volume * 0.5));
    } else if under("/datum/reagent/consumable/honey") {
        if tray.myseed.is_some() && dice.prob(20.0) {
            out.pollinate = true;
            return;
        }
        tray.adjust_weedlevel(dice.rand(1, 2) as f32);
        tray.adjust_pestlevel(dice.rand(1, 2) as f32);
    } else if under("/datum/reagent/consumable/ethanol/beer") {
        tray.adjust_plant_health(-r(volume * 0.05));
        tray.adjust_waterlevel(r(volume * 0.7));
    } else if under("/datum/reagent/consumable/milk") {
        tray.adjust_waterlevel(r(volume * 0.3));
        if !tray.has(FUNGAL) {
            seed(tray, &mut |s| s.adjust_potency(-r(volume * 0.5)));
        }
    } else if under("/datum/reagent/consumable/sodawater") {
        tray.adjust_waterlevel(r(volume));
        tray.adjust_plant_health(r(volume * 0.1));
    } else if under("/datum/reagent/toxin/mutagen") {
        tray.radioactive_exposure(volume * 0.1, env, dice, out);
        seed(tray, &mut |s| s.adjust_instability(r(volume * 0.2)));
    } else if under("/datum/reagent/toxin/plantbgone/weedkiller") {
        tray.adjust_toxic(r(volume * 0.5));
        tray.adjust_weedlevel(-dice.rand(1, 2) as f32);
    } else if under("/datum/reagent/toxin/plantbgone") {
        tray.adjust_plant_health(-r(volume * 10.0));
        tray.adjust_toxic(r(volume * 6.0));
        tray.adjust_weedlevel(-dice.rand(4, 8) as f32);
    } else if under("/datum/reagent/toxin/pestkiller/organic") {
        tray.adjust_toxic(r(volume * 0.1));
        tray.adjust_pestlevel(-dice.rand(1, 2) as f32);
    } else if under("/datum/reagent/toxin/pestkiller") {
        tray.adjust_toxic(r(volume));
        tray.adjust_pestlevel(-dice.rand(1, 2) as f32);
    } else if under("/datum/reagent/toxin/acid/fluacid") {
        tray.adjust_plant_health(-r(volume * 2.0));
        tray.adjust_toxic(r(volume * 3.0));
        tray.adjust_weedlevel(-dice.rand(1, 4) as f32);
    } else if under("/datum/reagent/toxin/acid") {
        tray.adjust_plant_health(-r(volume));
        tray.adjust_toxic(r(volume * 1.5));
        tray.adjust_weedlevel(-dice.rand(1, 2) as f32);
    } else if under("/datum/reagent/toxin") {
        tray.adjust_toxic(r(volume * 2.0));
    } else if under("/datum/reagent/blood") {
        tray.adjust_pestlevel(dice.rand(2, 3) as f32);
    } else if under("/datum/reagent/water/holywater") {
        tray.adjust_waterlevel(r(volume));
        tray.adjust_plant_health(r(volume * 0.1));
        seed(tray, &mut |s| s.adjust_instability(r(volume * 0.15)));
    } else if under(WATER) {
        tray.adjust_waterlevel(r(volume));
        nutrients.remove_reagent(id, volume);
    } else if under("/datum/reagent/chlorine") {
        tray.adjust_plant_health(-r(volume));
        tray.adjust_toxic(r(volume * 1.5));
        tray.adjust_waterlevel(-r(volume * 0.5));
        tray.adjust_weedlevel(-dice.rand(1, 3) as f32);
    } else if under("/datum/reagent/fluorine") {
        tray.adjust_plant_health(-r(volume * 2.0));
        tray.adjust_toxic(r(volume * 2.5));
        tray.adjust_waterlevel(-r(volume * 0.5));
        tray.adjust_weedlevel(-dice.rand(1, 4) as f32);
    } else if under("/datum/reagent/phosphorus") {
        tray.adjust_plant_health(-r(volume * 0.75));
        tray.adjust_waterlevel(-r(volume * 0.5));
        tray.adjust_weedlevel(-dice.rand(1, 2) as f32);
    } else if under("/datum/reagent/uranium") {
        let tox = env.plants.tox_damage(path);
        tray.radioactive_exposure(volume * 0.1, env, dice, out);
        seed(tray, &mut |s| s.adjust_instability(r(volume * 0.1)));
        tray.adjust_toxic(r(0.5 * volume * tox));
    } else if under("/datum/reagent/ammonia") {
        tray.adjust_plant_health(r(volume * 0.12));
        if tray.myseed.is_some() && dice.prob(10.0) {
            seed(tray, &mut |s| {
                s.adjust_yield(1.0);
                s.adjust_instability(1.0);
            });
        }
    } else if under("/datum/reagent/diethylamine") {
        tray.adjust_plant_health(r(volume));
        tray.adjust_pestlevel(-dice.rand(1, 2) as f32);
        seed(tray, &mut |s| {
            s.adjust_yield(r(volume));
            s.adjust_instability(-r(volume));
        });
    } else if under("/datum/reagent/plantnutriment/eznutriment") {
        seed(tray, &mut |s| {
            s.adjust_instability(0.2);
            s.adjust_potency(r(volume * 0.3));
            s.adjust_yield(r(volume * 0.1));
        });
    } else if under("/datum/reagent/plantnutriment/left4zednutriment") {
        tray.adjust_plant_health(r(volume * 0.1));
        seed(tray, &mut |s| s.adjust_instability(r(volume * 0.2)));
    } else if under("/datum/reagent/plantnutriment/robustharvestnutriment") {
        seed(tray, &mut |s| {
            s.adjust_instability(-0.25);
            s.adjust_potency(r(volume * 0.1));
            s.adjust_yield(r(volume * 0.2));
        });
    } else if under("/datum/reagent/plantnutriment/endurogrow") {
        seed(tray, &mut |s| {
            s.adjust_potency(-r(volume * 0.1));
            s.adjust_yield(-r(volume * 0.075));
            s.adjust_endurance(r(volume * 0.5));
        });
    } else if under("/datum/reagent/plantnutriment/liquidearthquake") {
        seed(tray, &mut |s| {
            s.adjust_weed_rate(r(volume * 0.1));
            s.adjust_weed_chance(r(volume * 0.3));
            s.adjust_production(-r(volume * 0.075));
        });
    } else if under("/datum/reagent/plantnutriment/chemocultivate") {
        seed(tray, &mut |s| {
            s.adjust_weed_rate(-r(volume * 0.05));
            s.adjust_weed_chance(-r(volume * 0.05));
            s.adjust_endurance(-r(volume * 0.5));
            s.adjust_lifespan(-r(volume));
        });
    } else if under("/datum/reagent/plantnutriment/stabilifeed") {
        let Some(s) = &mut tray.myseed else {
            return;
        };
        s.adjust_instability(-r(volume * 0.4));
        let removable: Vec<usize> = (0..s.genes.len())
            .filter(|&i| s.genes[i].mutability() & super::genes::PLANT_GENE_REMOVABLE != 0)
            .collect();
        if removable.is_empty() || !dice.prob(volume * 2.0) {
            return;
        }
        s.genes.remove(removable[dice.pick(removable.len())]);
    } else if under("/datum/reagent/ash") {
        tray.adjust_plant_health(r(volume));
        tray.adjust_weedlevel(-1.0);
    } else if under("/datum/reagent/saltpetre") {
        tray.adjust_plant_health(r(volume * 0.18));
        let chance = f32::from(dice.prob(volume % 10.0));
        seed(tray, &mut |s| {
            s.adjust_production(-r(volume / 10.0) - chance);
            s.adjust_potency(r(volume));
        });
    } else if under("/datum/reagent/brimdust") {
        tray.adjust_weedlevel(-1.0);
        tray.adjust_pestlevel(-1.0);
        tray.adjust_plant_health(r(volume));
        seed(tray, &mut |s| s.adjust_potency(r(volume * 0.5)));
    } else if under("/datum/reagent/drug/nicotine") {
        tray.adjust_toxic(r(volume));
        tray.adjust_pestlevel(-dice.rand(1, 2) as f32);
    } else if under("/datum/reagent/inverse/sal_acid") {
        tray.adjust_plant_health(r(volume * 0.5));
        seed(tray, &mut |s| {
            s.adjust_production(-r(volume * 0.2));
            s.adjust_potency(r(volume * 0.25));
            s.adjust_yield(r(volume * 0.2));
        });
    }
}

/// One growth cycle of `process()` (`world.time > lastcycle + cycledelay`).
pub fn cycle(
    tray: &mut Tray,
    nutrients: &mut Holder,
    env: &CycleEnv,
    dice: &mut impl Dice,
) -> CycleOutcome {
    let mut out = CycleOutcome::default();
    let rating = tray.rating;
    if tray.myseed.is_some() && tray.plant_status != PlantStatus::Dead {
        let is_fungus = tray.has(FUNGAL);
        // tg's `1 * (is_fungus && FAST_MUSHROOMS) ? FAST_MUSH_MODIFIER : 1` is `(1 * x) ? ...`:
        // a fungus in coir soil gets 1.4, anything else 1.
        // shortcut: the coir bonus is not read (no soil sacks to make coir trays).
        tray.age += 1.0;
        let maturation = tray.myseed.as_ref().map_or(0.0, |s| s.maturation);
        if tray.age < maturation {
            tray.lastproduce = tray.age;
        }
        out.update = true;

        // Nutrients.
        apply_chemicals(tray, nutrients, env, dice, &mut out);
        nutrients.remove_all(if tray.self_sustaining {
            tray.nutridrain.min(0.5)
        } else {
            tray.nutridrain
        });
        if nutrients.total_volume <= 0.0 {
            if tray.tray_flags & SLOW_RELEASE != 0 {
                if let Some(id) = env.chem.id(NITROGEN) {
                    nutrients.add_reagent(
                        env.chem,
                        id,
                        1.0,
                        ss13_reagents::DEFAULT_REAGENT_TEMPERATURE,
                        None,
                        None,
                        false,
                        false,
                    );
                }
            } else if !tray.has(WEED_HARDY) {
                tray.adjust_plant_health(-dice.rand(1, 3) as f32);
            }
        }

        // Photosynthesis.
        let threshold = if is_fungus { 0.2 } else { 0.4 };
        if env.lumcount.is_some_and(|l| l < threshold) {
            tray.adjust_plant_health(if is_fungus { -1.0 } else { -2.0 } / rating);
        }

        // Water.
        let thirst = if tray.tray_flags & SUPERWATER != 0 {
            SUPER_WATER_MODIFIER
        } else {
            1.0
        };
        tray.adjust_waterlevel(-dice.rand(1, 6) as f32 * thirst / rating);
        if tray.waterlevel <= 10.0 && !is_fungus {
            tray.adjust_plant_health(-dice.rand(0, 1) as f32 / rating);
            if tray.waterlevel <= 0.0 {
                tray.adjust_plant_health(-dice.rand(0, 2) as f32 / rating);
            }
        } else if tray.waterlevel > 10.0 && nutrients.total_volume > 0.0 {
            tray.adjust_plant_health(dice.rand(1, 2) as f32 / rating);
            let (chance, rate) = tray
                .myseed
                .as_ref()
                .map_or((0.0, 0.0), |s| (s.weed_chance, s.weed_rate));
            if dice.prob(chance) {
                tray.adjust_weedlevel(rate);
            } else if dice.prob(5.0) {
                tray.adjust_weedlevel(1.0 / rating);
            }
        }

        // Toxins.
        if tray.toxic >= 10.0 {
            if tray.has(TOXIN_ADAPTATION) {
                let health = dm_round(tray.toxic / dice.rand(10, 16) as f32);
                tray.adjust_plant_health(health);
                let potency = dm_round(tray.toxic / dice.rand(20, 30) as f32);
                if let Some(s) = &mut tray.myseed {
                    s.adjust_potency(potency);
                }
            } else if tray.toxic >= 40.0 && !tray.has(TOX_RESISTANCE) {
                if tray.toxic < 80.0 {
                    tray.adjust_plant_health(-1.0 / rating);
                    tray.adjust_toxic(-rating * 2.0);
                } else {
                    tray.adjust_plant_health(-3.0);
                    tray.adjust_toxic(-rating * 3.0);
                }
            }
        }

        // Pests and weeds.
        let carnivory = tray.has(CARNIVORY);
        if tray.pestlevel >= 8.0 {
            if !carnivory {
                let loss = dice.rand(2, 6) as f32;
                if let Some(s) = &mut tray.myseed
                    && s.potency >= 30.0
                {
                    // Pests eat leaves and nibble on fruit (tg caps it at the carnivory minimum).
                    s.set_potency((s.potency - loss).min(CARNIVORY_POTENCY_MIN));
                }
            } else {
                tray.adjust_plant_health(2.0 / rating);
                tray.adjust_pestlevel(-1.0 / rating);
            }
        } else if tray.pestlevel >= 4.0 {
            if !carnivory {
                let loss = dice.rand(1, 4) as f32;
                if let Some(s) = &mut tray.myseed
                    && s.potency >= 30.0
                {
                    s.set_potency((s.potency - loss).min(CARNIVORY_POTENCY_MIN));
                }
            } else {
                tray.adjust_plant_health(1.0 / rating);
                if dice.prob(50.0) {
                    tray.adjust_pestlevel(-1.0 / rating);
                }
            }
        } else if carnivory && dice.prob(5.0) {
            tray.adjust_pestlevel(-1.0 / rating);
        }
        if tray.weedlevel >= 5.0 && !tray.has(WEED_HARDY) {
            let loss = tray
                .myseed
                .as_ref()
                .is_some_and(|s| s.yield_ >= 3.0)
                .then(|| dice.rand(1, 2) as f32);
            if let (Some(loss), Some(s)) = (loss, &mut tray.myseed) {
                s.adjust_yield(-loss);
                s.set_yield(s.yield_.clamp(WEED_HARDY_YIELD_MIN, 10.0));
            }
        }

        // Pollination.
        out.pollinate = true;

        // Instability mutations.
        let instability = tray.myseed.as_ref().map_or(0.0, |s| s.instability);
        if instability >= 80.0 {
            tray.seed_mutate(Mutation::traits(instability - 75.0), env.plants, dice);
        }
        let instability = tray.myseed.as_ref().map_or(0.0, |s| s.instability);
        if instability >= 60.0 {
            let can = tray
                .myseed
                .as_ref()
                .is_some_and(|s| !s.mutatelist.is_empty() && !s.has_gene(NEVER_MUTATE));
            if dice.prob(instability / 2.0) && !tray.self_sustaining && can {
                tray.mutatespecie(env, dice, &mut out);
                if let Some(s) = &mut tray.myseed {
                    s.set_instability(s.instability / 2.0);
                }
            }
        }
        let instability = tray.myseed.as_ref().map_or(0.0, |s| s.instability);
        if instability >= 20.0 && dice.prob(instability) && !tray.has(STABLE_STATS) {
            if instability >= 40.0 {
                let stabmut = if instability >= 80.0 { 5 } else { 0 };
                tray.seed_mutate(
                    Mutation {
                        stabmut,
                        ..Mutation::HARD
                    },
                    env.plants,
                    dice,
                );
            } else {
                tray.seed_mutate(
                    Mutation {
                        stabmut: 0,
                        ..Mutation::MUTATE
                    },
                    env.plants,
                    dice,
                );
            }
        }

        // Health and age.
        if tray.plant_health <= 0.0 {
            tray.plantdies();
            tray.adjust_weedlevel(1.0 / rating);
        }
        let (lifespan, production, harvestable) =
            tray.myseed.as_ref().map_or((0.0, 0.0, false), |s| {
                (s.lifespan, s.production, s.yield_ != -1.0)
            });
        if tray.age > lifespan {
            tray.adjust_plant_health(-dice.rand(1, 5) as f32 / rating);
        }
        if tray.age > production
            && tray.age - tray.lastproduce > production
            && tray.plant_status == PlantStatus::Growing
        {
            if harvestable {
                tray.plant_status = PlantStatus::Harvestable;
            } else {
                tray.lastproduce = tray.age;
            }
        }
        if dice.prob(5.0) {
            tray.adjust_pestlevel(1.0 / rating);
        }
    } else if tray.waterlevel > 10.0 && nutrients.total_volume > 0.0 && dice.prob(10.0) {
        tray.adjust_weedlevel(1.0 / rating);
    }

    // Weeds overtaking the plant spot.
    if tray.weedlevel >= 10.0 && dice.prob(50.0) && !tray.self_sustaining {
        let loss = tray
            .myseed
            .as_ref()
            .is_some_and(|s| s.yield_ >= 3.0)
            .then(|| dice.rand(1, 2) as f32);
        if let (Some(loss), Some(s)) = (loss, &mut tray.myseed) {
            s.adjust_yield(-loss);
            s.set_yield(s.yield_.clamp(WEED_HARDY_YIELD_MIN, 10.0));
        }
        if tray.myseed.is_none() {
            tray.weedinvasion(env, dice, &mut out);
        }
        out.update = true;
    }
    out
}

/// `pollinate()` from `source` onto the neighbour `target` (a living plant): potency and
/// instability 10% and yield 50% towards the source's, and maybe a reagent gene of the target
/// for the source.
pub fn pollinate_pair(source: &mut Seed, target: &mut Seed, dice: &mut impl Dice) {
    target.set_potency(dm_round(
        target.potency + 0.1 * (source.potency - target.potency),
    ));
    if !target.has_gene(SAFE_INSTABILITY) {
        target.set_instability(dm_round(
            target.instability + 0.1 * (source.instability - target.instability),
        ));
    }
    target.set_yield(dm_round(
        target.yield_ + 0.5 * (source.yield_ - target.yield_),
    ));
    if source.instability >= 20.0 && dice.prob(70.0) && !target.reagents_add().is_empty() {
        perform_reagent_pollination(source, target, dice);
    }
}

/// `get_tray_overlay(age, status)`: the plant's sprite state.
pub fn plant_icon_state(
    seed_def: &ss13_mapload::plants::SeedDef,
    age: f32,
    status: PlantStatus,
) -> String {
    match status {
        PlantStatus::Dead => seed_def.icon_dead.clone(),
        PlantStatus::Harvestable if !seed_def.icon_harvest.is_empty() => {
            seed_def.icon_harvest.clone()
        }
        PlantStatus::Harvestable => format!("{}{}", seed_def.icon_grow, seed_def.growthstages),
        _ => {
            let stages = seed_def.growthstages.max(1) as f32;
            let stage = dm_round(age / seed_def.maturation.max(1.0) * stages).clamp(1.0, stages);
            format!("{}{}", seed_def.icon_grow, stage as u32)
        }
    }
}

/// `update_status_light_overlays()`: the indicator lights (equipment.dmi states).
pub fn status_lights(tray: &Tray, nutrients: f32) -> Vec<String> {
    if !tray.status_lights || tray.myseed.is_none() {
        return Vec::new();
    }
    let suffix = if tray.alt_tray { "-alt" } else { "" };
    let endurance = tray.myseed.as_ref().map_or(0.0, |s| s.endurance);
    let mut out = Vec::new();
    if tray.waterlevel <= 10.0 {
        out.push(format!("over_lowwater3{suffix}"));
    }
    if nutrients <= 2.0 {
        out.push(format!("over_lownutri3{suffix}"));
    }
    if tray.plant_health <= endurance / 2.0 {
        out.push(format!("over_lowhealth3{suffix}"));
    }
    if tray.weedlevel >= 5.0 || tray.pestlevel >= 5.0 || tray.toxic >= 40.0 {
        out.push(format!("over_alert3{suffix}"));
    }
    if tray.plant_status == PlantStatus::Harvestable {
        out.push(format!("over_harvest3{suffix}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use ss13_reagents::{ChemData, Owner, ReagentDef};

    use super::*;
    use crate::botany::{Script, test_plants};

    fn reagent(path: &str) -> ReagentDef {
        ReagentDef {
            path: path.into(),
            name: path.rsplit('/').next().unwrap_or_default().into(),
            description: String::new(),
            color: "#000000".into(),
            taste_description: String::new(),
            taste_mult: 1.0,
            metabolization_rate: 0.2,
            overdose_threshold: 0.0,
            ph: 7.0,
            specific_heat: 200.0,
            chemical_flags: 0,
            purity: 1.0,
            creation_purity: 1.0,
            inverse_chem: None,
            inverse_chem_val: 0.0,
            purge_multiplier: 1.0,
            toxpwr: 0.0,
            health_required: 0.0,
            liver_tolerance_multiplier: 1.0,
            boozepwr: 0.0,
            brute_heal: 0.0,
            nutriment_factor: 0.0,
            default_container: String::new(),
        }
    }

    fn chem() -> Chemistry {
        Chemistry::new(ChemData {
            glass_styles: Vec::new(),
            reagents: [
                WATER,
                "/datum/reagent/plantnutriment/eznutriment",
                "/datum/reagent/plantnutriment/robustharvestnutriment",
                "/datum/reagent/toxin/mutagen",
                "/datum/reagent/toxin/plantbgone/weedkiller",
                "/datum/reagent/ammonia",
            ]
            .into_iter()
            .map(reagent)
            .collect(),
            reactions: vec![],
        })
    }

    fn holder(chem: &Chemistry, list: &[(&str, f32)]) -> Holder {
        let mut h = Holder::new(20.0, ss13_reagents::flags::INJECTABLE, Owner::default());
        let list: Vec<(u16, f32)> = list
            .iter()
            .map(|(p, a)| (chem.id(p).unwrap(), *a))
            .collect();
        h.add_reagent_list(chem, &list, None, true);
        h
    }

    #[test]
    fn kudzu_reacts_to_blood_and_plasma_in_the_tray() {
        let plants = test_plants();
        let chem = Chemistry::new(ChemData {
            glass_styles: Vec::new(),
            reagents: [WATER, BLOOD, PLASMA].into_iter().map(reagent).collect(),
            reactions: vec![],
        });
        let nutrients = holder(&chem, &[(BLOOD, 15.0), (PLASMA, 4.0)]);
        let mut seed = Seed::new(plants.seed_def("/obj/item/seeds/tomato").unwrap(), &plants);
        let mut dice = Script {
            rolls: vec![-3, 5],
            ..Default::default()
        };
        // Not kudzu: nothing.
        on_chem_reaction(&mut seed, &nutrients, &chem, &mut dice);
        assert_eq!(seed.production, 6.0);
        seed.path = KUDZU.into();
        on_chem_reaction(&mut seed, &nutrients, &chem, &mut dice);
        assert_eq!(seed.production, 3.0);
        // 4 u of plasma is below its 5 u threshold.
        assert_eq!(seed.potency, 10.0);
    }

    fn planted(plants: &Plants) -> Tray {
        let mut tray = Tray::new(&plants.data.trays[0]);
        tray.waterlevel = 100.0;
        let seed = Seed::new(plants.seed_def("/obj/item/seeds/tomato").unwrap(), plants);
        tray.plant(seed, 0.0);
        tray
    }

    fn seed(tray: &Tray) -> &Seed {
        tray.myseed.as_ref().unwrap()
    }

    #[test]
    fn one_cycle_drinks_water_uses_nutrients_and_applies_them() {
        let (plants, chem) = (test_plants(), chem());
        let env = CycleEnv {
            chem: &chem,
            plants: &plants,
            lumcount: Some(1.0),
        };
        let mut tray = planted(&plants);
        tray.plant_health = 10.0;
        let mut nutrients = holder(
            &chem,
            &[("/datum/reagent/plantnutriment/eznutriment", 10.0)],
        );
        // rand(1, 6) water = 4, rand(1, 2) health = 2; no prob succeeds.
        let mut dice = Script {
            rolls: vec![4, 2],
            probs: vec![],
        };
        let out = cycle(&mut tray, &mut nutrients, &env, &mut dice);
        assert!(out.update && out.pollinate);
        assert_eq!((tray.age, tray.lastproduce), (1.0, 1.0));
        assert_eq!(tray.waterlevel, 96.0, "drinks rand(1, 6) / rating");
        assert_eq!(nutrients.total_volume, 9.0, "nutridrain 1 per cycle");
        assert_eq!(tray.plant_health, 12.0, "watered and fed: +rand(1, 2)");
        // E-Z-Nutrient (10u): potency + round(10 * 0.3), yield + round(10 * 0.1), +0.2
        // instability.
        let s = seed(&tray);
        assert_eq!((s.potency, s.yield_), (13.0, 4.0));
        assert!((s.instability - 25.2).abs() < 1e-4);
    }

    #[test]
    fn water_poured_into_the_nutrients_fills_the_tray() {
        let (plants, chem) = (test_plants(), chem());
        let env = CycleEnv {
            chem: &chem,
            plants: &plants,
            lumcount: None,
        };
        let mut tray = planted(&plants);
        tray.waterlevel = 20.0;
        let mut nutrients = holder(&chem, &[(WATER, 15.0)]);
        let mut dice = Script::default();
        cycle(&mut tray, &mut nutrients, &env, &mut dice);
        // +15 from the water reagent, -1 drunk.
        assert_eq!(tray.waterlevel, 34.0);
        assert_eq!(nutrients.total_volume, 0.0, "water leaves the holder");
    }

    #[test]
    fn dry_dark_and_starving_plants_lose_health() {
        let (plants, chem) = (test_plants(), chem());
        let env = CycleEnv {
            chem: &chem,
            plants: &plants,
            lumcount: Some(0.1),
        };
        let mut tray = planted(&plants);
        tray.waterlevel = 3.0;
        let mut nutrients = holder(&chem, &[]);
        // Starving -rand(1, 3) = 2, dark -2, water -3 -> 0, dry -rand(0, 1) = 1,
        // empty -rand(0, 2) = 2.
        let mut dice = Script {
            rolls: vec![2, 3, 1, 2],
            probs: vec![],
        };
        cycle(&mut tray, &mut nutrients, &env, &mut dice);
        assert_eq!(tray.waterlevel, 0.0);
        assert_eq!(tray.plant_health, 15.0 - 2.0 - 2.0 - 1.0 - 2.0);
    }

    #[test]
    fn a_tomato_grows_through_its_stages_and_becomes_harvestable_at_age_14() {
        let (plants, chem) = (test_plants(), chem());
        let env = CycleEnv {
            chem: &chem,
            plants: &plants,
            lumcount: None,
        };
        let def = plants.seed_def("/obj/item/seeds/tomato").unwrap().clone();
        let mut tray = planted(&plants);
        let mut nutrients = holder(
            &chem,
            &[("/datum/reagent/plantnutriment/eznutriment", 20.0)],
        );
        let mut stages = Vec::new();
        let mut dice = Script::default();
        while tray.plant_status == PlantStatus::Growing {
            assert!(tray.age < 30.0, "never harvestable");
            cycle(&mut tray, &mut nutrients, &env, &mut dice);
            stages.push(plant_icon_state(&def, tray.age, tray.plant_status));
        }
        // maturation 8 (`lastproduce` follows the age up to 7), production 6: harvestable once
        // `age - 7 > 6`.
        assert_eq!(tray.age, 14.0);
        assert_eq!(tray.plant_status, PlantStatus::Harvestable);
        assert_eq!(stages[0], "tomato-grow1");
        assert_eq!(stages[3], "tomato-grow3", "round(4 / 8 * 6)");
        assert_eq!(stages[7], "tomato-grow6");
        assert_eq!(stages.last().unwrap(), "tomato-harvest");
        assert!(status_lights(&tray, 10.0).contains(&"over_harvest3".to_owned()));
        assert!(tray.plant_health > 0.0);
    }

    #[test]
    fn health_at_zero_kills_and_old_age_hurts() {
        let (plants, chem) = (test_plants(), chem());
        let env = CycleEnv {
            chem: &chem,
            plants: &plants,
            lumcount: None,
        };
        let mut tray = planted(&plants);
        tray.plant_health = 1.0;
        tray.waterlevel = 5.0;
        let mut nutrients = holder(&chem, &[("/datum/reagent/plantnutriment/eznutriment", 5.0)]);
        let mut dice = Script {
            rolls: vec![1, 1],
            probs: vec![],
        };
        cycle(&mut tray, &mut nutrients, &env, &mut dice);
        assert_eq!(tray.plant_status, PlantStatus::Dead);
        assert_eq!((tray.plant_health, tray.weedlevel), (0.0, 1.0));
        // A dead plant does not age.
        cycle(&mut tray, &mut nutrients, &env, &mut Script::default());
        assert_eq!(tray.age, 1.0);

        let mut tray = planted(&plants);
        tray.age = 25.0;
        let mut nutrients = holder(&chem, &[("/datum/reagent/plantnutriment/eznutriment", 5.0)]);
        // water 1, +1 health, then past the lifespan -rand(1, 5) = 4.
        let mut dice = Script {
            rolls: vec![1, 1, 4],
            probs: vec![],
        };
        cycle(&mut tray, &mut nutrients, &env, &mut dice);
        assert_eq!(
            tray.plant_health,
            15.0 - 4.0,
            "health is capped at endurance first"
        );
    }

    #[test]
    fn toxins_hurt_and_drain() {
        let (plants, chem) = (test_plants(), chem());
        let env = CycleEnv {
            chem: &chem,
            plants: &plants,
            lumcount: None,
        };
        let mut tray = planted(&plants);
        tray.toxic = 50.0;
        tray.plant_health = 10.0;
        let mut nutrients = holder(&chem, &[("/datum/reagent/plantnutriment/eznutriment", 5.0)]);
        let mut dice = Script {
            rolls: vec![1, 1],
            probs: vec![],
        };
        cycle(&mut tray, &mut nutrients, &env, &mut dice);
        assert_eq!(tray.toxic, 48.0);
        assert_eq!(tray.plant_health, 10.0 + 1.0 - 1.0);
        // Weedkiller: +round(volume * 0.5) toxins, -rand(1, 2) weeds.
        let mut tray = planted(&plants);
        tray.weedlevel = 6.0;
        let mut nutrients = holder(
            &chem,
            &[("/datum/reagent/toxin/plantbgone/weedkiller", 4.0)],
        );
        let mut dice = Script {
            rolls: vec![2],
            probs: vec![],
        };
        cycle(&mut tray, &mut nutrients, &env, &mut dice);
        assert_eq!(tray.toxic, 2.0);
        assert_eq!(tray.weedlevel, 4.0);
    }

    #[test]
    fn mutagen_raises_instability_and_unstable_plants_mutate_species() {
        let (plants, chem) = (test_plants(), chem());
        let env = CycleEnv {
            chem: &chem,
            plants: &plants,
            lumcount: None,
        };
        let mut tray = planted(&plants);
        let mut nutrients = holder(&chem, &[("/datum/reagent/toxin/mutagen", 10.0)]);
        // radioactive_exposure(1): prob(25) succeeds -> +2 instability, +1 toxin; then
        // +round(10 * 0.2) instability.
        let mut dice = Script {
            rolls: vec![],
            probs: vec![true],
        };
        cycle(&mut tray, &mut nutrients, &env, &mut dice);
        assert_eq!(seed(&tray).instability, 29.0);
        assert_eq!(tray.toxic, 1.0);

        // Instability 70: prob(35) -> mutatespecie into the mutatelist, then halved; the
        // hardmutate rolls are all 0 here.
        let mut tray = planted(&plants);
        tray.myseed.as_mut().unwrap().instability = 70.0;
        tray.age = 5.0;
        let mut nutrients = holder(&chem, &[]);
        let mut dice = Script {
            rolls: vec![],
            probs: vec![true],
        };
        let out = cycle(&mut tray, &mut nutrients, &env, &mut dice);
        let s = seed(&tray);
        assert_eq!(s.path, "/obj/item/seeds/tomato/blue");
        assert_eq!(s.instability, 12.5, "a fresh blue tomato (25), halved");
        assert_eq!(tray.age, 0.0, "set_seed restarts the age");
        assert_eq!(tray.plant_health, 15.0);
        assert!(out.messages[0].contains("suddenly mutates into Blue-Tomato Plants"));
    }

    #[test]
    fn weeds_overtake_an_empty_tray() {
        let (plants, chem) = (test_plants(), chem());
        let env = CycleEnv {
            chem: &chem,
            plants: &plants,
            lumcount: None,
        };
        let mut tray = Tray::new(&plants.data.trays[0]);
        tray.weedlevel = 10.0;
        let mut nutrients = holder(&chem, &[]);
        // prob(50) succeeds, rand(1, 18) = 2: starthistle.
        let mut dice = Script {
            rolls: vec![2],
            probs: vec![true],
        };
        let out = cycle(&mut tray, &mut nutrients, &env, &mut dice);
        assert_eq!(seed(&tray).path, "/obj/item/seeds/starthistle");
        assert_eq!((tray.weedlevel, tray.plant_health), (0.0, 15.0));
        assert_eq!(tray.plant_status, PlantStatus::Growing);
        assert!(out.messages[0].contains("empty tray is overtaken by some starthistle"));
    }

    #[test]
    fn autogrow_tends_the_tray_and_blocks_mutations_and_weed_invasions() {
        let (plants, chem) = (test_plants(), chem());
        let env = CycleEnv {
            chem: &chem,
            plants: &plants,
            lumcount: None,
        };
        // `process(2)`: rand(1, 2) = 2 -> +2 water, -1 weeds, -1 pests.
        let mut tray = planted(&plants);
        tray.waterlevel = 50.0;
        tray.weedlevel = 5.0;
        tray.pestlevel = 3.0;
        tray.self_sustain(2.0, &mut Script::default());
        assert_eq!(tray.waterlevel, 50.0, "off: nothing");
        tray.self_sustaining = true;
        tray.self_sustain(
            2.0,
            &mut Script {
                rolls: vec![2],
                probs: vec![],
            },
        );
        assert_eq!(
            (tray.waterlevel, tray.weedlevel, tray.pestlevel),
            (52.0, 4.0, 2.0)
        );

        // Nutrients drain at most 0.5 per cycle; instability 70 rolls prob(35) (after the two
        // weed rolls) but no species mutation happens.
        tray.myseed.as_mut().unwrap().instability = 70.0;
        let mut nutrients = holder(
            &chem,
            &[("/datum/reagent/plantnutriment/eznutriment", 10.0)],
        );
        let mut dice = Script {
            rolls: vec![],
            probs: vec![false, false, true],
        };
        cycle(&mut tray, &mut nutrients, &env, &mut dice);
        assert_eq!(nutrients.total_volume, 9.5);
        assert_eq!(seed(&tray).path, "/obj/item/seeds/tomato");

        // Full weeds never overtake an autogrowing empty tray.
        let mut tray = Tray::new(&plants.data.trays[0]);
        tray.self_sustaining = true;
        tray.weedlevel = 10.0;
        let mut dice = Script {
            rolls: vec![2],
            probs: vec![true],
        };
        cycle(&mut tray, &mut holder(&chem, &[]), &env, &mut dice);
        assert!(tray.myseed.is_none());

        // Digging the plant out turns it off.
        let mut tray = planted(&plants);
        tray.self_sustaining = true;
        tray.remove_plant();
        assert!(!tray.self_sustaining);
    }

    #[test]
    fn pollination_averages_towards_the_source() {
        let plants = test_plants();
        let def = plants.seed_def("/obj/item/seeds/tomato").unwrap();
        let mut a = Seed::new(def, &plants);
        let mut b = Seed::new(def, &plants);
        a.potency = 60.0;
        a.yield_ = 9.0;
        a.instability = 10.0;
        b.potency = 10.0;
        b.yield_ = 3.0;
        b.instability = 30.0;
        pollinate_pair(&mut a, &mut b, &mut Script::default());
        assert_eq!((b.potency, b.yield_, b.instability), (15.0, 6.0, 28.0));
    }

    #[test]
    fn status_lights_follow_update_overlays() {
        let plants = test_plants();
        let mut tray = planted(&plants);
        assert!(status_lights(&tray, 10.0).is_empty());
        tray.waterlevel = 5.0;
        tray.plant_health = 7.0;
        tray.pestlevel = 5.0;
        assert_eq!(
            status_lights(&tray, 1.0),
            [
                "over_lowwater3",
                "over_lownutri3",
                "over_lowhealth3",
                "over_alert3"
            ]
        );
        tray.status_lights = false;
        assert!(status_lights(&tray, 1.0).is_empty(), "soil has none");
    }
}
