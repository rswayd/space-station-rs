use super::*;

fn ctx() -> UseContext {
    UseContext {
        human: true,
        ..default()
    }
}

#[test]
fn chemicals_regenerate_per_life_tick() {
    let mut c = Changeling::default();
    assert_eq!(c.chem_charges, 20.0);
    // One `Life()` (2 s) at 1 per second.
    c.regenerate_chemicals(2.0, false, false);
    assert_eq!(c.chem_charges, 22.0);
    // Capped at the storage.
    c.regenerate_chemicals(100.0, false, false);
    assert_eq!(c.chem_charges, 75.0);
    // Dead: only up to half the storage, and a full storage is cut down to it.
    c.chem_charges = 30.0;
    c.regenerate_chemicals(20.0, true, false);
    assert_eq!(c.chem_charges, 37.5);
    // Burning: a quarter speed, slowdowns do not apply.
    c.chem_charges = 0.0;
    c.chem_recharge_slowdown = ARMOR_SLOWDOWN;
    c.regenerate_chemicals(2.0, false, true);
    assert_eq!(c.chem_charges, 0.5);
    // The chitin's slowdown.
    c.regenerate_chemicals(2.0, false, false);
    assert_eq!(c.chem_charges, 0.5 + 2.0 * (1.0 - ARMOR_SLOWDOWN));
    c.adjust_chemicals(-1000.0, None);
    assert_eq!(c.chem_charges, 0.0);
}

#[test]
fn purchase_and_readapt_rules() {
    let mut c = Changeling::default();
    assert_eq!(c.check_purchase(Power::ArmBlade), Ok(2));
    assert!(
        c.check_purchase(Power::AbsorbDna).is_err(),
        "innate powers are not sold"
    );
    c.genetic_points -= 2;
    c.purchased.push(Power::ArmBlade);
    assert_eq!(
        c.check_purchase(Power::ArmBlade),
        Err("Мы уже развили эту способность!")
    );
    for power in [Power::Adrenaline, Power::Fleshmend, Power::Armor] {
        let cost = c.check_purchase(power).unwrap();
        c.genetic_points -= cost;
        c.purchased.push(power);
    }
    // 10 - 2 - 2 - 2 - 1.
    assert_eq!(c.genetic_points, 3);
    assert_eq!(c.spent_points(), 10);
    c.genetic_points = 1;
    c.purchased.retain(|p| *p != Power::Fleshmend);
    assert_eq!(
        c.check_purchase(Power::Fleshmend),
        Err("Мы достигли предела своих возможностей!")
    );
    // Stasis blocks purchases.
    c.genetic_points = 10;
    c.stasis = Stasis::Waiting {
        ready_at: 1.0,
        died: false,
    };
    assert!(c.check_purchase(Power::Fleshmend).is_err());
    c.stasis = Stasis::None;
    // Readapt refunds everything and resets the rates.
    c.chem_recharge_slowdown = ARMOR_SLOWDOWN;
    c.chem_charges = 100.0;
    let gone = c.remove_purchased();
    assert_eq!(gone.len(), 3);
    assert!(c.purchased.is_empty());
    assert_eq!(c.genetic_points, GENETIC_POINTS);
    assert_eq!((c.chem_recharge_slowdown, c.chem_charges), (0.0, 75.0));
    // The emporium lists the powers by name with their costs, the first four as before the stings.
    let names: Vec<(&str, Option<u32>)> = Power::ALL
        .into_iter()
        .filter(|p| !p.innate())
        .map(|p| (p.def().name, p.def().dna_cost))
        .collect();
    assert_eq!(
        names[..4],
        [
            ("Gene Stim", Some(2)),
            ("Fleshmend", Some(2)),
            ("Arm Blade", Some(2)),
            ("Chitinous Armor", Some(1)),
        ]
    );
    assert_eq!(
        Power::from_path("/datum/action/changeling/weapon/arm_blade"),
        Some(Power::ArmBlade)
    );
    assert_eq!(Power::from_path("/datum/action/changeling/headcrab"), None);
}

#[test]
fn use_checks_follow_can_sting() {
    let mut c = Changeling {
        absorbed_count: 1,
        ..default()
    };
    assert_eq!(c.check_use(Power::Transform, ctx()), Ok(()));
    // Fake death needs 15; 20 is there.
    assert_eq!(c.check_use(Power::Fakedeath, ctx()), Ok(()));
    c.chem_charges = 10.0;
    assert_eq!(
        c.check_use(Power::Fakedeath, ctx()),
        Err("требуется 15 химикатов!".to_owned())
    );
    // A bought power must be owned.
    c.chem_charges = 75.0;
    assert_eq!(c.check_use(Power::ArmBlade, ctx()), Err(String::new()));
    c.purchased.push(Power::ArmBlade);
    assert_eq!(c.check_use(Power::ArmBlade, ctx()), Ok(()));
    // `STABLE` powers need a conscious changeling; fleshmend works in hard crit, stasis
    // in death.
    let crit = UseContext {
        stat: Stat::HardCrit,
        knocked_out: true,
        ..ctx()
    };
    assert_eq!(
        c.check_use(Power::ArmBlade, crit),
        Err("недееспособны!".to_owned())
    );
    c.purchased.push(Power::Fleshmend);
    assert_eq!(c.check_use(Power::Fleshmend, crit), Ok(()));
    let dead = UseContext {
        stat: Stat::Dead,
        ..crit
    };
    assert_eq!(c.check_use(Power::Fakedeath, dead), Ok(()));
    // In stasis only the stasis button works.
    c.stasis = Stasis::Ready { died: false };
    assert_eq!(c.check_use(Power::Fakedeath, dead), Ok(()));
    assert_eq!(
        c.check_use(Power::Fleshmend, dead),
        Err("притворяемся мёртвым!".to_owned())
    );
    // Burning blocks most powers.
    c.stasis = Stasis::None;
    let burning = UseContext {
        on_fire: true,
        ..ctx()
    };
    assert_eq!(
        c.check_use(Power::ArmBlade, burning),
        Err("on fire!".to_owned())
    );
    // A fallen changeling has nothing.
    c.fallen = true;
    assert!(c.check_use(Power::Transform, ctx()).is_err());
}

