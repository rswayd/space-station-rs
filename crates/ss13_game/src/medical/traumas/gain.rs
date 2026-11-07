//! Gaining, losing and curing traumas, and the brain reagents that cure them.

use super::*;

/// The traumas of `mob`'s brain.
pub fn of_mob(world: &World, mob: Entity) -> Vec<Trauma> {
    organs::organ(world, mob, Slot::Brain)
        .map(|brain| traumas(world, brain).to_vec())
        .unwrap_or_default()
}

/// tg `has_trauma_type(type, resilience)` on a mob.
pub fn has(world: &World, mob: Entity, kind: Kind) -> bool {
    of_mob(world, mob).iter().any(|t| t.kind.same_type(kind))
}

/// tg `can_gain_trauma(type, resilience, natural_gain)`: not twice of one type, and a natural
/// gain only below its tier's limit.
pub fn can_gain(
    traumas: &[Trauma],
    kind: Kind,
    resilience: Option<Resilience>,
    natural: bool,
) -> bool {
    let resilience = resilience.unwrap_or(kind.def().resilience);
    if traumas.iter().any(|t| t.kind.same_type(kind)) {
        return false;
    }
    let tier = traumas
        .iter()
        .filter(|t| t.resilience == resilience)
        .count();
    !natural || tier < resilience.limit()
}

/// tg `brain_gain_trauma(type, resilience)`: the trauma goes into `brain`, its owner (if any)
/// gets its effects and the gain line. False if it cannot be gained.
pub fn gain(world: &mut World, brain: Entity, kind: Kind, resilience: Option<Resilience>) -> bool {
    if !can_gain(traumas(world, brain), kind, resilience, false) {
        return false;
    }
    let resilience = resilience.unwrap_or(kind.def().resilience);
    // An imprinted cortex's `COMSIG_CARBON_BLOCK_TRAUMA`.
    if owner(world, brain)
        .is_some_and(|mob| super::super::bioware::blocks_trauma(world, mob, resilience))
    {
        return false;
    }
    // `on_gain` returning FALSE: a split personality needs a living body with a player.
    if kind == Kind::SplitPersonality
        && owner(world, brain)
            .is_some_and(|mob| !super::super::split_personality::can_take(world, mob))
    {
        return false;
    }
    // An imaginary friend: a living body with a player too.
    if kind == Kind::ImaginaryFriend
        && owner(world, brain)
            .is_some_and(|mob| !super::super::imaginary_friend::can_take(world, mob))
    {
        return false;
    }
    // `/datum/brain_trauma/hypnosis/New(phrase)`: no phrase, no trauma ([`gain_hypnosis`]).
    if kind == Kind::Hypnosis {
        return false;
    }
    if kind == Kind::Obsessed {
        let mut rng = super::super::fork_rng(world);
        return gain_obsessed(world, brain, None, Some(resilience), &mut rng);
    }
    let mut trauma = Trauma {
        kind,
        resilience,
        possessed: Vec::new(),
        steal_at: 0.0,
        suppressed: false,
        phrase: String::new(),
        obsession: None,
        extra: Extra::default(),
    };
    if let Some(mob) = owner(world, brain) {
        let mut rng = super::super::fork_rng(world);
        super::super::special_traumas::on_gain(world, mob, &mut trauma, &mut rng);
    }
    let mut all = world.get::<Traumas>(brain).cloned().unwrap_or_default();
    all.0.push(trauma);
    world.entity_mut(brain).insert(all);
    if let Some(mob) = owner(world, brain) {
        info!("{mob} gained the brain trauma {}", kind.def().path);
        tell_owner(world, brain, kind.gain_text());
        refresh(world, mob);
    }
    true
}

