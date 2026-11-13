//! The traumas' effects: the scanner line, the derived state and each trauma's `on_life`.

use super::*;

/// The scanner's `Mental trauma: ...` line of `mob`'s brain (`get_status_appendix`).
pub fn scan_line(world: &World, mob: Entity) -> Option<String> {
    let mut all = of_mob(world, mob);
    // `known_trauma = FALSE`: scanners do not show it.
    all.retain(|t| t.kind != Kind::Beepsky && !matches!(t.kind, Kind::Voided { .. }));
    if all.is_empty() {
        return None;
    }
    let parts: Vec<String> = all
        .iter()
        .map(|t| {
            format!(
                "{} {}",
                t.resilience.scan_word(),
                ss13_chat::capitalize(&t.kind.scan_desc())
            )
        })
        .collect();
    // DM `english_list(list, and_text = ", and ")`.
    let text = match parts.as_slice() {
        [one] => one.clone(),
        [init @ .., last] => format!("{}, and {last}", init.join(", ")),
        [] => String::new(),
    };
    Some(format!("Mental trauma: {text}."))
}

/// `/datum/brain_trauma/severe/aphasia` `on_gain`/`on_lose` (`block`: every other language
/// blocked) and the hallucinations trauma's Gibbering (`gibbering`), from `LANGUAGE_APHASIA`.
pub(super) fn set_aphasia(world: &mut World, mob: Entity, block: bool, gibbering: bool) {
    use ss13_chat::language::{ALL, LanguageHolder, LanguageId, LanguageSource, Preset};
    let had = world
        .get::<LanguageHolder>(mob)
        .is_some_and(|h| h.has_source(LanguageSource::Aphasia));
    if !had && !block && !gibbering {
        return;
    }
    let before = world.get::<LanguageHolder>(mob).cloned();
    let mut holder = before.clone().unwrap_or_else(|| {
        LanguageHolder::from_preset(
            &Preset::of_path("/datum/language_holder/atom_basic").unwrap_or_default(),
            LanguageSource::Atom,
        )
    });
    for language in LanguageId::ALL {
        if language == LanguageId::Aphasia {
            continue;
        }
        holder.unblock(language, ALL, Some(LanguageSource::Aphasia));
        if block {
            holder.block(language, ALL, LanguageSource::Aphasia);
        }
    }
    holder.remove(LanguageId::Aphasia, ALL, Some(LanguageSource::Aphasia));
    if block || gibbering {
        holder.grant(LanguageId::Aphasia, ALL, LanguageSource::Aphasia);
    }
    if before.as_ref() != Some(&holder) {
        world.entity_mut(mob).insert(holder);
    }
}

