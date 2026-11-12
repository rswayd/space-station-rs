use super::*;

/// tg `has_quirk`.
pub fn has(world: &World, mob: Entity, quirk: Quirk) -> bool {
    world.get::<Quirks>(mob).is_some_and(|q| q.has(quirk))
}

/// tg `SSquirks.filter_invalid_quirks`: drops unknown names, blacklisted pairs and positive
/// quirks past the limit, then, while the point balance is positive (more spent than given),
/// positive quirks from the first on. Returns the same list when all of it is valid.
pub fn filter_invalid(names: &[String]) -> Vec<String> {
    let mut new: Vec<String> = Vec::new();
    let mut positive: Vec<(String, i32)> = Vec::new();
    let mut balance = -DEFAULT_QUIRK_POINTS;
    for name in names {
        let Some(quirk) = Quirk::by_name(name) else {
            continue;
        };
        let blacklisted = BLACKLIST.iter().any(|list| {
            list.contains(&quirk)
                && list
                    .iter()
                    .any(|other| *other != quirk && new.iter().any(|n| n == other.def().name))
        });
        if blacklisted {
            continue;
        }
        let value = quirk.def().value;
        if value > 0 {
            if positive.len() == MAX_POSITIVE_QUIRKS {
                continue;
            }
            positive.push((name.clone(), value));
        }
        balance += value;
        new.push(name.clone());
    }
    if balance > 0 {
        let mut left = balance;
        for (name, value) in positive {
            left -= value;
            new.retain(|n| *n != name);
            if left <= 0 {
                break;
            }
        }
    }
    new
}

/// tg `give_quirk`/`remove_quirk` of the preferences middleware: the change is kept only when
/// the result is valid as a whole. True if it was applied.
pub fn change_prefs(prefs: &mut QuirkPrefs, name: &str, give: bool) -> bool {
    let current = filter_invalid(&prefs.0);
    let new: Vec<String> = if give {
        let mut new = current.clone();
        if !new.iter().any(|n| n == name) {
            new.push(name.to_owned());
        }
        new
    } else {
        if !current.iter().any(|n| n == name) {
            return false;
        }
        current.iter().filter(|n| *n != name).cloned().collect()
    };
    if filter_invalid(&new) != new {
        return false;
    }
    prefs.0 = new;
    true
}

/// tg `AssignQuirks`: the client's chosen quirks onto its new human, silently, with their items.
pub fn assign(world: &mut World, client: Entity, mob: Entity) {
    let Some(prefs) = world.get::<QuirkPrefs>(client) else {
        return;
    };
    let names = filter_invalid(&prefs.0);
    for name in names {
        if let Some(quirk) = Quirk::by_name(&name) {
            add_quirk(world, mob, quirk, false, true);
        }
    }
}

pub(super) fn tell_mob(world: &mut World, mob: Entity, (span, text): (Span, &str)) {
    crate::tools::tell_mob(world, mob, span, text);
}

