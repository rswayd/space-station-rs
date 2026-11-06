//! Holder math, reaction matching and metabolism amounts against tg's formulas.

use ss13_reagents::metabolism::{
    Body, Damage, Liver, cryoxadone_power, digest, inebriation_tick, metabolize, metabolize_with,
    pyroxadone_power,
};
use ss13_reagents::{
    ChemData, Chemistry, DEFAULT_REAGENT_TEMPERATURE, Holder, Owner, ReactionDef, ReagentDef,
    flags, mix_color, reaction_flags,
};

fn reagent(path: &str, name: &str) -> ReagentDef {
    ReagentDef {
        path: path.into(),
        name: name.into(),
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
        inverse_chem_val: 0.25,
        purge_multiplier: 1.0,
        toxpwr: 0.0,
        health_required: -100.0,
        liver_tolerance_multiplier: 1.0,
        boozepwr: 0.0,
        brute_heal: 0.0,
        nutriment_factor: 0.0,
        default_container: String::new(),
    }
}

fn reaction(path: &str, required: &[(&str, f32)], results: &[(&str, f32)]) -> ReactionDef {
    let list = |l: &[(&str, f32)]| l.iter().map(|(p, n)| ((*p).to_owned(), *n)).collect();
    ReactionDef {
        path: path.into(),
        required_reagents: list(required),
        results: list(results),
        required_catalysts: Vec::new(),
        required_container: None,
        required_container_accepts_subtypes: false,
        mob_react: true,
        mix_message: "The solution begins to bubble.".into(),
        is_cold_recipe: false,
        required_temp: 100.0,
        optimal_temp: 500.0,
        overheat_temp: 900.0,
        optimal_ph_min: 5.0,
        optimal_ph_max: 9.0,
        determin_ph_range: 4.0,
        temp_exponent_factor: 2.0,
        ph_exponent_factor: 2.0,
        thermic_constant: 0.0,
        h_ion_release: 0.0,
        rate_up_lim: 30.0,
        purity_min: 0.15,
        reaction_flags: reaction_flags::REACTION_INSTANT,
        special: false,
        explosion: None,
        soup: None,
    }
}

const WATER: &str = "/datum/reagent/water";
const ETHANOL: &str = "/datum/reagent/consumable/ethanol";
const A: &str = "/datum/reagent/a";
const B: &str = "/datum/reagent/b";
const C: &str = "/datum/reagent/c";
const CAT: &str = "/datum/reagent/catalyst";
const D: &str = "/datum/reagent/d";
const E: &str = "/datum/reagent/e";
const F: &str = "/datum/reagent/f";
const LIBITAL: &str = "/datum/reagent/medicine/c2/libital";
const TOXIN: &str = "/datum/reagent/toxin";
const CRYOXADONE: &str = "/datum/reagent/medicine/cryoxadone";

fn chem() -> Chemistry {
    let mut libital = reagent(LIBITAL, "Libital");
    libital.metabolization_rate = 0.1;
    libital.purity = 0.75;
    libital.creation_purity = 0.75;
    let mut toxin = reagent(TOXIN, "Toxin");
    toxin.toxpwr = 1.5;
    let mut ethanol = reagent(ETHANOL, "Ethanol");
    ethanol.metabolization_rate = 0.1;
    ethanol.boozepwr = 65.0;
    let mut water = reagent(WATER, "Water");
    water.color = "#AAAAAA77".into();
    let mut a = reagent(A, "A");
    a.color = "#FF0000".into();
    let mut slow = reaction(
        "/datum/chemical_reaction/slow",
        &[(D, 1.0), (E, 1.0)],
        &[(F, 2.0)],
    );
    slow.reaction_flags = 0;
    slow.rate_up_lim = 10.0;
    slow.optimal_temp = 250.0;
    let mut catalysed = reaction("/datum/chemical_reaction/cat", &[(B, 1.0)], &[(C, 1.0)]);
    catalysed.required_catalysts = vec![(CAT.into(), 5.0)];
    let mut hot = reaction(
        "/datum/chemical_reaction/hot",
        &[(C, 1.0), (WATER, 1.0)],
        &[(A, 1.0)],
    );
    hot.required_temp = 400.0;
    Chemistry::new(ChemData {
        glass_styles: Vec::new(),
        reagents: vec![
            water,
            ethanol,
            a,
            reagent(B, "B"),
            reagent(C, "C"),
            reagent(CAT, "Catalyst"),
            reagent(D, "D"),
            reagent(E, "E"),
            reagent(F, "F"),
            libital,
            toxin,
            reagent(CRYOXADONE, "Cryoxadone"),
        ],
        reactions: vec![
            reaction(
                "/datum/chemical_reaction/ab",
                &[(A, 2.0), (B, 1.0)],
                &[(C, 3.0)],
            ),
            catalysed,
            hot,
            slow,
        ],
    })
}