/// tg `gain_trauma_type(category, resilience, natural_gain)`: a random gainable type of the
/// category from [`TG_POOL`] (a paralysis of a random kind). The gained type, if any: none
/// when the pick is not ported.
pub fn gain_type(
    world: &mut World,
    brain: Entity,
    category: Category,
    resilience: Option<Resilience>,
    natural: bool,
    rng: &mut CombatRng,
) -> Option<Kind> {
    let have = traumas(world, brain);
    let pool: Vec<Option<Kind>> = TG_POOL
        .into_iter()
        .filter(|&(_, kind, cat, initial)| {
            cat == category
                && match kind {
                    Some(kind) => can_gain(have, kind, resilience, natural),
                    // `can_gain_trauma` of a type that cannot be present: the tier limit only.
                    None => {
                        let tier = resilience.unwrap_or(initial);
                        !natural
                            || have.iter().filter(|t| t.resilience == tier).count() < tier.limit()
                    }
                }
        })
        .map(|(_, kind, _, _)| kind)
        .collect();
    if pool.is_empty() {
        return None;
    }
    let mut kind = pool[rng.range(0, pool.len() as u32 - 1) as usize]?;
    match kind {
        Kind::Paralysis(_) => kind = Kind::Paralysis(Paralysis::ALL[rng.range(0, 8) as usize]),
        // `pick(GLOB.phobia_types)`.
        Kind::Phobia(_) => {
            let random = &super::super::phobia_data::RANDOM;
            let key = random[rng.range(0, random.len() as u32 - 1) as usize];
            kind = Kind::Phobia(super::super::fear::phobia_index(key).unwrap_or(0));
        }
        _ => {}
    }
    gain(world, brain, kind, resilience).then_some(kind)
}

/// The `on_gain` lines of hypnosis, one picked.
pub(super) const HYPNOSIS_LINES: [&str; 5] = [
    "В этом что-то есть... и, по какой-то причине, это кажется правильным. Вам кажется, что вы должны следовать этим словам.",
    "Эти слова продолжают звучать у вас в голове. Вы обнаруживаете, что совершенно очарованы ими.",
    "Вы чувствуете, как часть вашего сознания повторяет это снова и снова. Вам нужно следовать этим словам.",
    "Вы чувствуете, что ваши мысли сосредотачиваются на этой фразе... Кажется, вы не можете выбросить её из головы.",
    "Болит голова, но это все, о чем вы можете думать. Должно быть, это жизненно важно.",
];

/// `gain_trauma(/datum/brain_trauma/hypnosis, resilience, phrase)`: `brain` is hypnotized with
/// `phrase` (trimmed; nothing for an empty one, as `New` deletes itself). False if not gained.
// shortcut: no `/datum/antagonist/hypnotized` (its objective and antag HUD: the antagonists
// group), no hypnosis alert, no highlight of the phrase in what the owner hears (no
// per-hearer speech rewriting; `handle_hearing`).
pub fn gain_hypnosis(
    world: &mut World,
    brain: Entity,
    phrase: &str,
    resilience: Option<Resilience>,
    rng: &mut CombatRng,
) -> bool {
    let phrase = phrase.trim();
    let kind = Kind::Hypnosis;
    if phrase.is_empty() || !can_gain(traumas(world, brain), kind, resilience, false) {
        return false;
    }
    let resilience = resilience.unwrap_or(kind.def().resilience);
    if owner(world, brain)
        .is_some_and(|mob| super::super::bioware::blocks_trauma(world, mob, resilience))
    {
        return false;
    }
    let mut all = world.get::<Traumas>(brain).cloned().unwrap_or_default();
    all.0.push(Trauma {
        kind,
        resilience,
        possessed: Vec::new(),
        steal_at: 0.0,
        suppressed: false,
        phrase: phrase.to_owned(),
        obsession: None,
        extra: Extra::default(),
    });
    world.entity_mut(brain).insert(all);
    let Some(mob) = owner(world, brain) else {
        return true;
    };
    info!("{mob} was hypnotized with the phrase '{phrase}'");
    let line = HYPNOSIS_LINES[rng.range(0, HYPNOSIS_LINES.len() as u32 - 1) as usize];
    tell_owner(world, brain, (Span::Hypnophrase, phrase.to_owned()));
    tell_owner(world, brain, (Span::Notice, line.to_owned()));
    tell_owner(
        world,
        brain,
        (
            Span::BoldWarning,
            "Вы были загипнотизированы этим предложением. Вы должны следовать этим словам. \
             Если это не четкий приказ, вы можете свободно интерпретировать, как именно действовать, \
             главное — вести себя так, будто эти слова являются для вас наивысшим приоритетом."
                .to_owned(),
        ),
    );
    refresh(world, mob);
    true
}

