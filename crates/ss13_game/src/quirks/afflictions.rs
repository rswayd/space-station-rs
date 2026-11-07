use super::*;

/// Extreme Medicine Allergy's pick: `MEDICINE_ALLERGIES` of the `/datum/reagent/medicine`
/// subtypes outside the blacklist.
pub fn pick_medicine_allergies(medicines: &[&str], rng: &mut CombatRng) -> Vec<String> {
    use ss13_mapload::fidelity::is_under;
    let mut pool: Vec<&str> = medicines
        .iter()
        .copied()
        .filter(|p| is_under(p, "/datum/reagent/medicine") && *p != "/datum/reagent/medicine")
        .filter(|p| !ALLERGY_BLACKLIST.iter().any(|b| is_under(p, b)))
        .collect();
    let mut out = Vec::new();
    while out.len() < MEDICINE_ALLERGIES && !pool.is_empty() {
        let i = rng.range(0, pool.len() as u32 - 1) as usize;
        out.push(pool.swap_remove(i).to_owned());
    }
    out
}

/// `/datum/quirk/item_quirk/allergic/add` and `food_allergic/add`: the allergens.
// shortcut: no Food Allergy preference (always "Random").
pub(super) fn allergy_add(world: &mut World, mob: Entity, quirk: Quirk) {
    let medicines: Vec<String> = if quirk == Quirk::MedicineAllergy {
        let paths: Vec<String> = world
            .get_resource::<crate::chem::Chem>()
            .map(|c| c.0.reagents.iter().map(|r| r.path.clone()).collect())
            .unwrap_or_default();
        let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
        let mut rng = world.get_resource_or_init::<CombatRng>();
        pick_medicine_allergies(&paths, &mut rng)
    } else {
        Vec::new()
    };
    let food = if quirk == Quirk::FoodAllergy {
        let mut rng = world.get_resource_or_init::<CombatRng>();
        FOOD_ALLERGIES[rng.range(0, FOOD_ALLERGIES.len() as u32 - 1) as usize].1
    } else {
        0
    };
    if world.get::<Allergies>(mob).is_none() {
        world.entity_mut(mob).insert(Allergies::default());
    }
    let mut allergies = world.get_mut::<Allergies>(mob).expect("inserted above");
    if quirk == Quirk::MedicineAllergy {
        allergies.medicines = medicines;
    } else {
        allergies.foods = food;
    }
}