fn beaker(max: f32) -> Holder {
    Holder::new(
        max,
        flags::OPENCONTAINER,
        Owner {
            path: "/obj/item/reagent_containers/cup/beaker".into(),
            living: false,
        },
    )
}

fn add(h: &mut Holder, chem: &Chemistry, path: &str, amount: f32) -> f32 {
    let id = chem.id(path).unwrap();
    h.add_reagent(
        chem,
        id,
        amount,
        DEFAULT_REAGENT_TEMPERATURE,
        None,
        None,
        false,
        false,
    )
}

fn amount(h: &Holder, chem: &Chemistry, path: &str) -> f32 {
    h.amount(chem.id(path).unwrap())
}

#[test]
fn trans_to_moves_every_reagent_in_proportion() {
    let chem = chem();
    let mut from = beaker(50.0);
    add(&mut from, &chem, WATER, 30.0);
    add(&mut from, &chem, ETHANOL, 10.0);
    let mut to = beaker(50.0);
    let moved = from.trans_to(&chem, &mut to, 20.0, 1.0, None, false, false);
    assert_eq!(moved, 20.0);
    assert_eq!(
        (amount(&to, &chem, WATER), amount(&to, &chem, ETHANOL)),
        (15.0, 5.0)
    );
    assert_eq!(
        (amount(&from, &chem, WATER), amount(&from, &chem, ETHANOL)),
        (15.0, 5.0)
    );
    assert_eq!((from.total_volume, to.total_volume), (20.0, 20.0));
    // `target_id`: only that reagent.
    let moved = from.trans_to(&chem, &mut to, 10.0, 1.0, chem.id(ETHANOL), false, false);
    assert_eq!(moved, 5.0, "at most what there is");
    assert_eq!(amount(&from, &chem, ETHANOL), 0.0);
    assert!(
        from.has(chem.id(ETHANOL).unwrap()).is_none(),
        "an emptied reagent is removed"
    );
    // The temperature follows the heat capacities.
    let mut cold = beaker(100.0);
    let id = chem.id(WATER).unwrap();
    cold.add_reagent(&chem, id, 10.0, 200.0, None, None, false, false);
    assert_eq!(
        cold.chem_temp, 200.0,
        "an empty holder takes the reagent's temperature"
    );
    cold.add_reagent(&chem, id, 10.0, 300.0, None, None, false, false);
    assert_eq!(cold.chem_temp, 250.0);
}

#[test]
fn maximum_volume_caps_adds_and_transfers() {
    let chem = chem();
    let mut to = beaker(50.0);
    assert_eq!(add(&mut to, &chem, WATER, 45.0), 45.0);
    let mut from = beaker(50.0);
    add(&mut from, &chem, ETHANOL, 30.0);
    assert_eq!(
        from.trans_to(&chem, &mut to, 10.0, 1.0, None, false, false),
        5.0
    );
    assert_eq!(from.total_volume, 25.0);
    assert!(to.holder_full());
    assert_eq!(
        add(&mut to, &chem, WATER, 10.0),
        0.0,
        "a full holder takes nothing"
    );
    let mut small = beaker(15.0);
    assert_eq!(add(&mut small, &chem, WATER, 20.0), 15.0);
}

