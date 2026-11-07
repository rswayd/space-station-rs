use super::*;

fn hit(kind: DamageType, damage: f32, sharpness: u32) -> Hit {
    Hit {
        kind,
        damage,
        sharpness,
        wound_bonus: 0.0,
        exposed_wound_bonus: 0.0,
        part_damage_before: 0.0,
    }
}

#[test]
fn setting_a_bone_spares_a_wound_that_worsened() {
    let mut world = World::new();
    let mut limbs = Limbs::default();
    apply_wound(limbs.limb_mut(BodyZone::LArm), WoundKind::Compound);
    let patient = world.spawn(limbs).id();
    let mob = world.spawn_empty().id();
    let actor = crate::tools::Actor {
        client: mob,
        mob,
        item: None,
    };
    // Started on a dislocation, which became a compound fracture during the do_after.
    finish_treatment(
        &mut world,
        actor,
        patient,
        BodyZone::LArm,
        Treat::SetBone,
        WoundKind::Dislocation,
    );
    let wounds = &world
        .get::<Limbs>(patient)
        .unwrap()
        .limb(BodyZone::LArm)
        .wounds;
    assert_eq!(wounds.len(), 1);
    assert_eq!(wounds[0].kind, WoundKind::Compound);
}

#[test]
fn mangled_limbs_come_off_only_when_dismemberable() {
    // Skin (avulsion) and bone (hairline fracture) mangled: a 30 slash at an arm at half
    // damage rolls 30 + 25 = 55% (no critical blunt bonus); some seeds take it off.
    let mut mangled = Limb::default();
    apply_wound(&mut mangled, WoundKind::SlashCritical);
    apply_wound(&mut mangled, WoundKind::Hairline);
    let h = Hit {
        part_damage_before: 25.0,
        ..hit(DamageType::Brute, 30.0, SHARP_EDGED)
    };
    let mut off = 0;
    for seed in 0..200 {
        let mut rng = CombatRng(seed);
        let mut limb = mangled.clone();
        let out = receive_damage(&mut rng, &mut limb, BodyZone::LArm, h, false, true, true);
        if let Some((wounding, outright)) = out.dismember {
            assert_eq!(wounding, WoundType::Slash);
            assert!(!outright);
            assert_eq!(out.gained, None, "a limb that comes off takes no new wound");
            off += 1;
        }
        let mut rng = CombatRng(seed);
        let mut limb = mangled.clone();
        let out = receive_damage(&mut rng, &mut limb, BodyZone::LArm, h, false, true, false);
        assert_eq!(out.dismember, None, "not dismemberable");
    }
    assert!((60..160).contains(&off), "about 55% of 200: {off}");
    // Below `DISMEMBER_MINIMUM_DAMAGE` nothing comes off; an intact limb never does.
    for seed in 0..50 {
        let small = Hit { damage: 9.0, ..h };
        let out = receive_damage(
            &mut CombatRng(seed),
            &mut mangled.clone(),
            BodyZone::LArm,
            small,
            false,
            true,
            true,
        );
        assert_eq!(out.dismember, None);
        let out = receive_damage(
            &mut CombatRng(seed),
            &mut Limb::default(),
            BodyZone::LArm,
            h,
            false,
            true,
            true,
        );
        assert_eq!(out.dismember, None);
    }
    // A robotic limb gets no flesh wounds at all.
    let robotic = Limb {
        robotic: true,
        ..Limb::default()
    };
    let out = receive_damage(
        &mut CombatRng(3),
        &mut robotic.clone(),
        BodyZone::LArm,
        hit(DamageType::Brute, 25.0, SHARP_EDGED),
        false,
        true,
        true,
    );
    assert_eq!(out, HitOutcome::default());
}

