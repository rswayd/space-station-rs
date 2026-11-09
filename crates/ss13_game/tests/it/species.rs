//! Species (`ss13_game::species`) on a bare world: the species table, `set_species` swapping
//! organs, damage rules, blood and look, the species' breathing, pressure and temperature
//! rules, limb sprites, equipment and mutation limits.

use bevy::prelude::*;
use ss13_atmos::{GasId, GasMixture};
use ss13_game::breath::{AlertCategory, Alerts, BodyLife, LifeCtx, Respiration, Worn};
use ss13_game::combat::CombatRng;
use ss13_game::health::{DamageType, Health, Stat, update_stat};
use ss13_game::human::{HumanLook, parts};
use ss13_game::medical::blood::{Blood, BloodType};
use ss13_game::medical::organs::{self, Organ, OrganHost, OrganOf, Slot};
use ss13_game::species::{self, SPECIES, Species, SpeciesId, Trait};
use ss13_game::zone::BodyZone;

/// A human body with the human organs (bare organ entities), blood and a look.
fn human(world: &mut World) -> Entity {
    world.init_resource::<Time>();
    let mob = world
        .spawn((
            Health::default(),
            Stat::Conscious,
            OrganHost,
            Blood::new(BloodType::OPlus),
            HumanLook::random_assistant(1),
        ))
        .id();
    for slot in Slot::ALL {
        // Not every slot is a human organ: the zombie tumour is infection-only.
        let Some(path) = SpeciesId::Human.def().organs.get(slot) else {
            continue;
        };
        let organ = organs::spawn_organ(world, path).unwrap();
        world.entity_mut(organ).insert(OrganOf(mob));
    }
    mob
}

fn organ_path(world: &World, mob: Entity, slot: Slot) -> Option<String> {
    organs::state(world, mob, slot).map(|o: &Organ| o.path.clone())
}

#[test]
fn the_species_table_matches_tg() {
    for (i, def) in SPECIES.iter().enumerate() {
        assert_eq!(def.id as usize, i, "{} out of order", def.key);
        assert_eq!(SpeciesId::from_key(def.key), Some(def.id));
        for slot in Slot::ALL {
            if let Some(path) = def.organs.get(slot) {
                assert_eq!(Slot::of_path(path), Some(slot), "{}: {path}", def.key);
            }
        }
        if let Some(blood) = def.blood {
            assert!(
                BloodType::from_path(blood).is_some(),
                "{}: {blood}",
                def.key
            );
        }
    }
    // `ROUNDSTART_RACES` of `game_options.txt` and BandaStation's `bandastation_config.txt`.
    let roundstart: Vec<&str> = SpeciesId::roundstart().map(|s| s.def().key).collect();
    assert_eq!(
        roundstart,
        [
            "human",
            "felinid",
            "cerulean",
            "lizard",
            "moth",
            "ethereal",
            "plasmaman",
            "skrell",
            "tajaran",
            "vulpkanin"
        ]
    );
    // A few tg numbers.
    let lizard = SpeciesId::Lizard.def();
    assert_eq!((lizard.coldmod, lizard.heatmod), (1.5, 0.67));
    assert_eq!(SpeciesId::Skrell.def().payday_modifier, 1.5);
    assert_eq!(SpeciesId::Vulpkanin.def().payday_modifier, 0.8);
    assert_eq!(SpeciesId::Tajaran.def().coldmod, 0.7);
    assert_eq!(SpeciesId::Ethereal.def().siemens_coeff, 0.5);
    assert!(species::organ_traits("/obj/item/organ/lungs/plasmaman").contains(&Trait::NoHunger));
}

