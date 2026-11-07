//! Casting the vampire's spells.

use super::*;

/// Button clicks: tg `Trigger` of the vampire's spells.
pub(super) fn trigger_spells(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>,
) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        let Some(spell) = Spell::from_id(&id) else {
            continue;
        };
        // The client must still control the vampire (not its ghost through a stale window).
        if actor != source
            || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
            || world
                .get::<Vampire>(actor)
                .and_then(|v| v.spell(spell))
                .is_none()
        {
            continue;
        }
        if spell.def().range.is_some() {
            toggle_aim(world, client, actor, spell);
        } else {
            cast(world, client, actor, spell, None);
        }
    }
}

/// A pointed spell's button: aiming on (after the cast checks) or off.
pub(super) fn toggle_aim(world: &mut World, client: Entity, mob: Entity, spell: Spell) {
    let aiming = world.get::<Vampire>(mob).and_then(|v| v.aiming);
    if aiming == Some(spell) {
        set_aim(world, mob, None);
        return;
    }
    if !checked(world, client, mob, spell) {
        return;
    }
    set_aim(world, mob, Some(spell));
}

pub(super) fn set_aim(world: &mut World, mob: Entity, spell: Option<Spell>) {
    if let Some(mut vampire) = world.get_mut::<Vampire>(mob) {
        vampire.aiming = spell;
    }
    if spell.is_some() {
        world.entity_mut(mob).insert(ClickIntercept);
    } else {
        world.entity_mut(mob).remove::<ClickIntercept>();
    }
    refresh(world, mob);
}

/// The cast checks with their feedback.
pub(super) fn checked(world: &mut World, client: Entity, mob: Entity, spell: Spell) -> bool {
    let ctx = cast_context(world, mob);
    let Some(result) = world.get::<Vampire>(mob).map(|v| v.cast_check(spell, ctx)) else {
        return false;
    };
    match result {
        Ok(()) => true,
        Err(CastFail::Message(text)) => {
            tell(world, client, Span::Warning, text);
            false
        }
        Err(_) => false,
    }
}

/// Clicks of an aiming vampire: tg pointed `InterceptClickOn` (`cast_range`, `is_valid_target`);
/// a right click stops aiming.
pub(super) fn pointed_clicks(
    world: &mut World,
    mut clicks: Local<MessageCursor<InterceptedClick>>,
) {
    let clicks: Vec<InterceptedClick> = clicks
        .read(world.resource::<Messages<InterceptedClick>>())
        .copied()
        .collect();
    for click in clicks {
        let Some(spell) = world.get::<Vampire>(click.mob).and_then(|v| v.aiming) else {
            // Not a vampire's intercept.
            continue;
        };
        if click.right {
            set_aim(world, click.mob, None);
            continue;
        }
        let range = spell.def().range.unwrap_or(0);
        if click
            .mob_pos
            .chebyshev(click.target_pos)
            .is_none_or(|d| d > range)
        {
            tell(world, click.client, Span::Warning, "Слишком далеко!");
            continue;
        }
        if !valid_target(world, click.mob, spell, click.target) {
            continue;
        }
        if cast(world, click.client, click.mob, spell, Some(click.target)) {
            set_aim(world, click.mob, None);
        }
    }
}

/// `is_valid_target`.
pub(super) fn valid_target(world: &World, mob: Entity, spell: Spell, target: Entity) -> bool {
    match spell {
        Spell::Lair => path_of(world, target).is_some_and(|p| is_under(&p, COFFIN)),
        Spell::SwitchPlaces => target != mob && world.get::<Health>(target).is_some(),
        _ => true,
    }
}