/// Applies the traumas of `mob`'s brain to `mob` (the traits of their `on_gain`/`on_lose`), with
/// the quirks that give the same traits.
pub fn refresh(world: &mut World, mob: Entity) {
    if world.get_entity(mob).is_err() {
        return;
    }
    let mut speech = ss13_chat::speech::Impediments::default();
    let mut pacifist = false;
    let mut paralyzed = [false; 6];
    let mut aphasia = false;
    let mut gibbering = false;
    let mut phobias = Vec::new();
    let mut monophobia = false;
    let mut split = false;
    let mut friend = false;
    let mut tenacity = false;
    for trauma in of_mob(world, mob) {
        match trauma.kind {
            Kind::Aphasia => aphasia = true,
            Kind::Hallucinations => gibbering |= !trauma.suppressed,
            Kind::Phobia(i) => phobias.push(i),
            Kind::Monophobia => monophobia = true,
            Kind::SplitPersonality => split = true,
            Kind::ImaginaryFriend => friend = true,
            Kind::Tenacity => tenacity = true,
            Kind::Stuttering => speech.stutter = true,
            Kind::Dumbness => speech.derp = true,
            Kind::SpeechImpediment => speech.unintelligible = true,
            Kind::Mute => speech.mute = true,
            Kind::Pacifism => pacifist = true,
            Kind::Voided { stable: false } => pacifist = true,
            Kind::Paralysis(p) => {
                for zone in p.zones() {
                    paralyzed[zone.index()] = true;
                }
            }
            _ => {}
        }
    }
    // The heretic's curse of paralysis (`TRAIT_PARALYSIS_L_LEG`, `TRAIT_PARALYSIS_R_LEG`).
    if world
        .get::<crate::antag::heretic::server::ParalysisCurse>(mob)
        .is_some()
    {
        for zone in [BodyZone::LLeg, BodyZone::RLeg] {
            paralyzed[zone.index()] = true;
        }
    }
    // The Mute (`TRAIT_MUTE`), Soft-Spoken (`TRAIT_FORCE_WHISPER`) and Pacifist
    // (`TRAIT_PACIFISM`) quirks (`crate::quirks`).
    if let Some(quirks) = world.get::<crate::quirks::Quirks>(mob) {
        speech.mute |= quirks.has(crate::quirks::Quirk::Mute);
        speech.force_whisper |= quirks.has(crate::quirks::Quirk::SoftSpoken);
        pacifist |= quirks.has(crate::quirks::Quirk::Pacifist);
    }
    // `/datum/status_effect/pacify` (`crate::status_effects` calls this when it starts or ends).
    pacifist |= world
        .get::<crate::status_effects::StatusEffects>(mob)
        .is_some_and(crate::status_effects::StatusEffects::pacified);
    // The Hippocrates bust's oath (`crate::service`).
    pacifist |= crate::service::hippocratic_sworn(world, mob);
    // A basketball minigame body (`BASKETBALL_MINIGAME_TRAIT`).
    pacifist |= world
        .get::<crate::minigames::basketball::Baller>(mob)
        .is_some();
    // A shy drone (`DRONE_SHY_TRAIT`, `crate::silicon::drones`).
    pacifist |= crate::silicon::drones::shy(world, mob);
    // `/datum/status_effect/gonbola_pacify`: `TRAIT_PACIFISM`, `TRAIT_MUTE`.
    let gonbola = crate::legcuffs::gonbola_worn(world, mob);
    pacifist |= gonbola;
    speech.mute |= gonbola;
    // The mime's vow (`TRAIT_MIMING`, `can_speak(allow_mimes = FALSE)`).
    speech.mute |= world.get::<crate::service::MimeVow>(mob).is_some();
    // A stuck item, a slit throat (`crate::afflictions`).
    speech.mute |= crate::afflictions::mutes(world, mob);
    // The Unintelligible mutation (`crate::genetics`).
    speech.unintelligible |=
        crate::genetics::has_effect(world, mob, crate::genetics::Effect::Unintelligible);
    set_aphasia(world, mob, aphasia, gibbering);
    super::super::fear::set_sources(world, mob, &phobias, monophobia);
    super::super::split_personality::sync(world, mob, split);
    super::super::imaginary_friend::sync(world, mob, friend);
    // Tenacity's `TRAIT_NOSOFTCRIT` and `TRAIT_NOHARDCRIT`, with the species' own and the
    // tactical deniability implant's.
    let species_softcrit = crate::species::def_of(world, mob).body_mods().no_softcrit;
    let deniability = crate::implants::denies_crit(world, mob);
    if let Some(health) = world.get::<Health>(mob) {
        let (soft, hard) = (
            species_softcrit || tenacity || deniability,
            tenacity || deniability,
        );
        if health.mods.no_softcrit != soft || health.mods.no_hardcrit != hard {
            let mut health = health.clone();
            health.mods.no_softcrit = soft;
            health.mods.no_hardcrit = hard;
            world.entity_mut(mob).insert(health);
        }
    }
    let mut entity = world.entity_mut(mob);
    if speech == ss13_chat::speech::Impediments::default() {
        entity.remove::<ss13_chat::speech::Impediments>();
    } else if entity.get::<ss13_chat::speech::Impediments>() != Some(&speech) {
        entity.insert(speech);
    }
    if pacifist {
        entity.insert(Pacifist);
    } else {
        entity.remove::<Pacifist>();
    }
    let Some(mut limbs) = world.get::<Limbs>(mob).cloned() else {
        return;
    };
    let mut newly = Vec::new();
    for zone in BodyZone::ALL {
        let limb = limbs.limb_mut(zone);
        if limb.paralyzed != paralyzed[zone.index()] {
            limb.paralyzed = paralyzed[zone.index()];
            if limb.paralyzed {
                newly.push(zone);
            }
        }
    }
    if world.get::<Limbs>(mob) != Some(&limbs) {
        world.entity_mut(mob).insert(limbs);
    }
    // `/obj/item/bodypart/arm/set_disabled`: the hand drops what it holds.
    for zone in newly {
        let hand = match zone {
            BodyZone::LArm => 0,
            BodyZone::RArm => 1,
            _ => continue,
        };
        drop_from_hand(world, mob, hand);
    }
}

