use super::*;

#[test]
fn blood_accounting_respects_the_victim_limit() {
    let mut v = Vampire::default();
    let (a, b) = (
        Entity::from_raw_u32(1).unwrap(),
        Entity::from_raw_u32(2).unwrap(),
    );
    // A full bite: min(20, 25) * 0.5 = 10.
    assert_eq!(v.adjust_blood(Some(a), 10.0), 10.0);
    assert_eq!((v.bloodtotal, v.bloodusable), (10.0, 10.0));
    // 200 per victim: the rest of the limit, then nothing.
    assert_eq!(v.adjust_blood(Some(a), 500.0), 190.0);
    assert_eq!(v.adjust_blood(Some(a), 10.0), 0.0);
    assert_eq!(v.drained_from(a), 200.0);
    assert_eq!(v.adjust_blood(Some(b), 10.0), 10.0);
    assert_eq!(v.drained.len(), 2);
    // Spending never goes below 0 or above the total.
    v.subtract_usable(205.0);
    assert_eq!((v.bloodtotal, v.bloodusable), (210.0, 5.0));
    v.subtract_usable(10.0);
    assert_eq!(v.bloodusable, 0.0);
    v.subtract_usable(-1000.0);
    assert_eq!(v.bloodusable, 210.0);
    // Without a victim (admin edits) there is no limit.
    assert_eq!(v.adjust_blood(None, 1000.0), 1000.0);
}

#[test]
fn thresholds_unlock_the_fork_powers() {
    let mut v = Vampire::default();
    assert_eq!(v.pending_powers(), [Power::Rejuvenate, Power::Glare]);
    v.powers = vec![Power::Rejuvenate, Power::Glare];
    v.bloodtotal = 99.0;
    assert!(v.pending_powers().is_empty());
    v.bloodtotal = 150.0;
    assert_eq!(
        v.pending_powers(),
        [Power::Vision, Power::UnlockSpecialization, Power::Lair]
    );
    v.powers.extend(v.pending_powers());
    // A spent tier never comes back.
    v.powers.retain(|p| *p != Power::Lair);
    v.spent.push(Power::Lair);
    v.bloodtotal = 500.0;
    assert_eq!(v.pending_powers(), [Power::Regen, Power::VisionAdvanced]);
    v.powers.extend(v.pending_powers());
    // The specialization's powers by blood; full power needs 8 victims and 1000 blood.
    v.subclass = Some(Subclass::Gargantua);
    assert_eq!(v.pending_powers(), [Power::BloodRush]);
    v.powers.push(Power::BloodRush);
    v.bloodtotal = 1000.0;
    v.drained = (1..=7)
        .map(|i| (Entity::from_raw_u32(i).unwrap(), 200.0))
        .collect();
    assert!(v.pending_powers().is_empty());
    v.drained.push((Entity::from_raw_u32(8).unwrap(), 10.0));
    assert_eq!(v.pending_powers(), [Power::Full, Power::VisionFull]);
}

fn with_spell(spell: Spell, usable: f32) -> Vampire {
    Vampire {
        bloodtotal: usable,
        bloodusable: usable,
        spells: vec![SpellState {
            spell,
            ready_at: 0.0,
            charges: GLARE_CHARGES,
            recharges: Vec::new(),
        }],
        ..default()
    }
}

#[test]
fn spell_cost_and_cooldown_rules() {
    let ctx = CastContext {
        now: 1.0,
        conscious: true,
        ..default()
    };
    let mut v = with_spell(Spell::BloodRush, 30.0);
    assert_eq!(v.cast_check(Spell::BloodRush, ctx), Ok(()));
    assert_eq!(v.cast_check(Spell::Cloak, ctx), Err(CastFail::Missing));
    // Nullification raises the cost: 30 * 1.5 = 45.
    v.adjust_nullification(50.0, 4.0);
    assert_eq!(v.nullified, 50.0);
    assert_eq!(v.blood_cost(30), 45);
    assert!(matches!(
        v.cast_check(Spell::BloodRush, ctx),
        Err(CastFail::Message(m)) if m.contains("45")
    ));
    // At 100 nullification nothing works, until full power.
    v.bloodusable = 100.0;
    v.bloodtotal = 100.0;
    v.adjust_nullification(100.0, 0.0);
    assert!(v.cast_check(Spell::BloodRush, ctx).is_err());
    v.powers.push(Power::Full);
    v.nullified = 0.0;
    assert_eq!(v.cast_check(Spell::BloodRush, ctx), Ok(()));
    // The chapel blocks a vampire without full power; dead blocks everyone; blood rush
    // needs a conscious caster, rejuvenate does not.
    v.powers.clear();
    let chapel = CastContext {
        in_chapel: true,
        ..ctx
    };
    assert!(v.cast_check(Spell::BloodRush, chapel).is_err());
    let unconscious = CastContext {
        conscious: false,
        ..ctx
    };
    assert_eq!(
        v.cast_check(Spell::BloodRush, unconscious),
        Err(CastFail::Unconscious)
    );
    let r = with_spell(Spell::Rejuvenate, 0.0);
    assert_eq!(r.cast_check(Spell::Rejuvenate, unconscious), Ok(()));
    let dead = CastContext { dead: true, ..ctx };
    assert!(r.cast_check(Spell::Rejuvenate, dead).is_err());
    // The cooldown.
    v.spells[0].ready_at = 31.0;
    assert_eq!(v.cast_check(Spell::BloodRush, ctx), Err(CastFail::Cooldown));
    assert_eq!(Spell::BloodRush.def().cooldown, 30.0);
    assert_eq!(Spell::Rejuvenate.def().cooldown, 20.0);
}