/// tg spell `Trigger` -> `Activate`: the checks, `before_cast`, the blood (`COMSIG_SPELL_CAST`),
/// `cast`, the cooldown and `after_cast`. True when it was cast.
pub(super) fn cast(
    world: &mut World,
    client: Entity,
    mob: Entity,
    spell: Spell,
    target: Option<Entity>,
) -> bool {
    if !checked(world, client, mob, spell) || !before_cast(world, client, mob, spell, target) {
        return false;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    let def = spell.def();
    let cost = world
        .get::<Vampire>(mob)
        .map_or(0, |v| v.blood_cost(def.cost));
    if let Some(mut vampire) = world.get_mut::<Vampire>(mob) {
        if def.cost > 0 {
            vampire.subtract_usable(cost as f32);
        }
        if let Some(state) = vampire.spell_mut(spell) {
            state.ready_at = now + def.cooldown;
        }
        if spell == Spell::Glare {
            vampire.spend_glare_charge(now);
        }
    }
    info!("{mob} casts {spell:?} (vampire)");
    match spell {
        Spell::Rejuvenate => rejuvenate(world, mob, now),
        Spell::Glare => glare(world, mob),
        Spell::Lair => {
            if let Some(coffin) = target {
                begin_lair(world, client, mob, coffin);
            }
        }
        Spell::Cloak => toggle_cloak(world, mob),
        Spell::BloodRush => blood_rush(world, mob, now),
        Spell::SwitchPlaces => {
            if let Some(target) = target {
                switch_places(world, mob, target);
            }
        }
    }
    // `vampire_ability/after_cast`.
    if def.cost > 0 {
        let usable = world.get::<Vampire>(mob).map_or(0.0, |v| v.bloodusable);
        chat_to(
            world,
            mob,
            Span::BoldNotice,
            format!(
                "У вас осталось {} единиц доступной крови.",
                blood_text(usable)
            ),
        );
    }
    refresh(world, mob);
    true
}

/// The spells' own `can_cast_spell`/`before_cast` checks; false cancels without cost.
pub(super) fn before_cast(
    world: &mut World,
    client: Entity,
    mob: Entity,
    spell: Spell,
    target: Option<Entity>,
) -> bool {
    match spell {
        Spell::BloodRush => {
            let knocked_down = world
                .get::<crate::status_effects::StatusEffects>(mob)
                .is_some_and(|e| e.knockdown > 0.0);
            if knocked_down || world.get::<crate::medical::surgery::Buckled>(mob).is_some() {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "Нельзя использовать это в беспомощном состоянии!",
                );
                return false;
            }
            true
        }
        Spell::Lair => {
            let Some(coffin) = target else {
                return false;
            };
            let name = name_of(world, coffin);
            if world.get::<VampireCoffin>(coffin).is_some() {
                tell(
                    world,
                    client,
                    Span::Warning,
                    format!("{name} служит другому и отказывается подчиняться вашей воле!"),
                );
                return false;
            }
            let Some(&at) = world.get::<TilePos>(coffin) else {
                return false;
            };
            let dense = (-1..=1)
                .flat_map(|dx| (-1..=1).map(move |dy| (dx, dy)))
                .any(|(dx, dy)| {
                    let pos = TilePos::new(at.x + dx, at.y + dy, at.z);
                    turf_at(world, pos)
                        .is_some_and(|t| world.get::<ss13_world::Density>(t).is_some())
                });
            if dense {
                tell(
                    world,
                    client,
                    Span::Warning,
                    format!("Для ритуала вокруг {name} нужно больше места!"),
                );
                return false;
            }
            true
        }
        _ => true,
    }
}

/// `vampire_rejuvenate/cast`: every stun off, stamina reset, standing up; with the regeneration
/// passive 14 s of healing.
pub(super) fn rejuvenate(world: &mut World, mob: Entity, now: f64) {
    if let Some(mut effects) = effects_mut(world, mob) {
        for kind in [
            Incap::Stun,
            Incap::Knockdown,
            Incap::Paralyze,
            Incap::Immobilize,
        ] {
            effects.set(kind, 0.0);
        }
        // `set_stamina_loss(0)`.
        effects.stamina = 0.0;
        effects.stamcrit = false;
    }
    chat_to(
        world,
        mob,
        Span::Notice,
        "Вы наполняете тело чистой кровью и снимаете все обездвиживающие эффекты.",
    );
    let Some(vampire) = world.get::<Vampire>(mob) else {
        return;
    };
    // `get_rejuv_mult`.
    if !vampire.has(Power::Regen) {
        return;
    }
    let health = world.get::<Health>(mob).map_or(100.0, Health::health);
    let mult = if vampire
        .subclass
        .is_some_and(Subclass::improved_rejuv_healing)
    {
        ((100.0 - health) / 20.0).clamp(1.0, 5.0)
    } else {
        1.0
    };
    if world.get::<Stat>(mob) == Some(&Stat::Dead) {
        return;
    }
    if let Some(mut vampire) = world.get_mut::<Vampire>(mob) {
        vampire.rejuvenation = Some((now + REJUVENATION_SECS, mult));
    }
}

