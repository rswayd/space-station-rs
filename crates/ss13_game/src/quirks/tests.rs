use super::*;

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|n| (*n).to_owned()).collect()
}

#[test]
fn every_quirk_has_one_definition_and_name() {
    for def in &QUIRKS {
        assert_eq!(def.quirk.def().path, def.path);
        assert_eq!(Quirk::by_name(def.name), Some(def.quirk));
    }
}

#[test]
fn filter_follows_points_blacklist_and_limits() {
    // 2 free points: Friendly (2) alone is fine, Jolly (4) is not.
    let ok = names(&["Friendly"]);
    assert_eq!(filter_invalid(&ok), ok);
    assert_eq!(filter_invalid(&names(&["Jolly"])), Vec::<String>::new());
    // Negative quirks pay for positive ones.
    let paid = names(&["Jolly", "Mute"]);
    assert_eq!(filter_invalid(&paid), paid);
    // Blacklisted: the later one is dropped.
    assert_eq!(
        filter_invalid(&names(&["Blind", "Nearsighted"])),
        names(&["Blind"])
    );
    // Unknown names are dropped.
    assert_eq!(filter_invalid(&names(&["Nonsense"])), Vec::<String>::new());
    // Over budget: positive quirks go from the first until the balance holds.
    assert_eq!(
        filter_invalid(&names(&["Friendly", "Jolly", "Light Drinker"])),
        names(&["Jolly", "Light Drinker"])
    );
}

#[test]
fn new_blacklists_hold() {
    assert_eq!(
        filter_invalid(&names(&["Mute", "Soft-Spoken"])),
        names(&["Mute"])
    );
    assert_eq!(
        filter_invalid(&names(&["Foreigner", "Common Second Language"])),
        names(&["Foreigner"])
    );
    assert_eq!(
        filter_invalid(&names(&["Ananas Affinity", "Ananas Aversion"])),
        names(&["Ananas Affinity"])
    );
}

#[test]
fn taste_quirks_change_the_tongue() {
    let tastes = (
        crate::food::LIKED_FOODTYPES,
        crate::food::DISLIKED_FOODTYPES,
        crate::food::TOXIC_FOODTYPES,
    );
    let mut world = World::new();
    let none = world.spawn_empty().id();
    assert_eq!(food_tastes(&world, none, tastes), tastes);
    let with = |world: &mut World, list: Vec<Quirk>| world.spawn(Quirks(list)).id();
    let vegetarian = with(&mut world, vec![Quirk::Vegetarian]);
    let (liked, disliked, _) = food_tastes(&world, vegetarian, tastes);
    assert_eq!(liked & MEAT, 0);
    assert_ne!(disliked & MEAT, 0);
    let deviant = with(&mut world, vec![Quirk::DeviantTastes]);
    assert_eq!(
        food_tastes(&world, deviant, tastes),
        (tastes.1, tastes.0, tastes.2)
    );
    // Ageusia tastes nothing but toxic food.
    let ageusia = with(&mut world, vec![Quirk::Ageusia, Quirk::PineappleLiker]);
    assert_eq!(food_tastes(&world, ageusia, tastes), (0, 0, tastes.2));
    let hater = with(&mut world, vec![Quirk::PineappleHater]);
    assert_ne!(food_tastes(&world, hater, tastes).1 & PINEAPPLE, 0);
}

#[test]
fn prefs_change_only_to_valid_sets() {
    let mut prefs = QuirkPrefs::default();
    assert_eq!(prefs.balance(), 2);
    assert!(!change_prefs(&mut prefs, "Jolly", true));
    assert!(change_prefs(&mut prefs, "Mute", true));
    assert!(change_prefs(&mut prefs, "Jolly", true));
    assert_eq!(prefs.balance(), 2);
    // Removing Mute would leave Jolly unpaid.
    assert!(!change_prefs(&mut prefs, "Mute", false));
    assert!(change_prefs(&mut prefs, "Jolly", false));
    assert!(!change_prefs(&mut prefs, "Jolly", false));
    assert_eq!(prefs.0, names(&["Mute"]));
}

#[test]
fn addict_quirks_max_their_addiction_and_take_it_back() {
    use crate::addictions::{Addiction, Addictions, MAX_ADDICTION_POINTS};
    let mut world = World::new();
    world.init_resource::<Time>();
    let mob = world.spawn_empty().id();
    assert!(add_quirk(&mut world, mob, Quirk::Smoker, false, true));
    assert!(add_quirk(&mut world, mob, Quirk::Alcoholic, false, true));
    let a = world.get::<Addictions>(mob).unwrap();
    assert_eq!(a.points(Addiction::Nicotine), MAX_ADDICTION_POINTS);
    assert_eq!(a.cycle(Addiction::Nicotine), Some(1.0));
    assert_eq!(a.cycle(Addiction::Alcohol), Some(1.0));
    let addicts = world.get::<AddictQuirks>(mob).unwrap();
    let smoker = addicts.0.iter().find(|s| s.quirk == Quirk::Smoker).unwrap();
    assert!(SMOKER_PACKS.iter().any(|(p, _)| *p == smoker.container));
    // A kicked habit comes back on the next 30 s process.
    crate::addictions::remove_points(&mut world, mob, Addiction::Nicotine, 900.0);
    assert_eq!(
        world
            .get::<Addictions>(mob)
            .unwrap()
            .cycle(Addiction::Nicotine),
        None
    );
    world.entity_mut(mob).insert(Stat::Conscious);
    process(&mut world, mob, Quirk::Smoker, 1.0);
    assert_eq!(
        world
            .get::<Addictions>(mob)
            .unwrap()
            .cycle(Addiction::Nicotine),
        Some(1.0)
    );
    assert!(remove_quirk(&mut world, mob, Quirk::Smoker));
    let a = world.get::<Addictions>(mob).unwrap();
    assert_eq!(a.points(Addiction::Nicotine), 0.0);
    assert_eq!(a.points(Addiction::Alcohol), 0.0);
}