#[test]
fn glare_has_two_charges_recharging_in_30_seconds() {
    let ctx = |now| CastContext {
        now,
        conscious: true,
        ..default()
    };
    let mut v = with_spell(Spell::Glare, 0.0);
    v.spend_glare_charge(10.0);
    assert_eq!(v.spell(Spell::Glare).unwrap().charges, 1);
    v.spend_glare_charge(12.0);
    // Out of charges: cooling down until the first recharge.
    assert_eq!(v.spell(Spell::Glare).unwrap().ready_at, 40.0);
    assert_eq!(
        v.cast_check(Spell::Glare, ctx(39.0)),
        Err(CastFail::Cooldown)
    );
    assert!(!v.recharge_glare(39.0));
    assert!(v.recharge_glare(40.0));
    assert_eq!(v.spell(Spell::Glare).unwrap().charges, 1);
    assert_eq!(v.cast_check(Spell::Glare, ctx(40.0)), Ok(()));
    assert!(v.recharge_glare(42.0));
    assert_eq!(v.spell(Spell::Glare).unwrap().charges, 2);
}

#[test]
fn glare_deviation_follows_the_facing() {
    let at = TilePos::new(5, 5, 1);
    // Facing north: a victim in front (north, north-east) gets the full glare.
    assert_eq!(glare_deviation(at, Dir::North, TilePos::new(5, 6, 1)), 0);
    assert_eq!(glare_deviation(at, Dir::North, TilePos::new(6, 6, 1)), 0);
    assert_eq!(glare_deviation(at, Dir::North, TilePos::new(6, 5, 1)), 1);
    assert_eq!(glare_deviation(at, Dir::North, TilePos::new(5, 4, 1)), 2);
    assert_eq!(glare_deviation(at, Dir::North, at), 0);
}

#[test]
fn info_panel_lists_every_specialization() {
    let umbrae = Subclass::Umbrae.ui();
    assert_eq!(umbrae.id, "umbrae");
    assert_eq!(umbrae.powers[0].name.as_deref(), Some("Покров тьмы"));
    assert_eq!(umbrae.powers[0].blood_required, Some(150));
    assert_eq!(Subclass::Gargantua.ui().powers.len(), 7);
    assert_eq!(Subclass::Dantalion.ui().full_powers.len(), 4);
    assert_eq!(Subclass::from_id("hemomancer"), Some(Subclass::Hemomancer));
    assert_eq!(Subclass::from_id("ancient"), None);
    assert_eq!(blood_text(12.5), "12.5");
    assert_eq!(blood_text(10.0), "10");
}

#[test]
fn hemomancer_and_ancient_powers() {
    let mut v = Vampire {
        subclass: Some(Subclass::Hemomancer),
        bloodtotal: 600.0,
        powers: vec![
            Power::Rejuvenate,
            Power::Glare,
            Power::Vision,
            Power::Lair,
            Power::Regen,
            Power::VisionAdvanced,
        ],
        spent: vec![Power::UnlockSpecialization],
        ..default()
    };
    assert_eq!(
        v.pending_powers(),
        [Power::BloodTendrils, Power::PredatorSenses]
    );
    v.bloodtotal = 800.0;
    assert!(v.pending_powers().contains(&Power::BloodEruption));
    // The ancient has every power at once, whatever the blood and victims.
    v.subclass = Some(Subclass::Ancient);
    v.bloodtotal = 0.0;
    let pending = v.pending_powers();
    for power in [
        Power::Cloak,
        Power::BloodRush,
        Power::SwitchPlaces,
        Power::BloodEruption,
        Power::Full,
        Power::VisionXray,
    ] {
        assert!(pending.contains(&power), "{power:?}");
    }
    assert!(Subclass::Ancient.improved_rejuv_healing());
    assert_eq!(
        Subclass::from_id("ancient"),
        None,
        "never picked in the panel"
    );
}

#[test]
fn tendrils_slow_for_six_seconds_once() {
    let mut world = World::new();
    world.insert_resource(Time::<()>::default());
    let victim = world.spawn(MoveSpeedModifier(0.25)).id();
    tendril_slow(&mut world, victim, 6.0);
    tendril_slow(&mut world, victim, 7.0);
    assert_eq!(world.get::<MoveSpeedModifier>(victim).unwrap().0, 0.75);
    assert_eq!(world.get::<TendrilSlow>(victim).unwrap().until, 7.0);
}