/// `get_dir` of BYOND: the signs of the offsets.
pub(super) fn dir_to(from: TilePos, to: TilePos) -> Option<Dir> {
    let (dx, dy) = ((to.x - from.x).signum(), (to.y - from.y).signum());
    Dir::DMI_ORDER.into_iter().find(|d| d.offset() == (dx, dy))
}

/// The glare's `calculate_deviation`: 0 none (the victim is in front), 1 partial (beside),
/// 2 full (behind).
pub fn glare_deviation(attacker: TilePos, facing: Dir, victim: TilePos) -> u8 {
    let Some(to_victim) = dir_to(attacker, victim) else {
        // The same tile: looking down is easy.
        return 0;
    };
    let (f, v) = (facing as u8, to_victim as u8);
    if f & v != 0 {
        0
    } else if f & to_victim.reverse() as u8 != 0 {
        2
    } else {
        1
    }
}

/// tg `affects_vampire`: other vampires never; holy people (the chaplain's `TRAIT_HOLY`) only to
/// a vampire at full power.
pub(super) fn affects_vampire(world: &World, target: Entity, vampire: Entity) -> bool {
    if world.get::<Vampire>(target).is_some() {
        return false;
    }
    let full = world
        .get::<Vampire>(vampire)
        .is_some_and(|v| v.has(Power::Full));
    let holy = world
        .get::<PlayerJob>(target)
        .and_then(|j| world.get_resource::<Jobs>()?.data.jobs.get(j.0))
        .is_some_and(|j| j.title == "Chaplain");
    full || !holy
}

/// `vampire_glare/cast` and `cast_on_thing_in_aoe` over the mobs around (radius 1).
pub(super) fn glare(world: &mut World, mob: Entity) {
    let Some(&at) = world.get::<TilePos>(mob) else {
        return;
    };
    let blindfolded = world
        .get::<Inventory>(mob)
        .and_then(|i| i.eyes)
        .and_then(|e| world.get::<DmItem>(e))
        .is_some_and(|dm| is_under(&dm.path, BLINDFOLD));
    if blindfolded {
        chat_to(world, mob, Span::Warning, "На ваших глазах повязка!");
        return;
    }
    let user = name_of(world, mob);
    let text = format!("Глаза {user} испускают ослепительную вспышку!");
    visible(world, mob, &text, &text, Span::Warning);
    let lying = world.get::<BodyPosition>(mob).is_some_and(|b| b.lying());
    let facing = world.get::<Facing>(mob).map_or(Dir::South, |f| f.0);
    let targets: Vec<(Entity, TilePos)> = world
        .resource::<SpatialIndex>()
        .in_radius(at, 1)
        .filter(|&e| e != mob)
        // `orange(1, user)` holds turf contents, not mobs inside a closet there.
        .filter(|&e| {
            world.get::<Health>(e).is_some() && world.get::<crate::closets::InCloset>(e).is_none()
        })
        .filter_map(|e| Some((e, *world.get::<TilePos>(e)?)))
        .collect();
    for (target, pos) in targets {
        if !affects_vampire(world, target, mob) {
            continue;
        }
        let deviation = if lying {
            1
        } else {
            glare_deviation(at, facing, pos)
        };
        let (confusion, stamina, knockdown) = match deviation {
            2 => (6.0, 20.0, 0.0),
            1 => (6.0, 40.0, 5.0),
            _ => (10.0, 70.0, 12.0),
        };
        if let Some(mut effects) = effects_mut(world, target) {
            // `adjust_confusion`, `AdjustKnockdown`.
            effects.confusion += confusion;
            if knockdown > 0.0 {
                effects.adjust(Incap::Knockdown, knockdown);
            }
            // A full glare also silences (`adjust_silence(8 SECONDS)`).
            if deviation == 0 {
                effects.adjust_timed(crate::status_effects::Timed::Silence, 8.0, None);
            }
        }
        crate::status_effects::adjust_stamina(world, target, stamina);
        chat_to(
            world,
            target,
            Span::Warning,
            format!("Вас ослепляет взгляд {user}."),
        );
        info!("{mob} glared at {target} (vampire)");
    }
}

