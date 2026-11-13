//! Absorbing a victim.

use super::*;

/// `can_absorb_dna(target, verbose)`.
pub(super) fn can_absorb(world: &mut World, mob: Entity, target: Entity, verbose: bool) -> bool {
    let (Some(ling), (name, look)) = (world.get::<Changeling>(mob).cloned(), dna_of(world, mob))
    else {
        return false;
    };
    let fail = |world: &mut World, text: String| {
        if verbose {
            chat_to(world, mob, Span::Warning, text);
        }
        false
    };
    if let Some(top) = ling.profiles.first()
        && top.same_dna(&name, look.as_ref())
        && ling.profiles.len() > DNA_MAX
    {
        return fail(
            world,
            "Мы достигли предела своей способности хранить генетическую информацию! Мы должны \
             трансформироваться, прежде чем поглощать больше."
                .to_owned(),
        );
    }
    let victim = name_of(world, target);
    let (target_name, target_look) = dna_of(world, target);
    if target_look.is_none() {
        return fail(
            world,
            format!("ДНК {victim} несовместимо с нашей биологией."),
        );
    }
    if ling.has_profile(&target_name, target_look.as_ref()) {
        return fail(world, "Мы уже имеем это ДНК в нашем хранилище!".to_owned());
    }
    // `TRAIT_BADDNA` (a drained body is also `TRAIT_HUSK`).
    if world.get::<Husk>(target).is_some() {
        return fail(world, format!("ДНК {victim} разрушено до неузнаваемости!"));
    }
    true
}

/// `absorb_dna/can_sting` + `sting_action`: the grab checks, then the first stage.
pub(super) fn start_absorb(world: &mut World, client: Entity, mob: Entity) -> bool {
    if world
        .get::<Changeling>(mob)
        .is_some_and(|c| c.absorbing.is_some())
    {
        tell(world, client, Span::Warning, "уже поглощаем!");
        return false;
    }
    let target = world.get::<Pulling>(mob).map(|p| p.0);
    let Some(target) = target.filter(|&t| world.get::<Health>(t).is_some()) else {
        tell(world, client, Span::Warning, "нужно взять в захват!");
        return false;
    };
    if world.get::<GrabState>(mob).copied().unwrap_or_default() < GrabState::Aggressive {
        tell(world, client, Span::Warning, "нужно усилить захват!");
        return false;
    }
    if !can_absorb(world, mob, target, true) {
        return false;
    }
    if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
        ling.absorbing = Some((target, 0));
    }
    info!("{mob} starts absorbing {target} (changeling)");
    absorb_stage(world, client, mob, target, 1);
    // The cost (0) and the cooldown come when it finishes.
    false
}

/// Stage `stage` of `attempt_absorb`: its message, then a 15 s `do_after`.
pub(super) fn absorb_stage(
    world: &mut World,
    client: Entity,
    mob: Entity,
    target: Entity,
    stage: u8,
) {
    let (user, victim) = (name_of(world, mob), name_of(world, target));
    match stage {
        1 => chat_to(
            world,
            mob,
            Span::Notice,
            "Это существо совместимо. Мы должны стоять неподвижно...",
        ),
        2 => visible(
            world,
            mob,
            "Мы вытягиваем жало-хоботок.",
            &format!("{user} вытягивает жало-хоботок!"),
            Span::Warning,
        ),
        _ => {
            visible(
                world,
                mob,
                &format!("Мы пронзаем {victim} жалом-хоботком."),
                &format!("{user} пронзает {victim} жалом-хоботком!"),
                Span::Danger,
            );
            chat_to(
                world,
                target,
                Span::BoldWarning,
                "Вы чувствуете острую колющую боль!",
            );
            // `take_overall_damage(40)`.
            hurt(world, target, DamageType::Brute, 40.0);
            // `absorbing_loop = new(owner, start_immediately = TRUE)`.
            crate::looping_sound::start(world, mob, &crate::looping_sound::defs::CHANGELING_ABSORB);
        }
    }
    if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
        ling.absorbing = Some((target, stage));
    }
    let item = world.get::<Inventory>(mob).and_then(Inventory::active_item);
    let actor = Actor { client, mob, item };
    let job = Job::Changeling(ChangelingJob::Absorb);
    if !do_after(world, actor, target, ABSORB_STAGE_DS, 0, job) {
        absorb_interrupted(world, mob);
    }
}