/// `dropItemToGround` of what `mob` holds in `hand` (not a `TRAIT_NODROP` item). True if it fell.
pub(super) fn drop_from_hand(world: &mut World, mob: Entity, hand: usize) -> bool {
    let (Some(item), Some(&pos)) = (
        world.get::<Inventory>(mob).and_then(|i| i.hands[hand]),
        world.get::<TilePos>(mob),
    ) else {
        return false;
    };
    if crate::inventory::no_drop(world, item) {
        return false;
    }
    crate::inventory::put_down(world, item, pos);
    true
}

/// Possessiveness lets go of everything it holds on to: the brain left the body or the body
/// died (tg's `relax` timers would still fire; here `on_life` stops).
pub fn let_go(world: &mut World, brain: Entity) {
    if !traumas(world, brain)
        .iter()
        .any(|t| !t.possessed.is_empty())
    {
        return;
    }
    let mut all = world.get::<Traumas>(brain).cloned().unwrap_or_default();
    for trauma in &mut all.0 {
        for (item, _) in std::mem::take(&mut trauma.possessed) {
            release(world, item);
        }
    }
    world.entity_mut(brain).insert(all);
}

/// Possessiveness lets go of `item` (`clear_trait`).
pub(super) fn release(world: &mut World, item: Entity) {
    if let Ok(mut entity) = world.get_entity_mut(item) {
        entity.remove::<NoDrop>();
    }
}

/// tg `on_life` of every trauma in `mob`'s brain, one `SSmobs` fire of `secs` (a living mob).
pub fn life(world: &mut World, mob: Entity, secs: f32) {
    let Some(brain) = organs::organ(world, mob, Slot::Brain) else {
        return;
    };
    let count = traumas(world, brain).len();
    if count == 0 {
        // A fear without traumas (`crate::afflictions` terrified) still ticks.
        if world.get::<super::super::fear::Fearful>(mob).is_some() {
            let mut rng = super::super::fork_rng(world);
            super::super::fear::tick(world, mob, secs, &mut rng);
        }
        return;
    }
    let mut rng = super::super::fork_rng(world);
    for index in 0..count {
        // An effect may have cured or moved the brain.
        let Some(mut trauma) = traumas(world, brain).get(index).cloned() else {
            return;
        };
        if owner(world, brain) != Some(mob) || world.get::<Stat>(mob) == Some(&Stat::Dead) {
            return;
        }
        let suppressed = trauma.suppressed;
        on_life(world, mob, &mut trauma, secs, &mut rng);
        let changed = trauma.suppressed != suppressed;
        if let Some(mut all) = world.get_mut::<Traumas>(brain)
            && let Some(slot) = all.0.get_mut(index)
            && slot.kind == trauma.kind
            && *slot != trauma
        {
            *slot = trauma;
        }
        if changed {
            refresh(world, mob);
        }
    }
    if owner(world, brain) == Some(mob) {
        super::super::fear::tick(world, mob, secs, &mut rng);
    }
}

pub(super) fn conscious(world: &World, mob: Entity) -> bool {
    match (world.get::<Health>(mob), world.get::<Stat>(mob)) {
        (Some(health), Some(&stat)) => !health.knocked_out(stat),
        _ => false,
    }
}

pub(super) fn say_to(world: &mut World, mob: Entity, span: Span, text: impl Into<String>) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        tell(world, client, span, text);
    }
}

