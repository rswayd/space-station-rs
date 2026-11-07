//! Life: the cloak, the sun, holy water and the timed effects.

use super::*;

/// The cloak and the blood rush as one movespeed modifier.
pub(super) fn update_speed(world: &mut World, mob: Entity, now: f64) {
    let Some(vampire) = world.get::<Vampire>(mob) else {
        return;
    };
    let mut speed = 0.0;
    if vampire.cloaking && vampire.cloak_fast {
        speed += CLOAK_SPEED;
    }
    if now < vampire.rush_until {
        speed += BLOOD_RUSH_SPEED;
    }
    if let Some(mut modifier) = world.get_mut::<MoveSpeedModifier>(mob) {
        modifier.set_if_neq(MoveSpeedModifier(speed));
    }
}

/// `handle_vampire_cloak`: alpha 40 and faster at `lumcount * 10 <= 2`, else alpha 200; off
/// while burning or not cloaking.
pub(super) fn handle_cloak(world: &mut World, mob: Entity) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let burning = crate::fire::on_fire(world, mob);
    let cloaking = world.get::<Vampire>(mob).is_some_and(|v| v.cloaking);
    let (alpha, fast) = if !cloaking || burning {
        (None, false)
    } else {
        // A tile without lighting is never dark (tg `get_lumcount` of fullbright areas is 1).
        let light = crate::botany::lumcount(world, pos).unwrap_or(1.0) * 10.0;
        if light <= 2.0 {
            (Some(40), true)
        } else {
            (Some(200), false)
        }
    };
    if let Some(mut vampire) = world.get_mut::<Vampire>(mob) {
        vampire.cloak_alpha = alpha;
        vampire.cloak_fast = fast;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    update_speed(world, mob, now);
}

/// The cloak's alpha on the mob, again whenever its appearance was rebuilt (clothes).
pub(super) fn cloak_alpha(
    mut mobs: Query<(&Vampire, &mut Appearance), Or<(Changed<Vampire>, Changed<Appearance>)>>,
) {
    for (vampire, mut appearance) in &mut mobs {
        let color = vampire.cloak_alpha.map(|a| [255, 255, 255, a]);
        if appearance.color != color {
            appearance.color = color;
        }
    }
}

/// Per tick: the glare's recharges, the end of the blood rush, a bite whose `do_after` broke.
// Cost: one pass over the vampires (a few per round).
pub(super) fn tick_vampires(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mobs: Vec<Entity> = world
        .query_filtered::<Entity, With<Vampire>>()
        .iter(world)
        .collect();
    for mob in mobs {
        let recharged = world
            .get_mut::<Vampire>(mob)
            .is_some_and(|mut v| v.recharge_glare(now));
        if recharged {
            refresh(world, mob);
        }
        let rush_over = world
            .get::<Vampire>(mob)
            .is_some_and(|v| v.rush_until > 0.0 && now >= v.rush_until);
        if rush_over {
            if let Some(mut vampire) = world.get_mut::<Vampire>(mob) {
                vampire.rush_until = 0.0;
            }
            update_speed(world, mob, now);
        }
        let biting = world
            .get::<DoAfter>(mob)
            .is_some_and(|d| d.job == Job::Vampire(VampireJob::Bite));
        if !biting
            && world
                .get::<Vampire>(mob)
                .is_some_and(|v| v.draining.is_some())
        {
            stop_bite(world, mob);
        }
    }
}

/// tg `COMSIG_LIVING_LIFE` -> `handle_vampire` each `SSmobs` fire, with the holy water reagent
/// tick, the fire handler's blood cost and the rejuvenation's healing.
// Cost: every 2 s per vampire a lumcount (radius 8 sources) while cloaked and a 20-step sun ray
// on space tiles.
pub(super) fn vampire_life(world: &mut World, mut last_fired: Local<Option<u64>>) {
    let fired = world.resource::<crate::breath::MobsClock>().times_fired;
    // No backlog of the fires before the first run.
    let fires = fired.saturating_sub(last_fired.unwrap_or(fired));
    *last_fired = Some(fired);
    if fires == 0 {
        return;
    }
    let secs = crate::breath::SSMOBS_WAIT_SECS as f32;
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mobs: Vec<Entity> = world
        .query_filtered::<Entity, With<Vampire>>()
        .iter(world)
        .collect();
    for _ in 0..fires {
        for &mob in &mobs {
            live(world, mob, secs, now);
        }
    }
}