#[test]
fn set_species_swaps_organs_blood_and_body() {
    let mut world = World::new();
    let mob = human(&mut world);
    assert_eq!(species::species_of(&world, mob), SpeciesId::Human);

    // Plasmaman: no heart or appendix, plasma lungs, no blood and no cardiac arrest for it.
    species::set_species(&mut world, mob, SpeciesId::Plasmaman);
    assert_eq!(
        world.get::<Species>(mob),
        Some(&Species(SpeciesId::Plasmaman))
    );
    assert!(organs::organ(&world, mob, Slot::Heart).is_none());
    assert!(organs::organ(&world, mob, Slot::Appendix).is_none());
    assert_eq!(
        organ_path(&world, mob, Slot::Lungs).as_deref(),
        Some("/obj/item/organ/lungs/plasmaman")
    );
    assert!(species::has_trait(&world, mob, Trait::NoBlood));
    assert!(
        species::has_trait(&world, mob, Trait::NoHunger),
        "from the lungs"
    );
    assert!(!organs::cardiac_arrest(&world, mob));
    assert_eq!(world.get::<Stat>(mob), Some(&Stat::Conscious));
    let look = world.get::<HumanLook>(mob).unwrap();
    assert_eq!(look.species, SpeciesId::Plasmaman);
    assert!(!look.female, "plasmamen have no sexes");

    // Lizard: a heart again, the lizard's own brain and blood.
    species::set_species(&mut world, mob, SpeciesId::Lizard);
    assert_eq!(
        organ_path(&world, mob, Slot::Heart).as_deref(),
        Some("/obj/item/organ/heart")
    );
    assert_eq!(
        organ_path(&world, mob, Slot::Brain).as_deref(),
        Some("/obj/item/organ/brain/lizard")
    );
    assert_eq!(
        world.get::<Blood>(mob).unwrap().blood_type,
        BloodType::Lizard
    );
    assert!(!species::has_trait(&world, mob, Trait::NoHunger));

    // Back to human: a human blood type and human organs.
    species::set_species(&mut world, mob, SpeciesId::Human);
    let blood = world.get::<Blood>(mob).unwrap().blood_type;
    assert!(BloodType::HUMAN.contains(&blood));
    assert_eq!(
        organ_path(&world, mob, Slot::Brain).as_deref(),
        Some("/obj/item/organ/brain")
    );
    // Missing organs grow back as the species' (`regenerate_organs`).
    species::set_species(&mut world, mob, SpeciesId::Skrell);
    let liver = organs::organ(&world, mob, Slot::Liver).unwrap();
    organs::remove(&mut world, liver, true);
    organs::regenerate(&mut world, mob);
    assert_eq!(
        organ_path(&world, mob, Slot::Liver).as_deref(),
        Some("/obj/item/organ/liver/skrell")
    );
}

#[test]
fn species_change_damage() {
    let mut world = World::new();
    let mob = human(&mut world);
    let hurt = |world: &mut World, kind, amount| {
        let mut health = world.get::<Health>(mob).unwrap().clone();
        health.damage_zone(BodyZone::Chest, kind, amount);
        world.entity_mut(mob).insert(health);
    };

    // Golem limbs: half brute and burn.
    species::set_species(&mut world, mob, SpeciesId::Golem);
    hurt(&mut world, DamageType::Brute, 10.0);
    hurt(&mut world, DamageType::Burn, 10.0);
    let h = world.get::<Health>(mob).unwrap();
    assert_eq!((h.brute, h.burn), (5.0, 5.0));
    // `TRAIT_NOSOFTCRIT`: conscious down to hard crit.
    let mut low = h.clone();
    low.adjust(DamageType::Oxy, 105.0);
    let mut stat = Stat::Conscious;
    update_stat(&low, &mut stat);
    assert_eq!(stat, Stat::Conscious);
    low.adjust(DamageType::Oxy, 30.0);
    update_stat(&low, &mut stat);
    assert_eq!(stat, Stat::HardCrit);

    // Jellypeople heal from toxins.
    species::set_species(&mut world, mob, SpeciesId::Jelly);
    let mut h = world.get::<Health>(mob).unwrap().healed();
    h.adjust(DamageType::Tox, 10.0);
    assert_eq!(h.tox, 0.0);
    h.tox = 20.0;
    h.adjust(DamageType::Tox, 10.0);
    assert_eq!(h.tox, 10.0);
    h.adjust(DamageType::Tox, -5.0);
    assert_eq!(h.tox, 15.0, "healing toxins hurts them");

    // Skeletons are immune to toxin damage, not to its healing.
    species::set_species(&mut world, mob, SpeciesId::Skeleton);
    let mut h = world.get::<Health>(mob).unwrap().healed();
    h.adjust(DamageType::Tox, 10.0);
    assert_eq!(h.tox, 0.0);
    h.tox = 10.0;
    h.adjust(DamageType::Tox, -4.0);
    assert_eq!(h.tox, 6.0);

    // Moths take 15% more.
    species::set_species(&mut world, mob, SpeciesId::Moth);
    let mut h = world.get::<Health>(mob).unwrap().healed();
    h.damage_zone(BodyZone::Chest, DamageType::Brute, 10.0);
    assert!((h.brute - 11.5).abs() < 1e-4);
}