/// tg `add_quirk` -> `add_to_holder`: false if the mob has it already. `announce`: the gain
/// line; `unique`: the one-time effects (items).
pub fn add_quirk(
    world: &mut World,
    mob: Entity,
    quirk: Quirk,
    announce: bool,
    unique: bool,
) -> bool {
    if world.get_entity(mob).is_err() || has(world, mob, quirk) {
        return false;
    }
    match world.get_mut::<Quirks>(mob) {
        Some(mut quirks) => quirks.0.push(quirk),
        None => {
            world.entity_mut(mob).insert(Quirks(vec![quirk]));
        }
    }
    let def = quirk.def();
    info!("{mob} gained the quirk {}", def.path);
    // `add`.
    match quirk {
        Quirk::Hypersensitive => {
            if let Some(mut mood) = world.get_mut::<Mood>(mob) {
                mood.mood_modifier += 0.5;
                mood.update_mood();
            }
        }
        Quirk::Unstable => {
            if let Some(mut mood) = world.get_mut::<Mood>(mob) {
                mood.unstable = true;
            }
        }
        Quirk::Mute | Quirk::Pacifist | Quirk::SoftSpoken => traumas::refresh(world, mob),
        Quirk::Bilingual => bilingual(world, mob),
        Quirk::Csl => csl(world, mob, true),
        Quirk::Foreigner => foreigner(world, mob, true),
        Quirk::Bald => {
            let covered = head_covered(world, mob);
            let old_hair = world
                .get_mut::<crate::human::HumanLook>(mob)
                .map(|mut look| look.hair_style.take());
            if let Some(old_hair) = old_hair {
                world.entity_mut(mob).insert(BaldHead { old_hair, covered });
                crate::inventory::refresh_look(world, mob);
            }
        }
        Quirk::BodyPurist => update_cybernetics(world, mob),
        Quirk::Junkie | Quirk::Smoker | Quirk::Alcoholic => addict_add(world, mob, quirk),
        Quirk::MedicineAllergy | Quirk::FoodAllergy => allergy_add(world, mob, quirk),
        // `post_add`: the policy line.
        Quirk::Pyromania => tell_mob(
            world,
            mob,
            (
                Span::Info,
                "Обратите внимание: наличие черты pyromania не даёт вам никаких дополнительных \
                 прав на устраивание поджогов.",
            ),
        ),
        Quirk::SocialAnxiety => {
            let mood_mod = anxiety_mood_mod(world, mob);
            world.entity_mut(mob).insert(ss13_chat::speech::Anxiety {
                mood_mod,
                dumb_thing: true,
            });
        }
        // `/datum/brain_trauma/severe/paralysis/hemiplegic` of a random side.
        Quirk::Hemiplegic => {
            let side = if world.get_resource_or_init::<CombatRng>().chance(50.0) {
                Paralysis::Left
            } else {
                Paralysis::Right
            };
            if let Some(brain) = organs::organ(world, mob, Slot::Brain) {
                traumas::gain(
                    world,
                    brain,
                    Kind::Paralysis(side),
                    Some(Resilience::Absolute),
                );
            }
        }
        // `/datum/brain_trauma/severe/paralysis/paraplegic`, `TRAUMA_RESILIENCE_ABSOLUTE`.
        Quirk::Paraplegic => {
            if let Some(brain) = organs::organ(world, mob, Slot::Brain) {
                traumas::gain(
                    world,
                    brain,
                    Kind::Paralysis(Paralysis::Legs),
                    Some(Resilience::Absolute),
                );
            }
        }
        _ => {}
    }
    if announce && let Some(gain) = def.gain {
        tell_mob(world, mob, gain);
    }
    if unique {
        match quirk {
            Quirk::Blind => {
                give_item(world, mob, BLINDFOLD, &[Location::Eyes, Location::Hands]);
            }
            Quirk::Nearsighted => {
                give_item(
                    world,
                    mob,
                    GLASSES,
                    &[Location::Eyes, Location::Backpack, Location::Hands],
                );
            }
            Quirk::Deaf => {
                give_item(world, mob, DEAF_PIN, &[Location::Backpack, Location::Hands]);
            }
            Quirk::Limper => {
                give_item(world, mob, CANE, &[Location::Hands, Location::Backpack]);
            }
            Quirk::TinMan => tin_man(world, mob),
            Quirk::Junkie | Quirk::Smoker | Quirk::Alcoholic => addict_items(world, mob, quirk),
            Quirk::MedicineAllergy | Quirk::FoodAllergy => allergy_items(world, mob, quirk),
            Quirk::ClownEnjoyer | Quirk::MimeFan | Quirk::Musician | Quirk::Tagger => {
                let path = match quirk {
                    Quirk::ClownEnjoyer => CLOWN_PIN,
                    Quirk::MimeFan => MIME_PIN,
                    Quirk::Musician => MUSIC_BEACON,
                    _ => SPRAYCAN,
                };
                give_item(world, mob, path, &[Location::Backpack, Location::Hands]);
            }
            Quirk::BrainTumor => {
                if let Some(at) = give_item(
                    world,
                    mob,
                    MANNITOL_BOTTLE,
                    &[
                        Location::LeftPocket,
                        Location::RightPocket,
                        Location::Backpack,
                        Location::Hands,
                    ],
                ) {
                    let text = format!(
                        "У вас есть баночка таблеток маннитола {at}. Они помогут вам выжить, пока \
                         вы не получите запас лекарств. Не полагайтесь на них слишком сильно!"
                    );
                    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
                        crate::tools::tell(world, client, Span::BoldNotice, text);
                    }
                }
            }
            _ => {}
        }
    }
    true
}