/// What the mob is allergic to, for the dog tag and the chat.
pub fn allergy_string(world: &World, mob: Entity, quirk: Quirk) -> String {
    let Some(allergies) = world.get::<Allergies>(mob) else {
        return String::new();
    };
    if quirk == Quirk::FoodAllergy {
        return FOOD_ALLERGIES
            .iter()
            .filter(|(_, bit)| allergies.foods & bit != 0)
            .map(|(name, _)| *name)
            .collect::<Vec<_>>()
            .join(", ");
    }
    let chem = world.get_resource::<crate::chem::Chem>();
    allergies
        .medicines
        .iter()
        .map(|path| {
            chem.and_then(|c| c.0.id(path)).map_or(path.clone(), |id| {
                chem.unwrap().0.def(id).name.to_lowercase()
            })
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// `add_unique` and `post_add`: the allergy dog tag and the warning.
// shortcut: the dog tag does not name the allergens (no `dogtag/allergy` text).
pub(super) fn allergy_items(world: &mut World, mob: Entity, quirk: Quirk) {
    let allergens = allergy_string(world, mob, quirk);
    let text = if quirk == Quirk::FoodAllergy {
        format!("У вас аллергия на: <b>{allergens}</b>. Следите за тем, что вы едите!")
    } else {
        format!(
            "У вас аллергия на: <b>{allergens}</b>. Убедитесь, что вы не употребляете ничего из \
             этого!"
        )
    };
    let span = if quirk == Quirk::FoodAllergy {
        Span::Info
    } else {
        Span::BoldNotice
    };
    tell_mob(world, mob, (span, &text));
    if let Some(at) = give_item(
        world,
        mob,
        ALLERGY_DOGTAG,
        &[Location::Backpack, Location::Hands],
    ) {
        let flavour = if quirk == Quirk::FoodAllergy {
            "Держите его поблизости с кухней."
        } else {
            "Make sure medical staff can see this..."
        };
        tell_mob(
            world,
            mob,
            (
                Span::BoldNotice,
                &format!("У вас есть жетон аллергика {at}. {flavour}"),
            ),
        );
    }
}

/// `histamine` into the blood (no chemistry: nothing).
pub(super) fn add_histamine(world: &mut World, mob: Entity, amount: f32) {
    let Some(chem) = world
        .get_resource::<crate::chem::Chem>()
        .map(|c| c.0.clone())
    else {
        return;
    };
    let Some(id) = chem.id(HISTAMINE) else {
        return;
    };
    if let Some(mut reagents) = world.get_mut::<crate::chem::MobReagents>(mob) {
        reagents.blood.add_reagent(
            &chem,
            id,
            amount,
            ss13_reagents::DEFAULT_REAGENT_TEMPERATURE,
            None,
            None,
            false,
            false,
        );
    }
}

/// Extreme Medicine Allergy's `block_metab` on each metabolism tick (`crate::chem`): an
/// allergen in the blood deals 3 toxin a second and adds 3 histamine a second, with 10% a
/// second to vomit and take 10 damage to a random organ; epinephrine stops it.
// shortcut: epinephrine stops the reaction but not the allergen's metabolism.
pub fn allergic_reagents(world: &mut World, mob: Entity, secs: f32) {
    let Some(allergies) = world.get::<Allergies>(mob) else {
        return;
    };
    if allergies.medicines.is_empty() || world.get::<Stat>(mob) == Some(&Stat::Dead) {
        return;
    }
    let (count, epinephrine) = {
        let (Some(chem), Some(reagents)) = (
            world.get_resource::<crate::chem::Chem>(),
            world.get::<crate::chem::MobReagents>(mob),
        ) else {
            return;
        };
        let mut count = 0;
        let mut epinephrine = false;
        for r in &reagents.blood.reagents {
            let path = &chem.0.def(r.id).path;
            if ss13_mapload::fidelity::is_under(path, EPINEPHRINE) {
                epinephrine = true;
            }
            if allergies
                .medicines
                .iter()
                .any(|a| ss13_mapload::fidelity::is_under(path, a))
            {
                count += 1;
            }
        }
        (count, epinephrine)
    };
    if count == 0 || epinephrine {
        return;
    }
    for _ in 0..count {
        if let (Some(mut health), Some(mut stat)) = (
            world.get::<Health>(mob).cloned(),
            world.get::<Stat>(mob).copied(),
        ) {
            crate::health::apply_damage(&mut health, &mut stat, DamageType::Tox, 3.0 * secs);
            world.entity_mut(mob).insert((health, stat));
        }
        add_histamine(world, mob, 3.0 * secs);
        if spt_prob(world, 10.0, secs) {
            crate::medical::diseases::vomit(world, mob, 10.0, true, false, 1);
            let slot = {
                let mut rng = world.get_resource_or_init::<CombatRng>();
                [
                    Slot::Brain,
                    Slot::Appendix,
                    Slot::Lungs,
                    Slot::Heart,
                    Slot::Liver,
                    Slot::Stomach,
                ][rng.range(0, 5) as usize]
            };
            organs::adjust_organ_loss(world, mob, slot, 10.0);
        }
    }
}

/// `checkLiked`'s `check_allergic_reaction(foodtypes, chance = 100, histamine_add = 10)` for
/// a food of `foodtypes`: anaphylaxis (or more histamine when it is already there). True if
/// the mob is allergic to it (the other food reactions are skipped).
// The reagent exposures (sugar, alcohol, bugs) that also check it are chemistry's.
pub fn food_allergy_reaction(world: &mut World, mob: Entity, foodtypes: u32) -> bool {
    let allergic = world.get::<Allergies>(mob).map_or(0, |a| a.foods);
    if allergic & foodtypes == 0 {
        return false;
    }
    use crate::medical::diseases::{Disease, Kind, force_contract};
    if force_contract(world, mob, &Disease::new(Kind::Anaphylaxis)) {
        tell_mob(
            world,
            mob,
            (Span::Warning, "Вы чувствуете начинающийся зуд в горле."),
        );
        mood::add_event(world, mob, "allergic_food", &ALLERGIC_FOOD_EVENT);
    } else {
        add_histamine(world, mob, 10.0);
    }
    true
}

/// The addiction of an addict quirk's reagent.
pub(super) fn addict_addiction(reagent: &str) -> Option<crate::addictions::Addiction> {
    crate::addictions::of_reagent(reagent, 1.0)
}

/// `/datum/quirk/item_quirk/addict/add` with the subtypes' `New`: the drug (Junkie: a random
/// one; Smoker: nicotine and a random pack; Alcoholic: ethanol and a random bottle with its
/// favourite booze), then its addiction maxed out.
// shortcut: no Junkie, Smoker or Alcoholic preference (always "Random").
pub(super) fn addict_add(world: &mut World, mob: Entity, quirk: Quirk) {
    let state = {
        let mut rng = world.get_resource_or_init::<CombatRng>();
        let mut pick = |n: usize| rng.range(0, n as u32 - 1) as usize;
        match quirk {
            Quirk::Junkie => AddictState {
                quirk,
                reagent: JUNKIE_DRUGS[pick(JUNKIE_DRUGS.len())],
                container: PILL_BOTTLE.to_owned(),
                favorite: None,
                next_process: 0.0,
            },
            Quirk::Smoker => {
                let (pack, cigarette) = SMOKER_PACKS[pick(SMOKER_PACKS.len())];
                AddictState {
                    quirk,
                    reagent: NICOTINE,
                    container: pack.to_owned(),
                    favorite: Some(cigarette.to_owned()),
                    next_process: 0.0,
                }
            }
            _ => {
                let (bottle, booze) = ALCOHOLIC_BOTTLES[pick(ALCOHOLIC_BOTTLES.len())];
                AddictState {
                    quirk,
                    reagent: ETHANOL,
                    container: format!("{BOTTLE_ROOT}{bottle}"),
                    favorite: Some(format!("{ETHANOL}/{booze}")),
                    next_process: 0.0,
                }
            }
        }
    };
    let addiction = addict_addiction(state.reagent);
    match world.get_mut::<AddictQuirks>(mob) {
        Some(mut addicts) => addicts.0.push(state),
        None => {
            world.entity_mut(mob).insert(AddictQuirks(vec![state]));
        }
    }
    if let Some(addiction) = addiction {
        crate::addictions::add_points(
            world,
            mob,
            addiction,
            crate::addictions::MAX_ADDICTION_POINTS,
        );
    }
}

pub(super) fn addict_state(world: &World, mob: Entity, quirk: Quirk) -> Option<AddictState> {
    world
        .get::<AddictQuirks>(mob)?
        .0
        .iter()
        .find(|a| a.quirk == quirk)
        .cloned()
}

/// `/datum/quirk/item_quirk/addict/add_unique`: the drug container (the Junkie's pill bottle
/// with 7 pills of 3 units), the Smoker's lighter.
// shortcut: the pills keep the template's look (no random `pill[1-20]` state).
pub(super) fn addict_items(world: &mut World, mob: Entity, quirk: Quirk) {
    let Some(state) = addict_state(world, mob, quirk) else {
        return;
    };
    let Some(container) = spawn_item(world, &state.container) else {
        return;
    };
    if quirk == Quirk::Junkie {
        let chem = world
            .get_resource::<crate::chem::Chem>()
            .map(|c| c.0.clone());
        let id = chem.as_ref().and_then(|c| c.id(state.reagent));
        for _ in 0..JUNKIE_PILLS {
            let Some(pill) = spawn_item(world, PILL) else {
                break;
            };
            crate::storage::put_inside(world, pill, container);
            if let (Some(chem), Some(id)) = (chem.as_ref(), id)
                && let Some(mut reagents) = world.get_mut::<crate::chem::Reagents>(pill)
            {
                reagents.0.add_reagent(
                    chem,
                    id,
                    JUNKIE_PILL_UNITS,
                    ss13_reagents::DEFAULT_REAGENT_TEMPERATURE,
                    None,
                    None,
                    true,
                    false,
                );
            }
        }
    }
    let at = place_item(
        world,
        mob,
        container,
        &[
            Location::LeftPocket,
            Location::RightPocket,
            Location::Backpack,
            Location::Hands,
        ],
    );
    let flavour = match quirk {
        Quirk::Junkie => "Лучше надеяться, что у вас они не закончатся...",
        Quirk::Smoker => "Убедитесь, что вы получите свои любимые, до того, как они закончатся.",
        _ => "Убедитесь, что запасы вашего любимого напитка не закончатся.",
    };
    let name = crate::chem::name_of(world, container);
    tell_mob(
        world,
        mob,
        (
            Span::BoldNotice,
            &format!("У вас есть {name} {at}. {flavour}"),
        ),
    );
    if quirk == Quirk::Smoker {
        give_item(
            world,
            mob,
            LIGHTER,
            &[
                Location::LeftPocket,
                Location::RightPocket,
                Location::Backpack,
                Location::Hands,
            ],
        );
    }
}

/// `/datum/quirk/item_quirk/addict/process` (every 30 s: the addiction comes back if it was
/// kicked), the Smoker's brand check of the cigarette in the mask slot and the Alcoholic's
/// `check_brandy` (followed every fire instead of each reagent tick).
// The booze check looks at the blood and the stomach (tg: the reagents that tick).
pub(super) fn addict_process(world: &mut World, mob: Entity, quirk: Quirk) {
    let Some(state) = addict_state(world, mob, quirk) else {
        return;
    };
    // `no_process_traits = list(TRAIT_LIVERLESS_METABOLISM)`.
    if crate::species::has_trait(world, mob, Trait::LiverlessMetabolism) {
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now >= state.next_process {
        if let Some(mut addicts) = world.get_mut::<AddictQuirks>(mob)
            && let Some(a) = addicts.0.iter_mut().find(|a| a.quirk == quirk)
        {
            a.next_process = now + ADDICT_PROCESS_SECS;
        }
        if let Some(addiction) = addict_addiction(state.reagent) {
            let active = world
                .get::<crate::addictions::Addictions>(mob)
                .and_then(|a| a.cycle(addiction))
                .is_some();
            if !active {
                tell_mob(
                    world,
                    mob,
                    (
                        Span::Danger,
                        "Вы думали, что завязали, но чувствуете, что возвращаетесь к вредным привычкам...",
                    ),
                );
                crate::addictions::add_points(
                    world,
                    mob,
                    addiction,
                    crate::addictions::MAX_ADDICTION_POINTS,
                );
            }
        }
    }
    let Some(favorite) = state.favorite else {
        return;
    };
    match quirk {
        Quirk::Smoker => {
            let mask = world
                .get::<Inventory>(mob)
                .and_then(|i| i.mask)
                .and_then(|m| world.get::<DmItem>(m))
                .map(|dm| dm.path.clone());
            let Some(mask) = mask else {
                return;
            };
            if !ss13_mapload::fidelity::is_under(&mask, "/obj/item/cigarette") {
                return;
            }
            if ss13_mapload::fidelity::is_under(&mask, &favorite) {
                mood::clear_event(world, mob, "wrong_cigs");
            } else {
                mood::add_event(world, mob, "wrong_cigs", &WRONG_BRAND_EVENT);
            }
        }
        Quirk::Alcoholic => {
            let (mut favourite, mut other) = (false, false);
            if let (Some(chem), Some(reagents)) = (
                world.get_resource::<crate::chem::Chem>(),
                world.get::<crate::chem::MobReagents>(mob),
            ) {
                for r in reagents
                    .blood
                    .reagents
                    .iter()
                    .chain(&reagents.stomach.reagents)
                {
                    let path = &chem.0.def(r.id).path;
                    if !ss13_mapload::fidelity::is_under(path, ETHANOL) {
                        continue;
                    }
                    if ss13_mapload::fidelity::is_under(path, &favorite) {
                        favourite = true;
                    } else {
                        other = true;
                    }
                }
            }
            if favourite {
                mood::clear_event(world, mob, "wrong_alcohol");
            } else if other {
                mood::add_event(world, mob, "wrong_alcohol", &WRONG_BRANDY_EVENT);
            }
        }
        _ => {}
    }
}