/// `balloon_alert("прервано!")`: the absorption ends.
pub(super) fn absorb_interrupted(world: &mut World, mob: Entity) {
    let was = world
        .get_mut::<Changeling>(mob)
        .and_then(|mut c| c.absorbing.take());
    stop_absorb_loop(world, mob);
    if was.is_some() {
        chat_to(world, mob, Span::Warning, "прервано!");
    }
}

/// `qdel(absorbing_loop)`.
fn stop_absorb_loop(world: &mut World, mob: Entity) {
    let path = crate::looping_sound::defs::CHANGELING_ABSORB.path;
    crate::looping_sound::cancel(world, mob, path, true);
}

/// [`Job::Changeling`] completions.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: ChangelingJob) {
    match job {
        ChangelingJob::Absorb => {
            let Some((target, stage)) = world.get::<Changeling>(mob).and_then(|c| c.absorbing)
            else {
                return;
            };
            if target != action.target {
                return;
            }
            match absorb_next(stage) {
                AbsorbStep::Stage(next) => absorb_stage(world, action.client, mob, target, next),
                AbsorbStep::Finish => finish_absorb(world, mob, target),
            }
        }
    }
}

/// `absorb_dna/sting_action` after `attempt_absorb`: the profile, the chemicals, the victim
/// killed and drained (`death()`, `Drain()`).
pub(super) fn finish_absorb(world: &mut World, mob: Entity, target: Entity) {
    if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
        ling.absorbing = None;
    }
    stop_absorb_loop(world, mob);
    // Revalidated: another changeling may have drained it meanwhile.
    if world.get_entity(target).is_err() || !can_absorb(world, mob, target, true) {
        return;
    }
    let (user, victim) = (name_of(world, mob), name_of(world, target));
    visible(
        world,
        mob,
        &format!("We have absorbed {victim}."),
        &format!("{user} sucks the fluids from {victim}!"),
        Span::Danger,
    );
    // `client || mind || last_mind`: a crew body or a player's.
    let true_absorb =
        world.get::<PlayerJob>(target).is_some() || world.get::<PlayerMob>(target).is_some();
    if !true_absorb {
        chat_to(
            world,
            mob,
            Span::Changeling,
            format!(
                "You absorb {victim}, but their weak DNA is not enough to satisfy your hunger."
            ),
        );
    }
    let (name, look) = dna_of(world, target);
    let memory = stolen_memory(world, target, &victim);
    if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
        if !ling.has_profile(&name, look.as_ref()) {
            ling.add_profile(Profile {
                name,
                look,
                protected: false,
            });
            if true_absorb {
                ling.true_absorbs += 1;
            }
        }
        ling.memory.push_str(&memory);
    }
    // `set_nutrition(min(nutrition + target.nutrition, NUTRITION_LEVEL_WELL_FED))`.
    let theirs = world.get::<Hunger>(target).map_or(0.0, |h| h.nutrition);
    if let Some(mut hunger) = world.get_mut::<Hunger>(mob)
        && hunger.nutrition < NUTRITION_LEVEL_WELL_FED
    {
        hunger.nutrition = (hunger.nutrition + theirs).min(NUTRITION_LEVEL_WELL_FED);
    }
    // "Absorb a lizard, speak Draconic" (`copy_languages(target, LANGUAGE_ABSORB)`).
    {
        use ss13_chat::language::{LanguageHolder, LanguageSource, Preset};
        let theirs = world
            .get::<LanguageHolder>(target)
            .cloned()
            .unwrap_or_else(|| {
                LanguageHolder::from_preset(
                    &Preset::of_path("/datum/language_holder/atom_basic").unwrap_or_default(),
                    LanguageSource::Atom,
                )
            });
        ss13_chat::language::edit_holder(world, mob, |h| {
            h.copy_from(&theirs, LanguageSource::Absorb);
        });
    }
    absorb_changeling(world, mob, target, &victim);
    if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
        ling.adjust_chemicals(10.0, None);
        if true_absorb {
            ling.can_respec += 1;
        }
    }
    info!("{mob} absorbed {target} (changeling)");
    drain(world, target);
    let now = world.resource::<Time>().elapsed_secs_f64();
    world.entity_mut(mob).insert(NextMove(now + CLICK_CD_MELEE));
    refresh(world, mob);
}

