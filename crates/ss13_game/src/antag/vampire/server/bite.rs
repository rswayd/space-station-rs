//! Biting: draining a victim's blood.

use super::*;

/// `vampire_biter/try_bite`: empty-handed clicks in combat mode on a human's head, before melee.
pub(super) fn bite_clicks(
    world: &mut World,
    mut clicks: Local<MessageCursor<crate::click::ClickResolved>>,
) {
    let clicks: Vec<crate::click::ClickResolved> = clicks
        .read(world.resource::<Messages<crate::click::ClickResolved>>())
        .copied()
        .collect();
    for click in clicks {
        let (mob, target) = (click.mob, click.target);
        if world
            .resource::<crate::security::MeleeHandled>()
            .0
            .contains(&mob)
        {
            continue;
        }
        let Some(vampire) = world.get::<Vampire>(mob) else {
            continue;
        };
        if vampire.draining.is_some() || target == mob || click.modifiers.right {
            continue;
        }
        let combat = world.get::<CombatMode>(mob).is_some_and(|c| c.0);
        let held = world.get::<Inventory>(mob).and_then(Inventory::active_item);
        if !combat || held.is_some() || zone(world, mob) != BodyZone::Head {
            continue;
        }
        if world.get::<Blood>(target).is_none() || world.get::<Health>(target).is_none() {
            continue;
        }
        world
            .resource_mut::<crate::security::MeleeHandled>()
            .0
            .insert(mob);
        // tg `ClickOn` -> `face_atom` before the unarmed attack.
        if let Some(dir) = dir_to(click.mob_pos, click.target_pos) {
            world.entity_mut(mob).insert(Facing(dir));
        }
        start_bite(world, click.client, mob, target);
    }
}

pub(super) fn zone(world: &World, mob: Entity) -> BodyZone {
    world
        .get::<ZoneSelected>(mob)
        .copied()
        .unwrap_or_default()
        .0
        .check_zone()
}

/// `can_bloodsuck`: every condition of starting, holding and draining a bite. Dead victims
/// are valid (only the usable blood needs a living one).
pub(super) fn can_bloodsuck(
    world: &mut World,
    mob: Entity,
    victim: Entity,
    feedback: bool,
) -> bool {
    let (Some(&at), Some(&victim_at)) = (world.get::<TilePos>(mob), world.get::<TilePos>(victim))
    else {
        return false;
    };
    if victim == mob || world.get::<Vampire>(mob).is_none() {
        return false;
    }
    if !world
        .resource::<SpatialIndex>()
        .adjacent(at, victim_at, |e| e == victim || e == mob)
    {
        return false;
    }
    let combat = world.get::<CombatMode>(mob).is_some_and(|c| c.0);
    let held = world.get::<Inventory>(mob).and_then(Inventory::active_item);
    let traits = incapacitation(world, mob);
    let conscious = world.get::<Stat>(mob) == Some(&Stat::Conscious);
    if !combat
        || zone(world, mob) != BodyZone::Head
        || held.is_some()
        || traits.incapacitated
        || !conscious
        // `can_unarmed_attack`: `TRAIT_HANDS_BLOCKED`.
        || traits.hands_blocked
    {
        return false;
    }
    let victim_name = name_of(world, victim);
    // shortcut: anything in the mask slot covers the mouth (tg `is_mouth_covered`).
    if world
        .get::<Inventory>(mob)
        .is_some_and(|i| i.mask.is_some())
    {
        if feedback {
            chat_to(
                world,
                mob,
                Span::Warning,
                format!("Ваша маска или намордник не позволяют укусить {victim_name}!"),
            );
        }
        return false;
    }
    if world.get::<Blood>(victim).is_none_or(|b| b.volume <= 0.0) {
        if feedback {
            chat_to(
                world,
                mob,
                Span::Warning,
                format!("В {victim_name} нет крови!"),
            );
        }
        return false;
    }
    // `TRAIT_VAMPIRE_LIKE`.
    if world.get::<Vampire>(victim).is_some() {
        if feedback {
            chat_to(
                world,
                mob,
                Span::Warning,
                format!("Ваши клыки не могут пронзить холодную плоть {victim_name}!"),
            );
        }
        return false;
    }
    true
}

/// `handle_bloodsucking`: the bite starts and its first `do_after`.
pub(super) fn start_bite(world: &mut World, client: Entity, mob: Entity, victim: Entity) {
    if !can_bloodsuck(world, mob, victim, true) || world.get::<DoAfter>(mob).is_some() {
        return;
    }
    let (user, target) = (name_of(world, mob), name_of(world, victim));
    visible(
        world,
        mob,
        &format!("Вы вонзаете клыки в {target} и начинаете высасывать его кровь."),
        &format!("{user} грубо хватает {target} за шею и вонзает свои клыки!"),
        Span::Danger,
    );
    info!("{mob} bites {victim} (vampire)");
    if let Some(mut vampire) = world.get_mut::<Vampire>(mob) {
        vampire.draining = Some(victim);
    }
    next_bite(world, client, mob, victim);
}