pub(super) fn live(world: &mut World, mob: Entity, secs: f32, now: f64) {
    let (Some(&pos), Some(&stat)) = (world.get::<TilePos>(mob), world.get::<Stat>(mob)) else {
        return;
    };
    let Some(vampire) = world.get::<Vampire>(mob).cloned() else {
        return;
    };
    let full = vampire.has(Power::Full);
    handle_cloak(world, mob);
    if stat != Stat::Dead {
        let space = turf_at(world, pos)
            .and_then(|t| path_of(world, t))
            .is_some_and(|p| is_under(&p, SPACE_TURF));
        if space {
            check_sun(world, mob, pos);
        }
        if in_chapel(world, mob) && !full && vampire.bloodtotal > 0.0 {
            vamp_burn(world, mob, 7.0);
        }
        holy_water(world, mob, full);
        // `on_fire_stacks_updated`: burning costs blood while it can harm.
        // shortcut: every `Life()` while the mob has fire stacks, not on each change of them;
        // no thermal protection check.
        let stacks = world
            .get::<crate::fire::FireStacks>(mob)
            .is_some_and(|f| f.stacks > 0.0);
        if stacks && !full {
            subtract_usable(world, mob, 5.0);
        }
        rejuvenation_tick(world, mob, secs, now);
    }
    if let Some(mut vampire) = world.get_mut::<Vampire>(mob) {
        vampire.nullified = (vampire.nullified - 2.0).max(0.0);
    }
    refresh(world, mob);
}

/// `/datum/status_effect/vampire_rejuvenation/tick`: the healing of 14 s spread over its ticks.
pub(super) fn rejuvenation_tick(world: &mut World, mob: Entity, secs: f32, now: f64) {
    let Some((ends_at, mult)) = world.get::<Vampire>(mob).and_then(|v| v.rejuvenation) else {
        return;
    };
    if now >= ends_at + f64::from(secs) {
        if let Some(mut vampire) = world.get_mut::<Vampire>(mob) {
            vampire.rejuvenation = None;
        }
        return;
    }
    let healing = mult * secs / REJUVENATION_SECS as f32;
    if let Some(mut health) = world.get_mut::<Health>(mob) {
        health.adjust(DamageType::Brute, -10.0 * healing);
        health.adjust(DamageType::Oxy, -25.0 * healing);
        health.adjust(DamageType::Tox, -10.0 * healing);
        health.adjust(DamageType::Burn, -10.0 * healing);
    }
    if let (Some(chem), Some(mut reagents)) = (
        world
            .get_resource::<crate::chem::Chem>()
            .map(|c| c.0.clone()),
        world.get::<crate::chem::MobReagents>(mob).cloned(),
    ) {
        let toxins: Vec<_> = reagents
            .blood
            .reagents
            .iter()
            .map(|r| r.id)
            .filter(|&id| chem.is_type(id, "/datum/reagent/toxin"))
            .collect();
        if !toxins.is_empty() {
            for id in toxins {
                reagents.blood.remove_reagent(id, 10.0 * healing);
            }
            world.entity_mut(mob).insert(reagents);
        }
    }
    if now >= ends_at
        && let Some(mut vampire) = world.get_mut::<Vampire>(mob)
    {
        vampire.rejuvenation = None;
    }
}

/// `check_sun`: starlight drains blood and burns, or turns a bloodless vampire to ash.
pub(super) fn check_sun(world: &mut World, mob: Entity, pos: TilePos) {
    let Some(sun) = world.get_resource::<crate::power::solar::Sun>().cloned() else {
        return;
    };
    let index = world.resource::<SpatialIndex>();
    let size = index.level_size(pos.z).unwrap_or((0, 0));
    let blocked = crate::power::solar::is_sunlight_blocked(pos, &sun, size, |p| {
        index
            .on_tile(p)
            .iter()
            .any(|&e| world.get::<Opaque>(e).is_some())
    });
    if blocked {
        return;
    }
    let usable = world.get::<Vampire>(mob).map_or(0.0, |v| v.bloodusable);
    if usable >= 10.0 {
        chat_to(
            world,
            mob,
            Span::Danger,
            "Свет звёзд высасывает ваши силы — покиньте его!",
        );
        subtract_usable(world, mob, 10.0);
        vamp_burn(world, mob, 10.0);
    } else {
        chat_to(
            world,
            mob,
            Span::Danger,
            "Ваше тело превращается в пепел — НЕМЕДЛЕННО уйдите от света звёзд!",
        );
        vamp_burn(world, mob, 85.0);
        // shortcut: `dust()` is death here (no dusting of bodies yet).
        let dead = world
            .get::<Health>(mob)
            .is_some_and(|h| h.health() <= crate::health::HEALTH_THRESHOLD_DEAD);
        if dead && let Some(mut stat) = world.get_mut::<Stat>(mob) {
            *stat = Stat::Dead;
        }
    }
}