/// `absorb_memories`' antagonist part: the victim's objectives.
pub(super) fn stolen_memory(world: &World, target: Entity, victim: &str) -> String {
    let objectives: Vec<String> =
        if let Some(t) = world.get::<crate::antag::traitor::Traitor>(target) {
            t.objectives.iter().map(|o| o.explanation.clone()).collect()
        } else if let Some(b) = world.get::<crate::antag::brother::Brother>(target) {
            b.objectives.iter().map(|o| o.explanation.clone()).collect()
        } else if let Some(v) = world.get::<crate::antag::vampire::server::Vampire>(target) {
            v.objectives.iter().map(|o| o.explanation()).collect()
        } else if let Some(c) = world.get::<Changeling>(target) {
            c.objectives
                .iter()
                .map(LingObjective::explanation)
                .collect()
        } else {
            Vec::new()
        };
    let mut memory = String::new();
    if !objectives.is_empty() {
        memory.push_str(" Задачи:");
        for (i, text) in objectives.iter().enumerate() {
            memory.push_str(&format!(" Задача #{}: {text}.", i + 1));
        }
    }
    memory.push_str(&format!(" Это все, что имел {victim}. "));
    memory
}

/// A changeling victim gives half its points and storage and all its DNA, and falls.
pub(super) fn absorb_changeling(world: &mut World, mob: Entity, target: Entity, victim: &str) {
    let Some(theirs) = world
        .get::<Changeling>(target)
        .filter(|c| !c.fallen)
        .cloned()
    else {
        return;
    };
    chat_to(
        world,
        mob,
        Span::BoldNotice,
        format!("{victim} были одними из нас. Мы поглотили их силу."),
    );
    if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
        let points = theirs.total_genetic_points / 2;
        ling.genetic_points += points;
        ling.total_genetic_points += points;
        let chems = (theirs.total_chem_storage / 2.0).round();
        ling.adjust_chemicals(chems, None);
        ling.total_chem_storage += chems;
        ling.absorbed_count += theirs.absorbed_count;
    }
    remove_powers(world, target);
    if let Some(mut fallen) = world.get_mut::<Changeling>(target) {
        fallen.fallen = true;
        fallen.purchased.clear();
    }
    world.entity_mut(target).remove::<ChangelingHivemind>();
    refresh(world, target);
}

/// `death()` + `Drain()`: dead, husked, bloodless.
pub(super) fn drain(world: &mut World, target: Entity) {
    if world.get::<Stat>(target) != Some(&Stat::Dead) {
        world.entity_mut(target).insert(Stat::Dead);
    }
    world.entity_mut(target).insert(Husk);
    if let Some(mut look) = world.get_mut::<HumanLook>(target) {
        look.husk = true;
    }
    if let Some(mut inventory) = world.get_mut::<Inventory>(target) {
        inventory.set_changed();
    }
    if let Some(mut blood) = world.get_mut::<Blood>(target) {
        blood.volume = 0.0;
    }
}

pub(super) fn hurt(world: &mut World, mob: Entity, kind: DamageType, amount: f32) {
    let (Some(mut health), Some(mut stat)) = (
        world.get::<Health>(mob).cloned(),
        world.get::<Stat>(mob).copied(),
    ) else {
        return;
    };
    let changed = apply_damage(&mut health, &mut stat, kind, amount);
    world.entity_mut(mob).insert(health);
    if changed.is_some() {
        world.entity_mut(mob).insert(stat);
    }
}
