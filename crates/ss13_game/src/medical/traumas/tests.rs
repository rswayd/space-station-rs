use super::*;

fn trauma(kind: Kind, resilience: Resilience) -> Trauma {
    Trauma {
        kind,
        resilience,
        possessed: Vec::new(),
        steal_at: 0.0,
        suppressed: false,
        phrase: String::new(),
        obsession: None,
        extra: Extra::default(),
    }
}

#[test]
fn gains_follow_types_and_tier_limits() {
    let have = vec![
        trauma(Kind::Stuttering, Resilience::Basic),
        trauma(Kind::Dumbness, Resilience::Basic),
        trauma(Kind::Paralysis(Paralysis::Legs), Resilience::Surgery),
    ];
    // One of a type: paralysis of any kind counts as the same.
    assert!(!can_gain(&have, Kind::Stuttering, None, false));
    assert!(!can_gain(
        &have,
        Kind::Paralysis(Paralysis::LArm),
        None,
        false
    ));
    // Two basic of three: a natural one more fits, then the tier is full.
    assert!(can_gain(&have, Kind::Concussion, None, true));
    let mut full = have.clone();
    full.push(trauma(Kind::Concussion, Resilience::Basic));
    assert!(!can_gain(&full, Kind::Healthy, None, true));
    // Given ones ignore the limit; another tier counts on its own.
    assert!(can_gain(&full, Kind::Healthy, None, false));
    assert!(can_gain(
        &full,
        Kind::Healthy,
        Some(Resilience::Lobotomy),
        true
    ));
}

#[test]
fn random_severe_gains_come_as_often_as_in_tg() {
    // tg has 13 randomly gainable severe types; paralysis is one of them.
    let severe = TG_POOL.iter().filter(|e| e.2 == Category::Severe).count();
    assert_eq!(severe, 13);
    let mut world = World::new();
    let brain = world.spawn_empty().id();
    let mut rng = CombatRng(11);
    let (rolls, mut paralysis, mut none) = (13_000, 0, 0);
    for _ in 0..rolls {
        match gain_type(&mut world, brain, Category::Severe, None, true, &mut rng) {
            Some(Kind::Paralysis(_)) => paralysis += 1,
            None => none += 1,
            Some(_) => {}
        }
        world.entity_mut(brain).remove::<Traumas>();
    }
    // 1/13 each (1000 expected); nothing from the 2 of 13 unported (discoordination,
    // hypnotic stupor). A bare brain takes a split personality (it needs a player only in a
    // body).
    assert!((850..1150).contains(&paralysis), "{paralysis}");
    assert!((1_700..2_300).contains(&none), "{none}");
}

#[test]
fn categories_and_scan_words() {
    // The ported kinds sit in the pool under their own type, category and resilience.
    for (path, kind, category, resilience) in TG_POOL {
        if let Some(kind) = kind {
            let def = kind.def();
            assert_eq!(
                (def.path, def.category, def.resilience),
                (path, category, resilience)
            );
        }
    }
    assert_eq!(Resilience::Lobotomy.scan_word(), "Deep-rooted");
    assert!(Resilience::Basic < Resilience::Surgery);
    assert_eq!(
        Kind::Paralysis(Paralysis::Left).gain_text().1,
        "Вы больше не чувствуете левую сторону вашего тела!"
    );
}