/// `vamp_burn(burn_chance)`.
pub(super) fn vamp_burn(world: &mut World, mob: Entity, chance: f32) {
    let Some(health) = world.get::<Health>(mob).map(Health::health) else {
        return;
    };
    let roll = world.resource_mut::<CombatRng>().chance(chance);
    if roll && health >= 50.0 {
        if health >= 75.0 {
            chat_to(world, mob, Span::Warning, "Ваша кожа отслаивается...");
        } else {
            chat_to(world, mob, Span::Warning, "Ваша кожа шипит!");
        }
        damage(world, mob, DamageType::Burn, 3.0);
    } else if health < 50.0 {
        if crate::fire::on_fire(world, mob) {
            chat_to(world, mob, Span::Danger, "Вы продолжаете гореть!");
        } else {
            chat_to(world, mob, Span::Danger, "Ваша кожа загорается!");
        }
        crate::fire::adjust_fire_stacks(world, mob, 5.0);
        crate::fire::ignite_mob(world, mob);
    }
}

pub(super) fn damage(world: &mut World, mob: Entity, kind: DamageType, amount: f32) {
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

/// `vampire_holywater/on_reagent_tick` while holy water is in the blood (80% of its ticks).
pub(super) fn holy_water(world: &mut World, mob: Entity, full: bool) {
    let chem = world
        .get_resource::<crate::chem::Chem>()
        .map(|c| c.0.clone());
    let id = chem.as_ref().and_then(|c| c.id(HOLY_WATER));
    let present = id.is_some_and(|id| {
        world
            .get::<crate::chem::MobReagents>(mob)
            .is_some_and(|r| r.blood.amount(id) > 0.0)
    });
    let Some(mut vampire) = world.get::<Vampire>(mob).cloned() else {
        return;
    };
    if !present {
        if vampire.holy_cycles != 0
            && let Some(mut v) = world.get_mut::<Vampire>(mob)
        {
            v.holy_cycles = 0;
        }
        return;
    }
    vampire.holy_cycles += 1;
    let cycle = vampire.holy_cycles;
    if let Some(mut v) = world.get_mut::<Vampire>(mob) {
        v.holy_cycles = cycle;
    }
    let roll = world.resource_mut::<CombatRng>().chance(80.0);
    if full || !roll || vampire.bloodtotal <= 0.0 {
        return;
    }
    if vampire.bloodusable > 0.0 {
        crate::status_effects::adjust_stamina(world, mob, 5.0);
        if let Some(mut v) = world.get_mut::<Vampire>(mob) {
            v.adjust_nullification(20.0, 4.0);
            v.subtract_usable(3.0);
        }
        let empty = world
            .get::<Vampire>(mob)
            .is_some_and(|v| v.bloodusable <= 0.0);
        if empty {
            // The holy water purges itself.
            if let (Some(id), Some(mut reagents)) =
                (id, world.get::<crate::chem::MobReagents>(mob).cloned())
            {
                let volume = reagents.blood.amount(id);
                reagents.blood.remove_reagent(id, volume);
                world.entity_mut(mob).insert(reagents);
            }
            return;
        }
        damage(world, mob, DamageType::Brute, 3.0);
        return;
    }
    if let Some(mut v) = world.get_mut::<Vampire>(mob) {
        v.adjust_nullification(20.0, 4.0);
    }
    match cycle {
        1..=4 => chat_to(world, mob, Span::Warning, "Что-то шипит в ваших венах!"),
        5..=12 => {
            chat_to(
                world,
                mob,
                Span::Danger,
                "Вы чувствуете сильное жжение внутри!",
            );
            damage(world, mob, DamageType::Burn, 1.0);
        }
        _ => {
            let name = name_of(world, mob);
            visible(
                world,
                mob,
                "Вас внезапно охватывает священное пламя!",
                &format!("{name} внезапно вспыхивает!"),
                Span::Danger,
            );
            let stacks = world
                .get::<crate::fire::FireStacks>(mob)
                .map_or(0.0, |f| f.stacks);
            let add = (stacks + 3.0).min(5.0) - stacks;
            crate::fire::adjust_fire_stacks(world, mob, add.max(0.0));
            crate::fire::ignite_mob(world, mob);
            damage(world, mob, DamageType::Burn, 3.0);
        }
    }
}
