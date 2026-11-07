use super::*;

#[test]
fn rune_table_follows_tg() {
    let names: Vec<&str> = RuneKind::ALL.iter().map(|k| k.def().cultist_name).collect();
    assert_eq!(
        names,
        [
            "Offer",
            "Empower",
            "Teleport",
            "Nar'Sie",
            "Revive",
            "Barrier",
            "Summon Cultist"
        ]
    );
    for kind in RuneKind::ALL {
        assert_eq!(RuneKind::from_name(kind.def().cultist_name), Some(kind));
        assert_eq!(RuneKind::from_path(kind.def().path), Some(kind));
    }
    let narsie = RuneKind::NarSie.def();
    assert_eq!(
        (
            narsie.req_cultists,
            narsie.scribe_delay,
            narsie.scribe_damage
        ),
        (9, 500.0, 40.1)
    );
    assert_eq!(narsie.erase_time, 50.0);
    let summon = RuneKind::Summon.def();
    assert_eq!((summon.req_cultists, summon.invoke_damage), (2, 10.0));
    let offer = RuneKind::Offer.def();
    assert_eq!(
        (offer.req_cultists, offer.scribe_delay, offer.scribe_damage),
        (1, 40.0, 0.1)
    );
    assert!(RuneKind::Teleport.def().req_keyword);
    assert!(!RuneKind::Offer.def().req_keyword);
}

#[test]
fn offer_needs_two_to_convert_and_three_for_the_linked() {
    assert_eq!(offer_invokers_needed(OfferTarget::Convert), 2);
    assert_eq!(offer_invokers_needed(OfferTarget::LinkedSacrifice), 3);
    assert_eq!(offer_invokers_needed(OfferTarget::Sacrifice), 1);
}

#[test]
fn revives_cost_three_sacrifices_each() {
    assert!(!can_revive_dead(2, 0));
    assert!(can_revive_dead(3, 0));
    assert!(!can_revive_dead(5, 3));
    assert!(can_revive_dead(6, 3));
}

#[test]
fn blood_magic_limits_and_costs() {
    assert_eq!(spell_limit(false), 1);
    assert_eq!(spell_limit(true), 4);
    assert_eq!(carving(false), (100.0, 40.0));
    assert_eq!(carving(true), (40.0, 8.0));
    assert_eq!(Spell::Shackles.def().charges, 4);
    assert_eq!(Spell::Stun.def().health_cost, 10.0);
    assert_eq!(Spell::Teleport.def().health_cost, 7.0);
    assert_eq!(Spell::Construction.def().health_cost, 12.0);
    for spell in Spell::ALL {
        assert_eq!(Spell::from_name(spell.def().name), Some(spell));
    }
}

#[test]
fn cult_stages_follow_bandastation_ratios() {
    // Low pop: 20% risen, 30% ascendent of the other living players.
    assert_eq!(cult_stage(1, 10), (false, false));
    assert_eq!(cult_stage(2, 10), (true, false));
    assert_eq!(cult_stage(3, 10), (true, true));
    // High pop (70+): 10% and 20%.
    assert_eq!(cult_stage(7, 70), (true, false));
    assert_eq!(cult_stage(14, 70), (true, true));
    assert_eq!(cult_stage(6, 70), (false, false));
    // Nobody else alive.
    assert_eq!(cult_stage(1, 0), (true, true));
    assert_eq!(stun_coef(false, false), 1.0);
    assert_eq!(stun_coef(true, false), 0.4);
    assert_eq!(stun_coef(true, true), 0.1);
}

#[test]
fn holy_water_deconverts_after_a_minute() {
    assert_eq!(holy_water_stage(10.0), (false, false));
    assert_eq!(holy_water_stage(26.0), (true, false));
    assert_eq!(holy_water_stage(60.0), (true, true));
}

#[test]
fn objectives_explain_and_complete() {
    let mut team = CultTeam {
        objectives: vec![
            CultObjective::Sacrifice {
                target: Some(Entity::PLACEHOLDER),
                name: "Bob".into(),
                job: "Ассистент".into(),
                sacced: false,
            },
            CultObjective::Eldergod {
                spots: vec![
                    (Entity::PLACEHOLDER, "Bridge".into()),
                    (Entity::PLACEHOLDER, "Bar".into()),
                    (Entity::PLACEHOLDER, "Chapel".into()),
                ],
                summoned: false,
            },
        ],
        ..default()
    };
    assert!(
        team.objectives[0]
            .explanation()
            .starts_with("Sacrifice Bob, Ассистент,")
    );
    assert!(
        team.objectives[1]
            .explanation()
            .contains("Bridge, Bar and Chapel")
    );
    assert!(team.is_sacrifice_target(Entity::PLACEHOLDER));
    assert!(!team.sacrifice_done() && !team.victory());
    if let CultObjective::Sacrifice { sacced, .. } = &mut team.objectives[0] {
        *sacced = true;
    }
    assert!(team.sacrifice_done() && !team.victory());
    assert!(!team.is_sacrifice_target(Entity::PLACEHOLDER));
    if let CultObjective::Eldergod { summoned, .. } = &mut team.objectives[1] {
        *summoned = true;
    }
    assert!(team.victory());
}