/// `/datum/quirk/bilingual/add` with the "Random" language preference: one roundstart species
/// language, or Galactic Uncommon when that one is already known.
// shortcut: no language, "speakable" or skill preferences (always spoken and understood).
pub(super) fn bilingual(world: &mut World, mob: Entity) {
    use ss13_chat::language::{ALL, LanguageHolder, LanguageId, LanguageSource};
    let pool = crate::species::uncommon_roundstart_languages();
    if pool.is_empty() {
        return;
    }
    let pick = {
        let mut rng = world.get_resource_or_init::<CombatRng>();
        pool[rng.range(0, pool.len() as u32 - 1) as usize]
    };
    let knows = |w: &World, l: LanguageId| {
        ss13_chat::language::understanding(w.get::<LanguageHolder>(mob), false, l)
            >= ss13_chat::language::FULL
    };
    let mut language = pick;
    if knows(world, language) {
        language = LanguageId::Uncommon;
        let text = if knows(world, language) {
            "Вы уже знакомы с данной чертой в своих предпочтениях, поэтому не стали изучать ее."
        } else {
            "Вы уже знакомы с данной чертой в своих предпочтениях, поэтому вместо нее вы выучили \
             старогалактический."
        };
        if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
            crate::tools::tell(world, client, Span::BoldNotice, text);
        }
        if knows(world, language) {
            return;
        }
    }
    ss13_chat::language::edit_holder(world, mob, |h| {
        h.grant(language, ALL, LanguageSource::Quirk);
    });
}

/// tg `remove_quirk`: false if the mob does not have it.
pub fn remove_quirk(world: &mut World, mob: Entity, quirk: Quirk) -> bool {
    let Some(mut quirks) = world.get_mut::<Quirks>(mob) else {
        return false;
    };
    let Some(i) = quirks.0.iter().position(|q| *q == quirk) else {
        return false;
    };
    quirks.0.remove(i);
    match quirk {
        Quirk::Hypersensitive => {
            if let Some(mut mood) = world.get_mut::<Mood>(mob) {
                mood.mood_modifier -= 0.5;
                mood.update_mood();
            }
        }
        Quirk::Unstable => {
            if let Some(mut mood) = world.get_mut::<Mood>(mob) {
                mood.unstable = false;
            }
        }
        Quirk::Jolly => mood::clear_event(world, mob, "jolly"),
        Quirk::Depression => mood::clear_event(world, mob, "depression"),
        Quirk::BadBack => mood::clear_event(world, mob, "back_pain"),
        Quirk::Mute | Quirk::Pacifist | Quirk::SoftSpoken => traumas::refresh(world, mob),
        Quirk::Csl => csl(world, mob, false),
        Quirk::Foreigner => foreigner(world, mob, false),
        Quirk::SocialAnxiety => {
            world.entity_mut(mob).remove::<ss13_chat::speech::Anxiety>();
        }
        Quirk::ClownEnjoyer => mood::clear_event(world, mob, "clown_enjoyer_pin"),
        Quirk::MimeFan => mood::clear_event(world, mob, "mime_fan_pin"),
        Quirk::Pyromania => {
            world.entity_mut(mob).remove::<LastFireSize>();
            mood::clear_event(world, mob, "pyromania_fire");
        }
        Quirk::MedicineAllergy | Quirk::FoodAllergy => {
            if let Some(mut allergies) = world.get_mut::<Allergies>(mob) {
                if quirk == Quirk::MedicineAllergy {
                    allergies.medicines.clear();
                } else {
                    allergies.foods = 0;
                }
            }
        }
        // `/datum/quirk/item_quirk/addict/remove`: every addiction goes.
        Quirk::Junkie | Quirk::Smoker | Quirk::Alcoholic => {
            if let Some(mut addicts) = world.get_mut::<AddictQuirks>(mob) {
                addicts.0.retain(|a| a.quirk != quirk);
            }
            for a in crate::addictions::Addiction::ALL {
                crate::addictions::remove_points(
                    world,
                    mob,
                    a,
                    crate::addictions::MAX_ADDICTION_POINTS,
                );
            }
        }
        Quirk::BodyPurist => {
            world.entity_mut(mob).remove::<Cybernetics>();
            mood::clear_event(world, mob, "body_purist");
        }
        Quirk::Bald => {
            mood::clear_event(world, mob, "bad_hair_day");
            let bald = world.entity_mut(mob).take::<BaldHead>();
            let restored = bald.is_some_and(|bald| {
                world
                    .get_mut::<crate::human::HumanLook>(mob)
                    .is_some_and(|mut look| {
                        let bald_now = look.hair_style.is_none();
                        if bald_now {
                            look.hair_style = bald.old_hair;
                        }
                        bald_now
                    })
            });
            if restored {
                crate::inventory::refresh_look(world, mob);
            }
        }
        // `remove_all_languages(LANGUAGE_QUIRK)`, `remove_all_partial_languages(LANGUAGE_QUIRK)`.
        Quirk::Bilingual => {
            if let Some(mut holder) = world.get_mut::<ss13_chat::language::LanguageHolder>(mob) {
                holder.remove_all(Some(ss13_chat::language::LanguageSource::Quirk));
            }
        }
        _ => {}
    }
    if let Some(lose) = quirk.def().lose {
        tell_mob(world, mob, lose);
    }
    true
}
