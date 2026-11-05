//! Diseases on a bare world: infection and immunity, cures by antibodies in injected blood,
//! skin contact through a touch, advanced diseases pushing out weaker ones, stage progression
//! over `Life()` ticks and the analyzer's lines.

use bevy::prelude::*;
use ss13_game::combat::CombatRng;
use ss13_game::health::{Health, Stat};
use ss13_game::medical::diseases::{
    self, BloodDiseases, Disease, Diseases, Kind, SPREAD_NON_CONTAGIOUS,
};
use ss13_game::medical::organs::OrganHost;
use ss13_game::medical::symptoms::SymptomKind;
use ss13_game::medical::wounds::Limbs;

fn world() -> World {
    let mut world = World::new();
    world.insert_resource(Time::<()>::default());
    world.insert_resource(CombatRng(42));
    world
}

fn human(world: &mut World) -> Entity {
    world
        .spawn((Health::default(), Stat::Conscious, Limbs::default()))
        .id()
}

#[test]
fn contracting_and_curing_leave_immunity() {
    let mut world = world();
    let mob = human(&mut world);
    let cold = Disease::new(Kind::Cold);
    assert!(diseases::force_contract(&mut world, mob, &cold));
    assert!(diseases::has_disease(&world, mob, &cold));
    assert!(
        !diseases::force_contract(&mut world, mob, &cold),
        "already sick"
    );
    diseases::cure(&mut world, mob, 0, true);
    assert!(diseases::of_mob(&world, mob).is_empty());
    assert!(!diseases::force_contract(&mut world, mob, &cold), "immune");
    // "Buy one, get one free": the cold's antibodies cover ICE9.
    assert!(!diseases::force_contract(
        &mut world,
        mob,
        &Disease::new(Kind::Cold9)
    ));
    assert!(diseases::force_contract(
        &mut world,
        mob,
        &Disease::new(Kind::Flu)
    ));
}

#[test]
fn only_living_humans_with_the_organ_catch_it() {
    let mut world = world();
    let dead = human(&mut world);
    world.entity_mut(dead).insert(Stat::Dead);
    assert!(!diseases::force_contract(
        &mut world,
        dead,
        &Disease::new(Kind::Flu)
    ));
    // Decloning acts on the dead (`process_dead`).
    assert!(diseases::force_contract(
        &mut world,
        dead,
        &Disease::new(Kind::Decloning)
    ));
    // Organs modelled, no lungs: no cold.
    let lungless = human(&mut world);
    world.entity_mut(lungless).insert(OrganHost);
    assert!(!diseases::force_contract(
        &mut world,
        lungless,
        &Disease::new(Kind::Cold)
    ));
    // Not a human.
    let thing = world.spawn((Health::default(), Stat::Conscious)).id();
    assert!(!diseases::force_contract(
        &mut world,
        thing,
        &Disease::new(Kind::Gbs)
    ));
}

#[test]
fn blood_carries_viruses_and_antibodies() {
    let mut world = world();
    let mob = human(&mut world);
    let syringe = world
        .spawn(BloodDiseases {
            viruses: vec![Disease::new(Kind::Flu), Disease::new(Kind::AdrenalCrisis)],
            resistances: Vec::new(),
        })
        .id();
    diseases::blood_exposed(&mut world, syringe, mob, false);
    assert!(diseases::has_disease(&world, mob, &Disease::new(Kind::Flu)));
    assert_eq!(
        Disease::new(Kind::AdrenalCrisis).spread_flags,
        SPREAD_NON_CONTAGIOUS
    );
    assert!(
        !diseases::has_disease(&world, mob, &Disease::new(Kind::AdrenalCrisis)),
        "not contagious"
    );
    // The syringe holds no blood any more (no chemistry here): its data is gone.
    assert!(world.get::<BloodDiseases>(syringe).is_none());
    // A survivor's blood: the flu antibodies cure it, without immunity.
    let vaccine = world
        .spawn(BloodDiseases {
            viruses: Vec::new(),
            resistances: vec![Disease::new(Kind::Flu).id()],
        })
        .id();
    diseases::blood_exposed(&mut world, vaccine, mob, false);
    assert!(diseases::of_mob(&world, mob).is_empty());
    assert!(diseases::force_contract(
        &mut world,
        mob,
        &Disease::new(Kind::Flu)
    ));
}