#[test]
fn update_total_rounds_like_tg() {
    let chem = chem();
    let mut h = beaker(100.0);
    assert_eq!(
        add(&mut h, &chem, WATER, 0.00004),
        0.0,
        "below the quantisation level"
    );
    assert_eq!(
        add(&mut h, &chem, WATER, 0.005),
        0.0,
        "a new reagent below 0.01"
    );
    add(&mut h, &chem, WATER, 10.00006);
    assert!((h.reagents[0].volume - 10.0001).abs() < 1e-5);
    assert_eq!(h.total_volume, 10.0, "the total is rounded to 0.01");
    h.reagents[0].volume = 0.004;
    h.update_total();
    assert!(
        h.reagents.is_empty(),
        "under 0.01 is removed when not reacting"
    );
    assert_eq!((h.total_volume, h.ph), (0.0, 7.0));
    add(&mut h, &chem, WATER, 1.0);
    assert_eq!(h.remove_all(0.25), 0.25);
    assert_eq!(h.total_volume, 0.75);
}

#[test]
fn instant_reactions_need_reagents_catalysts_and_temperature() {
    let chem = chem();
    let mut h = beaker(100.0);
    add(&mut h, &chem, A, 10.0);
    assert_eq!(amount(&h, &chem, C), 0.0);
    // A 2 + B 1 -> C 3: B is the limit.
    add(&mut h, &chem, B, 3.0);
    assert_eq!(amount(&h, &chem, A), 4.0);
    assert_eq!(amount(&h, &chem, B), 0.0);
    assert_eq!(amount(&h, &chem, C), 9.0);
    assert_eq!(h.messages, ["The solution begins to bubble."]);
    // B -> C needs 5 units of the catalyst present, which stays.
    let mut h = beaker(100.0);
    add(&mut h, &chem, B, 4.0);
    add(&mut h, &chem, CAT, 4.0);
    assert_eq!(amount(&h, &chem, B), 4.0, "not enough catalyst");
    add(&mut h, &chem, CAT, 1.0);
    assert_eq!((amount(&h, &chem, B), amount(&h, &chem, C)), (0.0, 4.0));
    assert_eq!(amount(&h, &chem, CAT), 5.0);
    // C + water -> A from 400 K only; heating starts it.
    let mut h = beaker(100.0);
    add(&mut h, &chem, WATER, 5.0);
    add(&mut h, &chem, C, 5.0);
    assert_eq!(amount(&h, &chem, A), 0.0, "300 K is too cold");
    while h.chem_temp < 400.0 {
        h.expose_temperature(&chem, 1000.0, 0.02);
    }
    assert_eq!(amount(&h, &chem, A), 5.0);
    // Inside a mob: no mix message.
    let mut mob = Holder::new(
        1000.0,
        flags::REAGENT_HOLDER_ALIVE,
        Owner {
            path: "/mob/living/carbon/human".into(),
            living: true,
        },
    );
    add(&mut mob, &chem, A, 2.0);
    add(&mut mob, &chem, B, 1.0);
    assert_eq!(amount(&mob, &chem, C), 3.0);
    assert!(mob.messages.is_empty());
}

#[test]
fn equilibrium_steps_at_the_reaction_rate() {
    let chem = chem();
    let mut h = beaker(100.0);
    add(&mut h, &chem, D, 10.0);
    // D + E -> 2 F at 300 K (above optimal 250: delta_t 1). The first step runs at once with
    // 1 s: 10 u/s / product ratio 2 = 5 per reagent.
    add(&mut h, &chem, E, 10.0);
    assert!(h.is_reacting);
    assert_eq!(amount(&h, &chem, F), 10.0);
    assert_eq!((amount(&h, &chem, D), amount(&h, &chem, E)), (5.0, 5.0));
    // Each SSreagents step of 0.25 s: 10 × 0.25 / 2 = 1.25 per reagent.
    h.process(&chem, 0.25);
    assert_eq!(amount(&h, &chem, D), 3.75);
    assert_eq!(amount(&h, &chem, F), 12.5);
    assert!(h.messages.is_empty());
    for _ in 0..10 {
        h.process(&chem, 0.25);
    }
    assert!(!h.is_reacting);
    assert_eq!(amount(&h, &chem, F), 20.0);
    assert_eq!(h.messages, ["The solution begins to bubble."]);
    assert_eq!(h.total_volume, 20.0);
}

#[test]
fn mix_color_blends_by_volume() {
    let chem = chem();
    let mut h = beaker(100.0);
    add(&mut h, &chem, A, 10.0);
    assert_eq!(mix_color(&chem, &h.reagents), Some([255, 0, 0, 255]));
    add(&mut h, &chem, WATER, 10.0);
    // Red without alpha, then water with alpha: tg swaps the colors, the weight stays 10/20.
    assert_eq!(mix_color(&chem, &h.reagents), Some([213, 85, 85, 187]));
}