#[test]
fn a_huge_wound_roll_takes_a_damaged_limb_off_outright() {
    // `wound_bonus` 200 beats the 150 threshold; a limb at full damage always goes.
    let h = Hit {
        wound_bonus: 200.0,
        part_damage_before: BodyZone::RLeg.max_damage(),
        ..hit(DamageType::Brute, 20.0, 0)
    };
    let limb = Limb::default();
    let rolled = roll_wounding(
        &mut CombatRng(5),
        &limb,
        BodyZone::RLeg,
        WoundType::Blunt,
        20.0,
        h,
        true,
    );
    assert_eq!(rolled, Some(Rolled::Dismember));
    let rolled = roll_wounding(
        &mut CombatRng(5),
        &limb,
        BodyZone::RLeg,
        WoundType::Blunt,
        20.0,
        h,
        false,
    );
    assert!(matches!(rolled, Some(Rolled::Wound(_))));
}

#[test]
fn wounding_type_follows_sharpness_and_mangling() {
    let limb = Limb::default();
    assert_eq!(
        wounding_of(hit(DamageType::Brute, 10.0, 0), &limb),
        Some((WoundType::Blunt, 10.0))
    );
    assert_eq!(
        wounding_of(hit(DamageType::Brute, 10.0, SHARP_EDGED), &limb),
        Some((WoundType::Slash, 10.0))
    );
    assert_eq!(
        wounding_of(hit(DamageType::Brute, 10.0, SHARP_POINTY), &limb),
        Some((WoundType::Pierce, 10.0))
    );
    assert_eq!(
        wounding_of(hit(DamageType::Burn, 10.0, SHARP_EDGED), &limb),
        Some((WoundType::Burn, 10.0))
    );
    assert_eq!(wounding_of(hit(DamageType::Tox, 10.0, 0), &limb), None);
    // A critical cut exposes the bone: slashes become blunt at 60%.
    let mut mangled = Limb::default();
    apply_wound(&mut mangled, WoundKind::SlashCritical);
    let (kind, damage) = wounding_of(hit(DamageType::Brute, 10.0, SHARP_EDGED), &mangled).unwrap();
    assert_eq!(kind, WoundType::Blunt);
    assert!((damage - 6.0).abs() < 1e-5);
}

#[test]
fn roll_modifiers_follow_check_woundings_mods() {
    let limb = Limb::default();
    let mut h = hit(DamageType::Brute, 10.0, SHARP_EDGED);
    h.wound_bonus = 10.0;
    h.exposed_wound_bonus = 15.0;
    // An arm has no resistance: the exposed bonus applies.
    assert_eq!(woundings_mods(&limb, BodyZone::LArm, 20.0, h), 45.0);
    // The chest's `wound_resistance` 10 counts as armor: no exposed bonus, 90%.
    assert!((woundings_mods(&limb, BodyZone::Chest, 20.0, h) - 27.0).abs() < 1e-4);
    // A maxed arm adds `disabled_wound_penalty`; existing wounds their penalty.
    let mut hurt = Limb::default();
    apply_wound(&mut hurt, WoundKind::Dislocation);
    h.part_damage_before = 50.0;
    assert_eq!(
        woundings_mods(&hurt, BodyZone::LArm, 20.0, h),
        20.0 + 10.0 + 5.0 + 15.0 + 15.0
    );
}