#[test]
fn skin_contact_spreads_on_touch() {
    let mut world = world();
    let sick = human(&mut world);
    let other = human(&mut world);
    let gbs = Disease::new(Kind::Gbs);
    assert!(diseases::force_contract(&mut world, sick, &gbs));
    for _ in 0..50 {
        diseases::touch(&mut world, other, sick);
        if diseases::has_disease(&world, other, &gbs) {
            break;
        }
    }
    assert!(diseases::has_disease(&world, other, &gbs));
    // Anaphylaxis is not contagious.
    let third = human(&mut world);
    assert!(diseases::force_contract(
        &mut world,
        sick,
        &Disease::new(Kind::Anaphylaxis)
    ));
    for _ in 0..50 {
        diseases::touch(&mut world, third, sick);
    }
    assert!(!diseases::has_disease(
        &world,
        third,
        &Disease::new(Kind::Anaphylaxis)
    ));
}

#[test]
fn stronger_advanced_disease_pushes_out_the_weaker() {
    let mut world = world();
    let mob = human(&mut world);
    // Resistance 3, transmission 4.
    let sneezy = Disease::advanced("Cold", &[SymptomKind::Sneeze]);
    assert!(diseases::force_contract(&mut world, mob, &sneezy));
    // Transmission 2 is not above resistance 3: refused.
    let weak = Disease::advanced("Flu", &[SymptomKind::Cough]);
    assert!(!diseases::force_contract(&mut world, mob, &weak));
    assert!(diseases::has_disease(&world, mob, &sneezy));
    // Transmission 4 + 3 = 7 beats it; the loser leaves no immunity.
    let strong = Disease::advanced(
        "Strong",
        &[SymptomKind::Sneeze, SymptomKind::ViralEvolution],
    );
    assert!(diseases::force_contract(&mut world, mob, &strong));
    assert_eq!(diseases::of_mob(&world, mob).len(), 1);
    assert!(diseases::has_disease(&world, mob, &strong));
    let all = world.get::<Diseases>(mob).unwrap();
    assert!(all.resistances.is_empty());
}

#[test]
fn life_ticks_climb_to_the_peak() {
    let mut world = world();
    let mob = human(&mut world);
    // Brainrot bypasses natural immunity: it only climbs.
    assert!(diseases::force_contract(
        &mut world,
        mob,
        &Disease::new(Kind::Brainrot)
    ));
    for _ in 0..3000 {
        diseases::life(&mut world, mob, 2.0);
    }
    let d = &diseases::of_mob(&world, mob)[0];
    assert_eq!(d.stage, d.max_stages);
}

#[test]
fn analyzer_lists_visible_diseases_only() {
    let mut world = world();
    let mob = human(&mut world);
    assert!(diseases::force_contract(
        &mut world,
        mob,
        &Disease::new(Kind::DeathSandwich)
    ));
    assert!(
        diseases::scan_lines(&world, mob).is_empty(),
        "hidden from scanners"
    );
    assert!(diseases::force_contract(
        &mut world,
        mob,
        &Disease::new(Kind::Flu)
    ));
    let lines: Vec<String> = diseases::scan_lines(&world, mob)
        .into_iter()
        .map(|(_, text)| text)
        .collect();
    assert_eq!(lines[0], "Внимание: Virus обнаружена");
    assert_eq!(lines[1], "Название: The Flu.");
    assert!(lines.iter().any(|l| l == "Stage: 1/3."));
    assert!(lines.iter().any(|l| l.contains("Spaceacillin")));
    // An admin heal cures every non-positive disease.
    diseases::cure_negative(&mut world, mob);
    assert!(diseases::of_mob(&world, mob).is_empty());
}

#[test]
fn event_api_builds_every_kind() {
    for kind in Kind::ALL {
        let d = Disease::new(kind);
        assert_eq!(Kind::from_path(kind.def().path), Some(kind));
        assert!(d.max_stages > 0);
    }
    let mut rng = CombatRng(9);
    let sample = Disease::random_advance(&mut rng, Some(3), 8);
    assert_eq!(sample.advance.as_ref().unwrap().symptoms.len(), 3);
}