#[derive(Default)]
struct TestBody {
    brute: f32,
    tox: f32,
    oxy: f32,
    losebreath: f32,
    drunk: f32,
    told: Vec<String>,
}

impl Body for TestBody {
    fn health(&self) -> f32 {
        100.0 - self.brute - self.tox - self.oxy
    }
    fn oxy_loss(&self) -> f32 {
        self.oxy
    }
    fn dead(&self) -> bool {
        false
    }
    fn adjust(&mut self, kind: Damage, amount: f32) {
        let v = match kind {
            Damage::Brute => &mut self.brute,
            Damage::Tox => &mut self.tox,
            Damage::Oxy => &mut self.oxy,
            Damage::Burn => return,
        };
        *v = (*v + amount).max(0.0);
    }
    fn heal_bodypart(&mut self, brute: f32, _burn: f32) {
        self.brute = (self.brute - brute).max(0.0);
    }
    fn losebreath(&mut self) -> &mut f32 {
        &mut self.losebreath
    }
    fn drunk(&mut self) -> &mut f32 {
        &mut self.drunk
    }
    fn tell(&mut self, text: String) {
        self.told.push(text);
    }
    fn spt_prob(&mut self, _: f32, _: f32) -> bool {
        false
    }
}

fn mob() -> Holder {
    Holder::new(
        1000.0,
        flags::REAGENT_HOLDER_ALIVE,
        Owner {
            path: "/mob/living/carbon/human".into(),
            living: true,
        },
    )
}

#[test]
fn metabolism_amounts_per_tick() {
    let chem = chem();
    let stomach = mob();
    // Libital: 0.1 u/s, 0.2 per 2 s tick, ratio 0.5: 3 × 1 × 0.5 × 2 = 3 brute healed.
    let mut body = TestBody {
        brute: 20.0,
        ..Default::default()
    };
    let mut blood = mob();
    let id = chem.id(LIBITAL).unwrap();
    blood.add_reagent(&chem, id, 5.0, 300.0, Some(0.75), None, false, false);
    metabolize(&chem, &mut blood, &stomach, &mut body, 310.15, 2.0);
    assert!((amount(&blood, &chem, LIBITAL) - 4.8).abs() < 1e-5);
    assert_eq!(body.brute, 17.0);
    // Toxin: 3 u or less is filtered by the liver (removed, no damage).
    let mut blood = mob();
    add(&mut blood, &chem, TOXIN, 3.0);
    metabolize(&chem, &mut blood, &stomach, &mut body, 310.15, 2.0);
    assert!((amount(&blood, &chem, TOXIN) - 2.6).abs() < 1e-5);
    assert_eq!(body.tox, 0.0);
    // Over it: 0.5 × 1.5 toxpwr × ratio 1 × 2 s = 1.5 toxin damage.
    add(&mut blood, &chem, TOXIN, 2.4);
    metabolize(&chem, &mut blood, &stomach, &mut body, 310.15, 2.0);
    assert_eq!(body.tox, 1.5);
    assert!((amount(&blood, &chem, TOXIN) - 4.6).abs() < 1e-5);
    // Ethanol: drunk += sqrt(volume) × boozepwr × 0.005 × ratio × 2.
    let mut blood = mob();
    add(&mut blood, &chem, ETHANOL, 16.0);
    metabolize(&chem, &mut blood, &stomach, &mut body, 310.15, 2.0);
    assert!(
        (body.drunk - 4.0 * 65.0 * 0.005 * 0.5 * 2.0).abs() < 1e-4,
        "{}",
        body.drunk
    );
    body.drunk = 90.0;
    inebriation_tick(&mut body);
    assert!((body.drunk - (90.0 - 0.01 - 3.6)).abs() < 1e-4);
    assert_eq!(body.tox, 2.5, "81+: 1 toxin per tick");
    // Water sobers: 0.5 × -0.25 × ratio 1 × 2.
    let mut blood = mob();
    add(&mut blood, &chem, WATER, 5.0);
    body.drunk = 1.0;
    metabolize(&chem, &mut blood, &stomach, &mut body, 310.15, 2.0);
    assert!((body.drunk - 0.75).abs() < 1e-5);
}