/// One `Life()` step's context over `health` and `alerts`.
fn ctx<'a>(health: &'a mut Health, alerts: &'a mut Alerts, rng: &'a mut CombatRng) -> LifeCtx<'a> {
    LifeCtx {
        health,
        stat: Stat::Conscious,
        alerts,
        rng,
        now: 0.0,
        events: Vec::new(),
    }
}

fn body_of(id: SpeciesId) -> BodyLife {
    let mut world = World::new();
    let mob = human(&mut world);
    species::set_species(&mut world, mob, id);
    BodyLife::of(&world, mob)
}

#[test]
fn pressure_and_temperature_follow_the_species() {
    let mut rng = CombatRng(7);
    // Vacuum: a human takes brute and the alert, a silverscale neither.
    for (id, hurt) in [(SpeciesId::Human, true), (SpeciesId::Silverscale, false)] {
        let mut resp = Respiration {
            body: body_of(id),
            ..Respiration::default()
        };
        let (mut health, mut alerts) = (Health::default(), Alerts::default());
        resp.environment_pressure(0.0, 2.0, &mut ctx(&mut health, &mut alerts, &mut rng));
        assert_eq!(health.brute > 0.0, hurt, "{id:?}");
        assert_eq!(
            alerts.get(AlertCategory::Pressure).is_some(),
            hurt,
            "{id:?}"
        );
    }
    // A 250 K core: below the human cold limit (270.15 K), above the vulpkanin's (210.15 K).
    for (id, hurt) in [(SpeciesId::Human, true), (SpeciesId::Vulpkanin, false)] {
        let mut resp = Respiration {
            body: body_of(id),
            bodytemperature: 250.0,
            coretemperature: 250.0,
            ..Respiration::default()
        };
        let (mut health, mut alerts) = (Health::default(), Alerts::default());
        resp.body_temperature(
            250.0,
            &Worn::default(),
            true,
            None,
            2.0,
            &mut ctx(&mut health, &mut alerts, &mut rng),
        );
        assert_eq!(health.burn > 0.0, hurt, "{id:?}");
    }
    // Lizards are cold-blooded: the core does not warm itself up.
    let mut resp = Respiration {
        body: body_of(SpeciesId::Lizard),
        bodytemperature: 280.0,
        coretemperature: 280.0,
        ..Respiration::default()
    };
    let (mut health, mut alerts) = (Health::default(), Alerts::default());
    resp.body_temperature(
        280.0,
        &Worn::default(),
        true,
        None,
        2.0,
        &mut ctx(&mut health, &mut alerts, &mut rng),
    );
    assert!(resp.coretemperature <= 280.0 + 1e-9);
    // Lizard cold damage is 1.5 times a tajaran's at the same core (both under their limit).
    let burn_at = |id: SpeciesId, rng: &mut CombatRng| {
        let mut resp = Respiration {
            body: body_of(id),
            bodytemperature: 150.0,
            coretemperature: 150.0,
            ..Respiration::default()
        };
        let (mut health, mut alerts) = (Health::default(), Alerts::default());
        resp.body_temperature(
            150.0,
            &Worn::default(),
            false,
            None,
            2.0,
            &mut ctx(&mut health, &mut alerts, rng),
        );
        health.burn
    };
    let (lizard, tajaran) = (
        burn_at(SpeciesId::Lizard, &mut rng),
        burn_at(SpeciesId::Tajaran, &mut rng),
    );
    assert!(lizard > tajaran, "{lizard} vs {tajaran}");
}

/// A breath of `moles` of one gas at room temperature.
fn breath_of(gas: GasId, moles: f64) -> GasMixture {
    let mut breath = GasMixture::vacuum();
    breath.temperature = 293.15;
    breath.moles[usize::from(gas.0)] = moles;
    breath
}