pub(super) fn emote(world: &mut World, mob: Entity, key: &str) {
    crate::speech::run_emote(world, mob, key);
}

pub(super) fn on_life(
    world: &mut World,
    mob: Entity,
    trauma: &mut Trauma,
    secs: f32,
    rng: &mut CombatRng,
) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    match trauma.kind {
        Kind::Dumbness => {
            if spt_prob(rng, 1.5, secs) {
                // `/datum/emote/living/drool` (BandaStation's line).
                crate::mobs::defense::emote(world, mob, "пускает слюни.");
            }
        }
        Kind::Concussion => {
            if !spt_prob(rng, 2.5, secs) {
                return;
            }
            match rng.range(1, 11) {
                1 => crate::medical::diseases::vomit(world, mob, 10.0, true, false, 1),
                2 | 3 => crate::status_effects::adjust_timed(
                    world,
                    mob,
                    crate::status_effects::Timed::Dizziness,
                    20.0,
                    None,
                ),
                6..=9 => crate::status_effects::adjust_timed(
                    world,
                    mob,
                    crate::status_effects::Timed::Slurring(
                        crate::status_effects::SlurKind::Generic,
                    ),
                    60.0,
                    None,
                ),
                11 => {
                    say_to(world, mob, Span::Warning, "Вы теряете сознание.");
                    crate::status_effects::apply(world, mob, Incap::Unconscious, 8.0);
                }
                4 | 5 => {
                    if let Some(mut effects) = crate::status_effects::effects_mut(world, mob) {
                        effects.confusion += 10.0;
                    }
                    crate::vision::set_eye_blur_if_lower(world, mob, 20.0);
                }
                10 => {
                    say_to(
                        world,
                        mob,
                        Span::Notice,
                        "На мгновение вы забываете, что делали.",
                    );
                    crate::status_effects::apply(world, mob, Incap::Stun, 2.0);
                }
                _ => {}
            }
        }
        Kind::Healthy => crate::status_effects::adjust_stamina(world, mob, -6.0 * secs),
        Kind::MuscleWeakness => {
            let running = world
                .get::<crate::janitor::Walking>(mob)
                .is_none_or(|w| !w.0);
            let fall_chance = if running { 3.0 } else { 1.0 };
            let standing = world.get::<BodyPosition>(mob).is_some_and(|b| !b.lying());
            let held = world.get::<Inventory>(mob).and_then(Inventory::active_item);
            if spt_prob(rng, 0.5 * fall_chance, secs) && standing {
                say_to(world, mob, Span::Warning, "У вас подкашивается нога!");
                crate::status_effects::apply(world, mob, Incap::Paralyze, 3.5);
            } else if let Some(item) = held {
                let w_class = world.get::<DmWeight>(item).map_or(3, |w| w.w_class);
                let active = world.get::<Inventory>(mob).map_or(0, |i| i.active);
                if spt_prob(rng, 0.5 * (1.0 + f32::from(w_class)), secs)
                    && drop_from_hand(world, mob, active)
                {
                    let name = super::super::name_of(world, item);
                    say_to(world, mob, Span::Warning, format!("Вы роняете {name}!"));
                }
            } else if spt_prob(rng, 1.5, secs) {
                say_to(
                    world,
                    mob,
                    Span::Warning,
                    "Вы чувствуете внезапную слабость в мышцах!",
                );
                crate::status_effects::adjust_stamina(world, mob, 50.0);
            }
        }
        Kind::NervousCough => {
            if !spt_prob(rng, 6.0, secs) {
                return;
            }
            if rng.chance(5.0) {
                let line = if rng.chance(50.0) {
                    "У вас приступ кашля!"
                } else {
                    "Вы не можете перестать кашлять!"
                };
                say_to(world, mob, Span::Warning, line);
                crate::status_effects::apply(world, mob, Incap::Immobilize, 2.0);
                // shortcut: tg spaces these two by 0.6 s timers.
                emote(world, mob, "cough");
                emote(world, mob, "cough");
            }
            emote(world, mob, "cough");
        }
        Kind::Possessive => possessive(world, mob, trauma, now, rng, secs),
        Kind::Hallucinations => hallucinations(world, mob, trauma, secs),
        Kind::Narcolepsy { permanent } => narcolepsy(world, mob, permanent, rng, secs),
        Kind::SplitPersonality => super::super::split_personality::on_life(world, mob, secs, rng),
        Kind::Kleptomaniac => kleptomania(world, mob, trauma, now, rng, secs),
        Kind::Hypnosis => hypnosis_life(world, mob, &trauma.phrase, rng, secs),
        Kind::Obsessed => super::super::obsession::on_life(world, mob, trauma, secs),
        _ => super::super::special_traumas::on_life(world, mob, trauma, secs, rng),
    }
}