/// `gain_trauma(/datum/brain_trauma/special/obsessed, resilience, obsession)`: `brain` becomes
/// obsessed with `target`, or (none given) with [`super::super::obsession::find_obsession`]'s pick.
/// False if not gained (nobody to obsess over, as `on_gain` returning FALSE).
pub fn gain_obsessed(
    world: &mut World,
    brain: Entity,
    target: Option<Entity>,
    resilience: Option<Resilience>,
    rng: &mut CombatRng,
) -> bool {
    let kind = Kind::Obsessed;
    if !can_gain(traumas(world, brain), kind, resilience, false) {
        return false;
    }
    let resilience = resilience.unwrap_or(kind.def().resilience);
    let Some(mob) = owner(world, brain) else {
        return false;
    };
    if super::super::bioware::blocks_trauma(world, mob, resilience) {
        return false;
    }
    let Some(target) = target
        .filter(|&t| t != mob && world.get_entity(t).is_ok())
        .or_else(|| super::super::obsession::find_obsession(world, mob, &[], rng))
    else {
        return false;
    };
    let mut all = world.get::<Traumas>(brain).cloned().unwrap_or_default();
    all.0.push(Trauma {
        kind,
        resilience,
        possessed: Vec::new(),
        steal_at: 0.0,
        suppressed: false,
        phrase: String::new(),
        obsession: Some(super::super::obsession::Obsession::new(target)),
        extra: Extra::default(),
    });
    world.entity_mut(brain).insert(all);
    info!("{mob} has developed an obsession with {target}");
    tell_owner(world, brain, kind.gain_text());
    refresh(world, mob);
    true
}

/// `set_obsession(new_obsession)` (BandaStation): a new obsession for `mob`'s trauma, its
/// progress reset. False without the trauma.
pub fn set_obsession(world: &mut World, mob: Entity, target: Entity) -> bool {
    let Some(brain) = organs::organ(world, mob, Slot::Brain) else {
        return false;
    };
    let Some(mut all) = world.get::<Traumas>(brain).cloned() else {
        return false;
    };
    let Some(trauma) = all.0.iter_mut().find(|t| t.kind == Kind::Obsessed) else {
        return false;
    };
    trauma.obsession = Some(super::super::obsession::Obsession::new(target));
    world.entity_mut(brain).insert(all);
    crate::mood::clear_event(world, mob, super::super::obsession::CATEGORY);
    true
}

/// The phrase `mob`'s brain is hypnotized with, if any.
pub fn hypnotic_phrase(world: &World, mob: Entity) -> Option<String> {
    of_mob(world, mob)
        .into_iter()
        .find(|t| t.kind == Kind::Hypnosis)
        .map(|t| t.phrase)
}

/// `/datum/brain_trauma/hypnosis/on_life`: 1% a second the phrase comes back, as a thought or
/// as a voice on the radio (a chat hallucination).
pub(super) fn hypnosis_life(
    world: &mut World,
    mob: Entity,
    phrase: &str,
    rng: &mut CombatRng,
    secs: f32,
) {
    if !spt_prob(rng, 1.0, secs) {
        return;
    }
    if rng.chance(50.0) {
        say_to(
            world,
            mob,
            Span::Hypnophrase,
            format!("...{}...", phrase.to_lowercase()),
        );
    } else {
        crate::hallucination::kinds::chat(world, mob, true, Some(phrase.to_owned()));
    }
}

/// Takes the trauma at `index` out of `brain` (tg `qdel(trauma)` -> `on_lose`).
pub(super) fn lose(world: &mut World, brain: Entity, index: usize) {
    let Some(mut all) = world.get::<Traumas>(brain).cloned() else {
        return;
    };
    let trauma = all.0.remove(index);
    world.entity_mut(brain).insert(all);
    for (item, _) in &trauma.possessed {
        release(world, *item);
    }
    // `/datum/brain_trauma/mild/hallucinations/on_lose`.
    if trauma.kind == Kind::Hallucinations
        && let Some(mob) = owner(world, brain)
    {
        crate::hallucination::set_hallucinations(world, mob, 0.0, false);
    }
    if let Some(mob) = owner(world, brain) {
        info!("{mob} lost the brain trauma {}", trauma.kind.def().path);
        if trauma.kind == Kind::Obsessed {
            crate::mood::clear_event(world, mob, super::super::obsession::CATEGORY);
        }
        super::super::special_traumas::on_lose(world, Some(mob), &trauma);
        if trauma.kind == Kind::Hypnosis {
            info!(
                "{mob} is no longer hypnotized with the phrase '{}'",
                trauma.phrase
            );
            tell_owner(
                world,
                brain,
                (
                    Span::Danger,
                    format!(
                        "Вы внезапно выходите из состояния гипноза. Фраза '{}' больше не кажется вам важной.",
                        trauma.phrase
                    ),
                ),
            );
        }
        tell_owner(world, brain, trauma.kind.lose_text());
        refresh(world, mob);
    }
}