#[test]
fn plasmamen_breathe_plasma() {
    let mut rng = CombatRng(3);
    let mut resp = Respiration {
        body: body_of(SpeciesId::Plasmaman),
        ..Respiration::default()
    };
    // Air without plasma suffocates them.
    let (mut health, mut alerts) = (Health::default(), Alerts::default());
    let mut air = breath_of(GasId::O2, 0.05);
    assert!(!resp.check_breath(Some(&mut air), &mut ctx(&mut health, &mut alerts, &mut rng)));
    assert!(health.oxy > 0.0);
    assert!(alerts.get(AlertCategory::NotEnoughPlasma).is_some());
    assert!(alerts.get(AlertCategory::NotEnoughOxygen).is_none());
    // Plasma is breathed (into CO2) without toxin damage.
    let mut plasma = breath_of(GasId::PLASMA, 0.05);
    assert!(resp.check_breath(
        Some(&mut plasma),
        &mut ctx(&mut health, &mut alerts, &mut rng)
    ));
    assert_eq!(health.tox, 0.0);
    assert!(alerts.get(AlertCategory::NotEnoughPlasma).is_none());
    assert!(plasma.moles[usize::from(GasId::CO2.0)] > 0.0);
    // The same plasma poisons a human.
    let mut human = Respiration::default();
    let (mut health, mut alerts) = (Health::default(), Alerts::default());
    let mut plasma = breath_of(GasId::PLASMA, 0.05);
    human.check_breath(
        Some(&mut plasma),
        &mut ctx(&mut health, &mut alerts, &mut rng),
    );
    assert!(health.tox > 0.0);
}

#[test]
fn limbs_are_drawn_from_the_species_sprites() {
    let look = |species: SpeciesId| HumanLook {
        species,
        female: false,
        mutant_color: [1, 2, 3],
        ..HumanLook::random_assistant(5)
    };
    let states = |species| -> Vec<(String, String, Option<[u8; 3]>)> {
        parts(&look(species))
            .unwrap()
            .into_iter()
            .map(|p| (p.dmi.to_owned(), p.state, p.color))
            .collect()
    };
    let lizard = states(SpeciesId::Lizard);
    let dmi = "icons/mob/human/species/lizard/bodyparts.dmi";
    for state in [
        "lizard_head",
        "lizard_chest_m",
        "lizard_l_hand",
        "lizard_r_leg",
    ] {
        assert!(
            lizard
                .iter()
                .any(|(d, s, c)| d == dmi && s == state && *c == Some([1, 2, 3])),
            "{state} in {lizard:?}"
        );
    }
    // Lizard heads draw no hair.
    assert!(!lizard.iter().any(|(d, ..)| d.ends_with("human_face.dmi")));
    // Tajaran legs are digitigrade.
    assert!(
        states(SpeciesId::Tajaran)
            .iter()
            .any(|(_, s, _)| s == "digitigrade_l_leg")
    );
    // Golems have no hands; plasmamen are not tinted.
    assert!(
        !states(SpeciesId::Golem)
            .iter()
            .any(|(_, s, _)| s.ends_with("_hand"))
    );
    assert!(
        states(SpeciesId::Plasmaman)
            .iter()
            .filter(|(d, ..)| d.contains("plasmaman"))
            .all(|(.., c)| c.is_none())
    );
    // Moths have their own eyes.
    assert!(
        states(SpeciesId::Moth)
            .iter()
            .any(|(_, s, _)| s == "motheyes_l")
    );
}

#[test]
fn equipment_and_mutations_follow_the_species() {
    let mut world = World::new();
    let mob = human(&mut world);
    // `ITEM_SLOT_FEET`, `ITEM_SLOT_ICLOTHING`.
    let (feet, uniform) = (1 << 7, 1 << 1);
    assert!(species::can_wear(&world, mob, feet));
    assert!(species::can_mutate(&world, mob));
    species::set_species(&mut world, mob, SpeciesId::Golem);
    assert!(!species::can_wear(&world, mob, feet));
    assert!(!species::can_wear(&world, mob, uniform));
    assert!(!species::can_mutate(&world, mob));
    species::set_species(&mut world, mob, SpeciesId::Monkey);
    assert!(species::can_wear(&world, mob, uniform));
    assert!(!species::can_wear(&world, mob, feet));
    species::set_species(&mut world, mob, SpeciesId::Plasmaman);
    assert!(!species::can_mutate(&world, mob), "mineral and geneless");
}