/// `/datum/brain_trauma/mild/hallucinations/on_life`: hallucinations up to a minute and
/// Gibbering, unless mindbreaker holds them off (`TRAIT_RDS_SUPPRESSED`).
pub(super) fn hallucinations(world: &mut World, mob: Entity, trauma: &mut Trauma, secs: f32) {
    if crate::status_effects::knocked_out(world, mob) {
        return;
    }
    trauma.suppressed = rds_suppressed(world, mob);
    if trauma.suppressed {
        crate::hallucination::adjust_hallucinations(world, mob, -10.0 * secs);
        return;
    }
    crate::hallucination::adjust_hallucinations_up_to(world, mob, 5.0 * secs, 60.0);
}

/// `TRAIT_RDS_SUPPRESSED`: mindbreaker (or its jellyfish variant) in the blood.
pub fn rds_suppressed(world: &World, mob: Entity) -> bool {
    [
        "/datum/reagent/toxin/mindbreaker",
        "/datum/reagent/toxin/mindbreaker/fish",
    ]
    .iter()
    .any(|path| crate::chem::blood_amount(world, mob, path) > 0.0)
}

/// `/datum/brain_trauma/severe/narcolepsy/on_life`: drowsy first, then asleep; more often while
/// running, while drowsy; never with modafinil or synaptizine in the blood.
// shortcut: no `TRAIT_STIMULATED` (caffeine's halving), no source of it yet.
pub(super) fn narcolepsy(
    world: &mut World,
    mob: Entity,
    permanent: bool,
    rng: &mut CombatRng,
    secs: f32,
) {
    let asleep = world
        .get::<crate::status_effects::StatusEffects>(mob)
        .is_some_and(|e| e.sleeping > 0.0);
    if asleep {
        return;
    }
    let immune = [
        "/datum/reagent/medicine/modafinil",
        "/datum/reagent/medicine/synaptizine",
    ]
    .iter()
    .any(|path| crate::chem::blood_amount(world, mob, path) > 0.0);
    if immune {
        return;
    }
    // `sleep_chance`, `sleep_chance_running`, `sleep_chance_drowsy`, the sleep times.
    let (base, running_add, drowsy_add, sleep_min, sleep_max) = if permanent {
        (0.333, 0.333, 1.0, 20, 30)
    } else {
        (1.0, 2.0, 3.0, 6, 6)
    };
    use crate::status_effects::Timed;
    let drowsy = crate::status_effects::timed(world, mob, Timed::Drowsiness) > 0.0;
    let running = world
        .get::<crate::janitor::Walking>(mob)
        .is_none_or(|w| !w.0);
    let mut chance = base;
    if running {
        chance += running_add;
    }
    if drowsy {
        chance += drowsy_add;
    }
    if !spt_prob(rng, chance, secs) {
        return;
    }
    if !drowsy {
        say_to(world, mob, Span::Warning, "Вы чувствуете усталость...");
        let secs = rng.range(20, 30) as f32;
        crate::status_effects::adjust_timed(world, mob, Timed::Drowsiness, secs, None);
        if rng.chance(50.0) {
            emote(world, mob, "yawn");
        } else if rng.chance(33.0) {
            crate::mobs::defense::emote(world, mob, "трёт свои глаза.");
        }
    } else {
        say_to(world, mob, Span::Warning, "Вы засыпаете.");
        let secs = rng.range(sleep_min, sleep_max) as f32;
        crate::status_effects::apply(world, mob, Incap::Sleeping, secs);
        let asleep = world
            .get::<crate::status_effects::StatusEffects>(mob)
            .is_some_and(|e| e.sleeping > 0.0);
        if rng.chance(50.0) && asleep {
            crate::speech::run_emote(world, mob, "snore");
        }
    }
}