#[test]
fn medicine_allergies_skip_the_blacklist() {
    let medicines = [
        "/datum/reagent/medicine",
        "/datum/reagent/medicine/epinephrine",
        "/datum/reagent/medicine/omnizine/godblood",
        "/datum/reagent/medicine/c2/libital",
        "/datum/reagent/medicine/c2/aiuri",
        "/datum/reagent/medicine/morphine",
        "/datum/reagent/medicine/spaceacillin",
        "/datum/reagent/medicine/inacusiate",
        "/datum/reagent/medicine/oculine",
        "/datum/reagent/medicine/mannitol",
        "/datum/reagent/water",
    ];
    let mut rng = CombatRng::default();
    let picked = pick_medicine_allergies(&medicines, &mut rng);
    assert_eq!(picked.len(), MEDICINE_ALLERGIES);
    for p in &picked {
        assert!(p.starts_with("/datum/reagent/medicine/"));
        assert!(!p.contains("epinephrine") && !p.contains("godblood"));
    }
    let mut unique = picked.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), picked.len());
}

#[test]
fn food_allergy_reacts_only_to_its_foodtype() {
    let mut world = World::new();
    world.init_resource::<Time>();
    let mob = world.spawn_empty().id();
    assert!(add_quirk(&mut world, mob, Quirk::FoodAllergy, false, true));
    let foods = world.get::<Allergies>(mob).unwrap().foods;
    assert_eq!(foods.count_ones(), 1);
    assert!(FOOD_ALLERGIES.iter().any(|(_, bit)| *bit == foods));
    assert!(!food_allergy_reaction(&mut world, mob, !foods));
    assert!(food_allergy_reaction(&mut world, mob, foods));
    assert!(remove_quirk(&mut world, mob, Quirk::FoodAllergy));
    assert!(!food_allergy_reaction(&mut world, mob, foods));
}

#[test]
fn anxiety_grows_with_company_and_bad_mood() {
    assert_eq!(anxiety_mod(None, 400.0, 0), 0.0);
    // Two people around: 12.5 each.
    assert_eq!(anxiety_mod(None, 400.0, 2), 25.0);
    // Very well fed: calm (the factor drops to 0 at 1000 nutrition).
    assert_eq!(anxiety_mod(None, 1000.0, 1), 0.0);
    assert!(filter_invalid(&names(&["Social Anxiety", "Mute"])) == names(&["Social Anxiety"]));
}

#[test]
fn anxious_mob_gets_the_chat_component() {
    let mut world = World::new();
    world.init_resource::<Time>();
    let mob = world.spawn(TilePos::new(1, 1, 0)).id();
    assert!(add_quirk(
        &mut world,
        mob,
        Quirk::SocialAnxiety,
        false,
        true
    ));
    assert_eq!(
        world.get::<ss13_chat::speech::Anxiety>(mob),
        Some(&ss13_chat::speech::Anxiety {
            mood_mod: 0.0,
            dumb_thing: true
        })
    );
    assert!(remove_quirk(&mut world, mob, Quirk::SocialAnxiety));
    assert!(world.get::<ss13_chat::speech::Anxiety>(mob).is_none());
}

#[test]
fn hobby_quirks_change_paint_heat_and_pick_one_fandom() {
    let mut world = World::new();
    world.init_resource::<Time>();
    let mob = world.spawn(Stat::Conscious).id();
    assert_eq!(tagger_multiplier(&world, mob), 1.0);
    assert!(add_quirk(&mut world, mob, Quirk::Tagger, false, true));
    assert!(add_quirk(&mut world, mob, Quirk::Pyromania, false, true));
    assert_eq!(tagger_multiplier(&world, mob), 0.5);
    assert_eq!(heat_multiplier(&world, mob), PYROMANIA_HEAT_MOD);
    // No fire around: the count stays 0 and no moodlet.
    process(&mut world, mob, Quirk::Pyromania, 1.0);
    assert_eq!(world.get::<LastFireSize>(mob), Some(&LastFireSize(0)));
    assert!(remove_quirk(&mut world, mob, Quirk::Pyromania));
    assert_eq!(heat_multiplier(&world, mob), 1.0);
    assert_eq!(
        filter_invalid(&names(&["Clown Enjoyer", "Mime Fan", "Mute"])),
        names(&["Clown Enjoyer", "Mute"])
    );
}

#[test]
fn quirk_items_are_runtime_types() {
    let registry = ss13_mapload::runtime::runtime_types();
    let missing: Vec<String> = item_paths()
        .into_iter()
        .filter(|p| !registry.contains(p.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "not in ss13_mapload::runtime: {missing:?}"
    );
}