/// The lair ritual's start (`vampire_lair/cast`).
pub(super) fn begin_lair(world: &mut World, client: Entity, mob: Entity, coffin: Entity) {
    let name = name_of(world, coffin);
    chat_to(
        world,
        mob,
        Span::Danger,
        format!("Вы начинаете помечать {name}!"),
    );
    do_after(
        world,
        Actor {
            client,
            mob,
            item: None,
        },
        coffin,
        LAIR_RITUAL_DS,
        0,
        Job::Vampire(VampireJob::Lair),
    );
}

/// The ritual done: the coffin is the vampire's (anchored), the lair objective is met and the
/// spell goes away (`COMSIG_VAMPIRE_ABILITY_COMPLETE_LAIR`).
pub(super) fn complete_lair(world: &mut World, mob: Entity, coffin: Entity) {
    if world.get::<VampireCoffin>(coffin).is_some()
        || !path_of(world, coffin).is_some_and(|p| is_under(&p, COFFIN))
    {
        return;
    }
    world
        .entity_mut(coffin)
        .insert((VampireCoffin(mob), Anchored(true)));
    if let Some(mut vampire) = world.get_mut::<Vampire>(mob) {
        vampire.has_lair = true;
        // `remove_spell_ability`: the tier is removed, so it is never granted again.
        vampire.powers.retain(|p| *p != Power::Lair);
        vampire.spells.retain(|s| s.spell != Spell::Lair);
        vampire.spent.push(Power::Lair);
        vampire.aiming = None;
    }
    world.entity_mut(mob).remove::<ClickIntercept>();
    info!("{mob} made {coffin} its lair (vampire)");
    refresh(world, mob);
}

/// `vampire_cloak/cast`.
pub(super) fn toggle_cloak(world: &mut World, mob: Entity) {
    let Some(on) = world.get_mut::<Vampire>(mob).map(|mut v| {
        v.cloaking = !v.cloaking;
        v.cloaking
    }) else {
        return;
    };
    chat_to(
        world,
        mob,
        Span::Notice,
        format!(
            "Теперь во тьме вас будут {}.",
            if on {
                "не замечать"
            } else {
                "видеть"
            }
        ),
    );
    handle_cloak(world, mob);
}

/// `vampire_blood_rush/cast`: faster for 10 s, knockdown off, standing up.
pub(super) fn blood_rush(world: &mut World, mob: Entity, now: f64) {
    chat_to(world, mob, Span::Notice, "Вы чувствуете прилив энергии!");
    if let Some(mut vampire) = world.get_mut::<Vampire>(mob) {
        vampire.rush_until = now + BLOOD_RUSH_SECS;
    }
    if let Some(mut effects) = effects_mut(world, mob) {
        effects.set(Incap::Knockdown, 0.0);
    }
    update_speed(world, mob, now);
}

/// `vampire_switch_places/cast`: the two swap tiles.
pub(super) fn switch_places(world: &mut World, mob: Entity, target: Entity) {
    let (Some(&a), Some(&b)) = (world.get::<TilePos>(mob), world.get::<TilePos>(target)) else {
        return;
    };
    world.entity_mut(target).insert(a);
    world.entity_mut(mob).insert(b);
}