/// `qdel` of `brain`'s trauma of `kind`'s type (whatever its resilience), if any.
pub fn remove_kind(world: &mut World, brain: Entity, kind: Kind) {
    if let Some(index) = traumas(world, brain)
        .iter()
        .position(|t| t.kind.same_type(kind))
    {
        lose(world, brain, index);
    }
}

/// tg `TRAIT_ILLITERATE`: the Illiterate quirk or dyslexia.
pub fn illiterate(world: &World, mob: Entity) -> bool {
    crate::quirks::has(world, mob, crate::quirks::Quirk::Illiterate)
        || has(world, mob, Kind::Dyslexia)
}

/// tg `cure_trauma_type(resilience)`: one random trauma of at most `resilience`.
pub fn cure_one(world: &mut World, brain: Entity, resilience: Resilience, rng: &mut CombatRng) {
    let curable: Vec<usize> = traumas(world, brain)
        .iter()
        .enumerate()
        .filter(|(_, t)| t.resilience <= resilience)
        .map(|(i, _)| i)
        .collect();
    if !curable.is_empty() {
        let index = curable[rng.range(0, curable.len() as u32 - 1) as usize];
        lose(world, brain, index);
    }
}

/// tg `cure_all_traumas(resilience)`: every trauma of at most `resilience`; how many.
pub fn cure_all(world: &mut World, brain: Entity, resilience: Resilience) -> usize {
    let mut cured = 0;
    while let Some(index) = traumas(world, brain)
        .iter()
        .position(|t| t.resilience <= resilience)
    {
        lose(world, brain, index);
        cured += 1;
    }
    cured
}

/// tg `roll_for_brain_trauma(delta)` after `delta` more brain damage brought it to `damage`
/// (above [`organs::BRAIN_DAMAGE_MILD`]).
pub fn roll(world: &mut World, brain: Entity, delta: f32, damage: f32) {
    let mut rng = super::super::fork_rng(world);
    let mild = organs::BRAIN_DAMAGE_MILD;
    if rng.chance(delta * (1.0 + ((damage - mild) / 100.0).max(0.0))) {
        gain_type(world, brain, Category::Mild, None, true, &mut rng);
    }
    let severe = organs::BRAIN_DAMAGE_SEVERE;
    if damage < severe {
        return;
    }
    if rng.chance(delta * (1.0 + ((damage - severe) / 100.0).max(0.0))) {
        let category = if rng.chance(20.0) {
            Category::Special
        } else {
            Category::Severe
        };
        gain_type(world, brain, category, None, true, &mut rng);
    }
}

/// `/datum/reagent/medicine/mannitol`, `/datum/reagent/medicine/neurine`.
pub const MANNITOL: &str = "/datum/reagent/medicine/mannitol";
pub const NEURINE: &str = "/datum/reagent/medicine/neurine";

/// The brain reagents in `mob`'s blood for one metabolism tick of `secs`: mannitol heals the
/// brain (`on_mob_life`, 1 a second), neurine cures a basic trauma now and then (8% a second)
/// and, in a corpse, heals the brain (`on_mob_dead`, 0.5 a second).
// shortcut: a metabolization ratio of 1 and full purity; no mannitol overdose tips, no
// neurine anticonvulsant, neurotoxin purge or `initial_bdamage` restore.
pub fn brain_reagents(world: &mut World, mob: Entity, secs: f32) {
    let dead = world.get::<Stat>(mob) == Some(&Stat::Dead);
    if !dead && crate::chem::blood_amount(world, mob, MANNITOL) > 0.0 {
        organs::adjust_organ_loss(world, mob, Slot::Brain, -secs);
    }
    if crate::chem::blood_amount(world, mob, NEURINE) <= 0.0 {
        return;
    }
    if dead {
        organs::adjust_organ_loss(world, mob, Slot::Brain, -0.5 * secs);
        return;
    }
    let mut rng = super::super::fork_rng(world);
    if spt_prob(&mut rng, 8.0, secs)
        && let Some(brain) = organs::organ(world, mob, Slot::Brain)
    {
        cure_one(world, brain, Resilience::Basic, &mut rng);
    }
}