/// `/datum/brain_trauma/mild/possessive/on_life`, `relax`, `clear_trait`.
pub(super) fn possessive(
    world: &mut World,
    mob: Entity,
    trauma: &mut Trauma,
    now: f64,
    rng: &mut CombatRng,
    secs: f32,
) {
    // `COMSIG_ITEM_DROPPED`/`COMSIG_MOVABLE_MOVED` (it left the hand some other way), `relax`.
    let mut kept = Vec::new();
    for (item, until) in std::mem::take(&mut trauma.possessed) {
        let held = world.get::<CarriedBy>(item).is_some_and(|c| c.0 == mob);
        if held && now < until {
            kept.push((item, until));
            continue;
        }
        if held {
            let name = super::super::name_of(world, item);
            say_to(
                world,
                mob,
                Span::Notice,
                format!("Вы чувствуете себя более комфортно, отпуская {name}."),
            );
        }
        release(world, item);
    }
    trauma.possessed = kept;
    if !spt_prob(rng, 5.0, secs) {
        return;
    }
    let hand = rng.range(0, 1) as usize;
    let Some(item) = world.get::<Inventory>(mob).and_then(|i| i.hands[hand]) else {
        return;
    };
    if crate::inventory::no_drop(world, item) {
        return;
    }
    world.entity_mut(item).insert(NoDrop);
    let until = now + f64::from(rng.range(30, 180));
    trauma.possessed.push((item, until));
    let name = super::super::name_of(world, item);
    say_to(
        world,
        mob,
        Span::Warning,
        format!("Вы чувствуете потребность держать {name} при себе..."),
    );
}

/// `WEIGHT_CLASS_BULKY`.
pub(super) const WEIGHT_CLASS_BULKY: u8 = 4;

/// `/datum/brain_trauma/severe/kleptomaniac/on_life`: now and then an empty hand picks up a
/// small item next to the mob, without a message.
pub(super) fn kleptomania(
    world: &mut World,
    mob: Entity,
    trauma: &mut Trauma,
    now: f64,
    rng: &mut CombatRng,
    secs: f32,
) {
    let hands = [0, 1].map(|h| super::super::bodyparts::has_hand(world, mob, h));
    if !hands.contains(&true) || !spt_prob(rng, 5.0, secs) || now < trauma.steal_at {
        return;
    }
    if !conscious(world, mob) || crate::status_effects::incapacitation(world, mob).hands_blocked {
        return;
    }
    let (Some(inventory), Some(&pos)) = (world.get::<Inventory>(mob), world.get::<TilePos>(mob))
    else {
        return;
    };
    // The active hand if it is free, else the other one.
    let active = inventory.active;
    let Some(hand) = [active, 1 - active]
        .into_iter()
        .find(|&h| hands[h] && inventory.hands[h].is_none())
    else {
        return;
    };
    let index = world.resource::<ss13_world::SpatialIndex>();
    let mut stealables: Vec<Entity> = index
        .in_radius(pos, 1)
        .filter(|&e| {
            world.get::<DmItem>(e).is_some_and(DmItem::pickable)
                && world.get::<CarriedBy>(e).is_none()
                && world
                    .get::<DmWeight>(e)
                    .is_none_or(|w| w.w_class < WEIGHT_CLASS_BULKY)
                && world
                    .get::<TilePos>(e)
                    .is_some_and(|&at| index.adjacent(pos, at, |x| x == e))
        })
        .collect();
    stealables.sort_unstable();
    if !stealables.is_empty() {
        let item = stealables[rng.range(0, stealables.len() as u32 - 1) as usize];
        info!("{mob} attempted to pick up {item} (kleptomania)");
        crate::inventory::pick_up(world, mob, item, hand);
    }
    trauma.steal_at = now + 8.0;
}