#[test]
fn wound_thresholds_and_seeded_chances() {
    // A scalpel (10 force, +10, +15 exposed) on an arm: roll 1..25 + 25.
    let mut h = hit(DamageType::Brute, 10.0, SHARP_EDGED);
    h.wound_bonus = 10.0;
    h.exposed_wound_bonus = 15.0;
    let mut rng = CombatRng(11);
    let mut counts = std::collections::HashMap::new();
    for _ in 0..10_000 {
        let limb = Limb::default();
        let got = check_wounding(&mut rng, &limb, BodyZone::LArm, WoundType::Slash, 10.0, h);
        *counts.entry(got).or_insert(0u32) += 1;
    }
    // 10^1.4 = 25.1 -> rand(1, 25) + 25: always >= 20 (abrasion), >= 50 (laceration) only at 25.
    assert_eq!(counts.get(&None), None, "{counts:?}");
    let severe = counts
        .get(&Some(WoundKind::SlashSevere))
        .copied()
        .unwrap_or(0);
    assert!((250..550).contains(&severe), "1 in 25: {counts:?}");
    // The same hit to the chest: (roll + 10) * 0.9, abrasion from roll 13 on.
    let mut moderate = 0;
    for _ in 0..10_000 {
        let limb = Limb::default();
        if check_wounding(&mut rng, &limb, BodyZone::Chest, WoundType::Slash, 10.0, h).is_some() {
            moderate += 1;
        }
    }
    assert!(
        (4800..5600).contains(&moderate),
        "13 of 25 rolls: {moderate}"
    );
    // A punch (7 damage, no bonus) rolls at most 15: never wounds.
    let punch = hit(DamageType::Brute, 7.0, 0);
    for _ in 0..1000 {
        let limb = Limb::default();
        assert_eq!(
            check_wounding(
                &mut rng,
                &limb,
                BodyZone::LArm,
                WoundType::Blunt,
                7.0,
                punch
            ),
            None
        );
    }
    // Damage over 25 is capped: 25^1.4 = 90.6 -> 91.
    let big = hit(DamageType::Brute, 100.0, 0);
    let mut compound = 0;
    for _ in 0..2000 {
        let limb = Limb::default();
        if check_wounding(
            &mut rng,
            &limb,
            BodyZone::LLeg,
            WoundType::Blunt,
            100.0,
            big,
        ) == Some(WoundKind::Compound)
        {
            compound += 1;
        }
    }
    assert_eq!(compound, 0, "115 is out of reach of a single fresh hit");
}

#[test]
fn worse_wounds_replace_milder_ones_and_bleeding_clots() {
    let mut limb = Limb::default();
    apply_wound(&mut limb, WoundKind::SlashModerate);
    assert_eq!(limb.wounds[0].blood_flow, 1.75);
    apply_wound(&mut limb, WoundKind::SlashSevere);
    assert_eq!(limb.wounds.len(), 1);
    assert_eq!(limb.wounds[0].blood_flow, 2.75);
    assert!(limb.has_state(state::SKIN_CUT | state::VESSELS_UNCLAMPED));
    // An arm's bleeding: the wound's flow, the vessels counted once (0.375 - 1.5).
    assert!((limb.bleed_rate(BodyZone::LArm, false) - (2.75 + 0.375 - 1.5)).abs() < 1e-5);
    // Clotting 0.02/s takes it below 2 after 37.5 s: demoted to a moderate cut.
    let mut rng = CombatRng(1);
    let ctx = TickCtx {
        secs: 2.0,
        lying: false,
        bodytemperature: BODYTEMP_NORMAL,
        nutrition: 400.0,
        burn_damage: 0.0,
        has_blood: true,
    };
    for _ in 0..19 {
        process(&mut rng, &mut limb, BodyZone::LArm, ctx);
    }
    assert_eq!(limb.wounds[0].kind, WoundKind::SlashModerate);
    assert!(limb.surgery == 0, "the severe cut's states left with it");
    // Gauze soaks 0.125/s more: the cut closes.
    limb.gauze = Some(Gauze {
        name: "марля".into(),
        absorption_capacity: 5.0,
        absorption_rate: 0.125,
        splint_factor: 0.7,
        burn_cleanliness_bonus: 0.35,
    });
    let mut ticks = 0;
    while !limb.wounds.is_empty() {
        process(&mut rng, &mut limb, BodyZone::LArm, ctx);
        ticks += 1;
        assert!(ticks < 20);
    }
    assert!(limb.gauze.as_ref().unwrap().absorption_capacity < 5.0);
}

#[test]
fn splints_and_disabled_legs_limit_walking() {
    let mut limbs = Limbs::default();
    let health = Health::default();
    apply_wound(limbs.limb_mut(BodyZone::LLeg), WoundKind::Compound);
    assert!(limbs.limb(BodyZone::LLeg).wounds[0].disabling);
    // One leg out (3 ds) and its limp 7 × 70% on every other step.
    let l = limbs.limits(&health);
    assert!((l.slowdown - (3.0 + 7.0 * 0.7 / 2.0)).abs() < 1e-4, "{l:?}");
    assert!(!l.floored);
    apply_wound(limbs.limb_mut(BodyZone::RLeg), WoundKind::Compound);
    assert!(limbs.limits(&health).floored);
}