#[test]
fn the_liver_filters_and_suffers_toxins() {
    let chem = chem();
    let stomach = mob();
    let mut body = TestBody::default();
    // A half-dead liver filters half as much: 3 u of toxin are over its 1.5 u tolerance.
    let mut blood = mob();
    add(&mut blood, &chem, TOXIN, 3.0);
    let damage = metabolize_with(
        &chem,
        &mut blood,
        &stomach,
        &mut body,
        310.15,
        2.0,
        Liver::Working { health: 0.5 },
    );
    assert_eq!(body.tox, 1.5);
    // `(amount / 15) * toxpwr` a second: 3 / 15 × 1.5 × 2 s.
    assert!((damage - 0.6).abs() < 1e-5, "{damage}");
    // At most 2 a second however much there is.
    add(&mut blood, &chem, TOXIN, 200.0);
    let damage = metabolize_with(
        &chem,
        &mut blood,
        &stomach,
        &mut body,
        310.15,
        2.0,
        Liver::Working { health: 1.0 },
    );
    assert_eq!(damage, 4.0);
    // Without a liver nothing (but `self_consuming` reagents) metabolizes or hurts the liver.
    let mut blood = mob();
    add(&mut blood, &chem, LIBITAL, 5.0);
    let mut body = TestBody {
        brute: 20.0,
        ..Default::default()
    };
    let damage = metabolize_with(
        &chem,
        &mut blood,
        &stomach,
        &mut body,
        310.15,
        2.0,
        Liver::Liverless,
    );
    assert_eq!((damage, body.brute), (0.0, 20.0));
    assert!((amount(&blood, &chem, LIBITAL) - 5.0).abs() < 1e-5);
}

#[test]
fn stomach_drips_into_the_blood() {
    let chem = chem();
    let mut stomach = mob();
    let mut blood = mob();
    add(&mut stomach, &chem, WATER, 10.0);
    // (round(0.05 × 10, 0.05) + max(0.2, 0.25)) × 2 s = 1.5 units.
    digest(&chem, &mut stomach, &mut blood, 2.0);
    assert_eq!(amount(&blood, &chem, WATER), 1.5);
    assert_eq!(amount(&stomach, &chem, WATER), 8.5);
}

#[test]
fn cryoxadone_heals_only_a_cold_body() {
    let chem = chem();
    let stomach = mob();
    let mut body = TestBody {
        brute: 20.0,
        oxy: 20.0,
        ..Default::default()
    };
    let mut blood = mob();
    add(&mut blood, &chem, CRYOXADONE, 5.0);
    // A warm body: nothing healed, the rate becomes 0.2 × (0.00001 × 310.15² + 0.5).
    metabolize(&chem, &mut blood, &stomach, &mut body, 310.15, 2.0);
    assert_eq!((body.brute, body.oxy), (20.0, 20.0));
    let rate = blood.reagents[0].metabolization_rate;
    assert!((rate - 0.2 * (0.00001 * 310.15_f32.powi(2) + 0.5)).abs() < 1e-5);
    // At 100 K: power 2.7; the previous rate (0.292 u/s) gives ratio 1.46 over 2 s.
    let ratio = rate * 2.0 / (0.2 * 2.0);
    let power = cryoxadone_power(100.0, false);
    assert!((power - 2.7).abs() < 1e-5);
    metabolize(&chem, &mut blood, &stomach, &mut body, 100.0, 2.0);
    assert!((body.brute - (20.0 - 0.5 * power * ratio * 2.0)).abs() < 1e-3);
    assert!((body.oxy - (20.0 - 1.5 * power * ratio * 2.0)).abs() < 1e-3);
    assert_eq!(cryoxadone_power(100.0, true), power * 2.0);
}

#[test]
fn pyroxadone_needs_a_hot_body() {
    assert_eq!(pyroxadone_power(310.15, false), None);
    assert_eq!(pyroxadone_power(350.0, false), Some(2.0));
    assert_eq!(pyroxadone_power(400.0, true), Some(4.0));
    assert_eq!(pyroxadone_power(450.0, false), Some(3.0));
    assert_eq!(pyroxadone_power(500.0, false), Some(5.0));
}