pub(super) fn next_bite(world: &mut World, client: Entity, mob: Entity, victim: Entity) {
    let actor = Actor {
        client,
        mob,
        item: None,
    };
    if !do_after(
        world,
        actor,
        victim,
        SUCK_RATE_DS,
        0,
        Job::Vampire(VampireJob::Bite),
    ) {
        stop_bite(world, mob);
    }
}

/// The end of the loop: "Вы прекращаете высасывать кровь из ...".
pub(super) fn stop_bite(world: &mut World, mob: Entity) {
    let Some(victim) = world
        .get_mut::<Vampire>(mob)
        .and_then(|mut v| v.draining.take())
    else {
        return;
    };
    if world.get_entity(victim).is_ok() {
        let name = name_of(world, victim);
        chat_to(
            world,
            mob,
            Span::Notice,
            format!("Вы прекращаете высасывать кровь из {name}."),
        );
    }
}

/// `can_gain_usable_blood_from`: alive, under the limit, and a player's body (tg: a `ckey` or a
/// ghost; here a crew body).
pub(super) fn can_gain_from(world: &World, vampire: &Vampire, victim: Entity) -> bool {
    world.get::<Stat>(victim).is_some_and(|s| *s != Stat::Dead)
        && vampire.drained_from(victim) < BLOOD_DRAIN_LIMIT
        && world.get::<PlayerJob>(victim).is_some()
}

/// `bloodsuck_tick`: the blood physically drained (from corpses too).
pub fn bloodsuck_tick(world: &mut World, mob: Entity, victim: Entity, drain: f32) -> f32 {
    if drain <= 0.0 || !can_bloodsuck(world, mob, victim, true) {
        return 0.0;
    }
    let drained = world
        .get_mut::<Blood>(victim)
        .map_or(0.0, |mut b| -b.adjust(-drain, 0.0, BLOOD_VOLUME_MAXIMUM));
    if drained <= 0.0 {
        return 0.0;
    }
    let eligible = world
        .get::<Vampire>(mob)
        .is_some_and(|v| can_gain_from(world, v, victim));
    let mut gain = if eligible {
        drained.min(BITE_GAIN_CAP) * BLOOD_GAINED_MODIFIER
    } else {
        0.0
    };
    if gain > 0.0 {
        gain = gain_blood(world, mob, Some(victim), gain);
        let (total, usable) = world
            .get::<Vampire>(mob)
            .map(|v| (v.bloodtotal, v.bloodusable))
            .unwrap_or_default();
        chat_to(
            world,
            mob,
            Span::BoldNotice,
            format!(
                "Вы накопили {} ед. крови; для использования осталось {}.",
                blood_text(total),
                blood_text(usable)
            ),
        );
    } else {
        let name = name_of(world, victim);
        chat_to(
            world,
            mob,
            Span::BoldNotice,
            format!("Питание кровью {name} утоляет ваш голод, но не даёт доступной крови."),
        );
    }
    // `set_nutrition(min(NUTRITION_LEVEL_WELL_FED, nutrition + (gain || 5)), forced = TRUE)`.
    if let Some(mut hunger) = world.get_mut::<Hunger>(mob) {
        let fed = if gain > 0.0 { gain } else { 5.0 };
        hunger.nutrition = NUTRITION_LEVEL_WELL_FED.min(hunger.nutrition + fed);
    }
    drained
}

/// The end of a bite's `do_after`: a drain, the victim's blood warnings, the next bite.
pub(super) fn complete_bite(world: &mut World, mob: Entity, action: &DoAfter) {
    let victim = action.target;
    if world.get::<Vampire>(mob).and_then(|v| v.draining) != Some(victim) {
        return;
    }
    let before = world.get::<Blood>(victim).map_or(0.0, |b| b.volume);
    if bloodsuck_tick(world, mob, victim, DRAIN_AMOUNT) <= 0.0 {
        stop_bite(world, mob);
        return;
    }
    let volume = world.get::<Blood>(victim).map_or(0.0, |b| b.volume);
    if volume <= 0.0 {
        chat_to(world, mob, Span::Warning, "Вы обескровили свою жертву!");
        stop_bite(world, mob);
        return;
    }
    if volume <= BLOOD_VOLUME_BAD && before > BLOOD_VOLUME_BAD {
        chat_to(
            world,
            mob,
            Span::Danger,
            "Объём крови вашей жертвы опасно низок.",
        );
    } else if volume <= BLOOD_VOLUME_OKAY && before > BLOOD_VOLUME_OKAY {
        chat_to(
            world,
            mob,
            Span::Warning,
            "У вашей жертвы слишком мало крови!",
        );
    }
    next_bite(world, action.client, mob, victim);
}

/// [`Job::Vampire`] completions.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: VampireJob) {
    match job {
        VampireJob::Bite => complete_bite(world, mob, action),
        VampireJob::Lair => complete_lair(world, mob, action.target),
    }
}