#[test]
fn absorb_takes_three_fifteen_second_stages() {
    let mut stage = 0;
    let mut stages = Vec::new();
    while let AbsorbStep::Stage(n) = absorb_next(stage) {
        stages.push(n);
        stage = n;
    }
    assert_eq!(stages, [1, 2, 3]);
    let total: f32 = stages.iter().map(|_| ABSORB_STAGE_DS / 10.0).sum();
    assert_eq!(total, 45.0);
}

#[test]
fn profiles_dedupe_and_push_out_the_oldest() {
    let mut c = Changeling::default();
    let profile = |i: usize| Profile {
        name: format!("crew {i}"),
        look: Some(HumanLook::random_assistant(i as u64)),
        protected: false,
    };
    for i in 0..=DNA_MAX {
        c.add_profile(profile(i));
    }
    assert_eq!(c.profiles.len(), DNA_MAX + 1);
    assert!(c.has_profile("crew 3", profile(3).look.as_ref()));
    // The same name with another body is other DNA.
    assert!(!c.has_profile("crew 3", profile(4).look.as_ref()));
    // Over the limit the oldest goes; the count keeps every gain.
    c.add_profile(profile(DNA_MAX + 1));
    assert_eq!(c.profiles.len(), DNA_MAX + 1);
    assert_eq!(c.profiles[0].name, "crew 1");
    assert_eq!(c.absorbed_count, DNA_MAX as u32 + 2);
}

#[test]
fn stings_are_bought_in_the_stings_category() {
    let stings: Vec<Power> = Power::ALL.into_iter().filter(|p| p.is_sting()).collect();
    assert_eq!(stings.len(), 7);
    for power in stings {
        assert_eq!(power.def().category, "stings");
        assert!(!power.innate());
    }
    // Extract DNA is free to evolve.
    assert_eq!(Power::ExtractDnaSting.def().dna_cost, Some(0));
    let mut c = Changeling {
        chosen_sting: Some(Power::MuteSting),
        ..default()
    };
    c.remove_purchased();
    assert_eq!(c.chosen_sting, None);
}

#[test]
fn the_transformation_sting_wears_off_but_not_while_dead() {
    let mut world = World::new();
    world.init_resource::<Time>();
    world.init_resource::<StingTimers>();
    let stung = |name: &str| TransStung {
        remaining: 1.0,
        original: Profile {
            name: name.to_owned(),
            look: None,
            protected: false,
        },
    };
    let alive = world
        .spawn((Name::new("Ling"), Stat::Conscious, stung("Alice")))
        .id();
    let dead = world
        .spawn((Name::new("Ling"), Stat::Dead, stung("Bob")))
        .id();
    world
        .resource_mut::<Time>()
        .advance_by(std::time::Duration::from_secs(2));
    sting_timers(&mut world);
    assert_eq!(world.get::<Name>(alive).unwrap().as_str(), "Alice");
    assert!(world.get::<TransStung>(alive).is_none());
    assert_eq!(world.get::<Name>(dead).unwrap().as_str(), "Ling");
    assert_eq!(world.get::<TransStung>(dead).unwrap().remaining, 1.0);
}

#[test]
fn augmented_eyesight_trades_flash_protection_for_xray() {
    let mut world = World::new();
    let mut ling = Changeling::default();
    let mob = world.spawn(ling.clone()).id();
    assert_eq!(ling_flash_protect(&world, mob), 0);
    ling.purchased.push(Power::AugmentedEyesight);
    world.entity_mut(mob).insert(ling.clone());
    assert_eq!(ling_flash_protect(&world, mob), 2);
    assert!(!ling_xray(&world, mob));
    ling.xray = true;
    world.entity_mut(mob).insert(ling.clone());
    assert_eq!(ling_flash_protect(&world, mob), -1);
    assert!(ling_xray(&world, mob));
    // Readapting takes it all back.
    ling.remove_purchased();
    world.entity_mut(mob).insert(ling);
    assert_eq!(ling_flash_protect(&world, mob), 0);
    assert!(!ling_xray(&world, mob));
}
