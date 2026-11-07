//! Clicks and items: the codex, the blades and the mansus grasp.

use super::*;

/// An empty hand on a heretic thing (`attack_hand` / `attack_hand_secondary`).
pub fn interact(
    world: &mut World,
    client: Entity,
    mob: Entity,
    target: Entity,
    right: bool,
) -> bool {
    if world.get::<Influence>(target).is_some() {
        if !is_heretic(world, mob) {
            return false;
        }
        if right {
            start_drain(world, client, mob, target, 1, DRAIN_DS);
        }
        return true;
    }
    if world.get::<VisibleInfluence>(target).is_some() {
        if is_heretic(world, mob) {
            tell(
                world,
                client,
                Span::BoldWarning,
                "Вам лучше не искушать неподвластные вам силы!",
            );
        } else {
            // 25%: the active hand's arm is torn off and kept by the influence (tg "stored for
            // later fishage"; no fishing here, so it is gone).
            let zone = if world.get::<Inventory>(mob).map_or(0, |i| i.active) == 0 {
                crate::zone::BodyZone::LArm
            } else {
                crate::zone::BodyZone::RArm
            };
            let has_arm = world
                .get::<crate::medical::wounds::Limbs>(mob)
                .is_some_and(|l| !l.limb(zone).missing);
            if has_arm && world.resource_mut::<CombatRng>().chance(25.0) {
                tell(
                    world,
                    client,
                    Span::Danger,
                    format!(
                        "Потустороннее присутствие разрывает и распыляет вашу {}, когда вы \
                         пытаетесь коснуться дыры в самой ткани реальности!",
                        zone.ru().1
                    ),
                );
                if let Some(Some(arm)) =
                    crate::medical::bodyparts::dismember_limb(world, mob, zone, false)
                {
                    world.entity_mut(arm).despawn();
                }
            } else {
                tell(
                    world,
                    client,
                    Span::Danger,
                    "Вы отдёргиваете руку от отверстия, когда мистическая энергия бьется, \
                     пытаясь зацепиться за этот мир!",
                );
            }
        }
        return true;
    }
    if world.get::<TransmutationRune>(target).is_some() {
        if !is_heretic(world, mob) {
            return false;
        }
        if world
            .get::<TransmutationRune>(target)
            .is_some_and(|r| r.in_use)
        {
            return true;
        }
        try_rituals(world, client, mob, target);
        return true;
    }
    false
}

/// A held item used on a target: a writing tool on a floor (the rune), the codex on an
/// influence, the grasp on a rune (erasing it).
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<MansusFist>(held).is_some() {
        if world.get::<TransmutationRune>(target).is_some() {
            // `effect_remover` (0.4 s): the rune is gone, the hand used without a refund.
            let name = name_of(world, target);
            tell(world, client, Span::Notice, format!("Вы убираете {name}."));
            despawn(world, target);
            despawn_held(world, mob, held);
            let t = now(world);
            if let Some(mut h) = world.get_mut::<Heretic>(mob) {
                h.set_cooldown(&spell_id(BASIC), t + GRASP_COOLDOWN);
            }
            refresh(world, mob);
            return true;
        }
        // Living targets are the melee click's (`heretic_clicks`).
        return !is_living(world, target);
    }
    let Some(path) = path_of(world, held) else {
        return false;
    };
    if knowledge::is_type(&path, CODEX) && world.get::<Influence>(target).is_some() {
        if is_heretic(world, mob) {
            start_drain(world, client, mob, target, 2, CODEX_DRAIN_DS);
            return true;
        }
        return false;
    }
    if is_writing_utensil(&path) && is_heretic(world, mob) {
        let Some(&pos) = world.get::<TilePos>(target) else {
            return false;
        };
        if crate::construction::turf_at(world, pos) != Some(target) {
            return false;
        }
        if !grasp_in_offhand(world, mob) {
            return false;
        }
        try_draw_rune(world, client, mob, held, target, pos);
        return true;
    }
    false
}

/// `attack_self`: a sickly blade breaks to teleport the heretic.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if world.get::<MansusFist>(item).is_some() {
        return true;
    }
    if !is_blade(world, item) {
        return false;
    }
    if world
        .get::<Heretic>(mob)
        .is_some_and(|h| h.unlimited_blades)
    {
        return true;
    }
    // `seek_safety`.
    let name = name_of(world, item);
    let usable = is_heretic(world, mob);
    let dest = crate::events::electrical_storm::random_station_turf(world);
    match (usable, dest) {
        (true, Some(dest)) => {
            world.entity_mut(mob).insert(dest);
            tell(
                world,
                client,
                Span::Warning,
                format!(
                    "Когда вы разбиваете {name}, вы чувствуете, как порыв энергии проходит через \
                     ваше тело. {}",
                    blade_use_message(&path_of(world, item).unwrap_or_default())
                ),
            );
        }
        (true, None) => tell(
            world,
            client,
            Span::Warning,
            format!("Вы разбиваете {name}, но ваша мольба остается без ответа."),
        ),
        (false, _) => tell(
            world,
            client,
            Span::Warning,
            format!("Вы разбиваете {name}."),
        ),
    }
    despawn_held(world, mob, item);
    true
}

/// `after_use_message` of the blades.
pub(super) fn blade_use_message(path: &str) -> &'static str {
    match path.rsplit('/').next().unwrap_or_default() {
        "rust" => "Ржавые холмы слышат ваш зов...",
        "ash" => "Ночной дозорный слышит ваш зов...",
        "flesh" => "Маршал слышит ваш зов...",
        _ => "",
    }
}

/// The grasp in the inactive hand (`check_mansus_grasp_offhand`).
pub fn grasp_in_offhand(world: &World, mob: Entity) -> bool {
    world.get::<Inventory>(mob).is_some_and(|i| {
        i.hands[1 - i.active].is_some_and(|h| world.get::<MansusFist>(h).is_some())
    })
}

/// The grasp or a sickly blade on a mob, before melee.
pub(super) fn heretic_clicks(
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
            || !is_living(world, target)
        {
            continue;
        }
        let Some(held) = world.get::<Inventory>(mob).and_then(Inventory::active_item) else {
            continue;
        };
        let t = now(world);
        if world.get::<NextMove>(mob).is_some_and(|n| t < n.0) {
            continue;
        }
        if world.get::<MansusFist>(held).is_some() {
            world
                .resource_mut::<crate::security::MeleeHandled>()
                .0
                .insert(mob);
            grasp_hit(world, mob, held, target);
            continue;
        }
        if is_blade(world, held) {
            if !is_heretic(world, mob) {
                // `pre_attack` of a non-heretic.
                world
                    .resource_mut::<crate::security::MeleeHandled>()
                    .0
                    .insert(mob);
                chat_to(
                    world,
                    mob,
                    Span::Danger,
                    "Ты чувствуешь, как импульс чужого интеллекта бьет по твоему разуму!",
                );
                crate::status_effects::apply(world, mob, Incap::Paralyze, 5.0);
            } else {
                world
                    .resource_mut::<PendingBladeHits>()
                    .0
                    .push((mob, target, t));
            }
        }
    }
}

/// `afterattack` of the sickly blade: the melee landed (it set the attacker's `next_move`).
pub(super) fn after_melee(world: &mut World) {
    let pending = std::mem::take(&mut world.resource_mut::<PendingBladeHits>().0);
    for (mob, target, at) in pending {
        if !world.get::<NextMove>(mob).is_some_and(|n| n.0 > at) {
            continue;
        }
        blade_attack(world, mob, target);
    }
}

/// `COMSIG_HERETIC_BLADE_ATTACK`: the starting knowledge triggers the mark, the blade upgrade
/// adds its effect.
pub(super) fn blade_attack(world: &mut World, mob: Entity, target: Entity) {
    let Some(h) = world.get::<Heretic>(mob) else {
        return;
    };
    let (ash, upgrade) = (h.has(ASH_START), h.has(ASH_BLADE_UPGRADE));
    // `starting/trigger_mark` triggers any heretic's mark.
    let other_path = crate::antag::heretic::paths::has_path_start(world, mob);
    let triggered = (ash || other_path)
        && (trigger_ash_mark(world, target)
            || crate::antag::heretic::paths::trigger_path_mark(world, target));
    if ash && triggered {
        // `base_ash/trigger_mark`: 75% of the grasp's cooldown back.
        if let Some(mut h) = world.get_mut::<Heretic>(mob) {
            let id = spell_id(BASIC);
            let ready = h.cooldown(&id) - GRASP_COOLDOWN * 0.75;
            h.set_cooldown(&id, ready);
        }
        refresh(world, mob);
    }
    if upgrade && mob != target {
        // `blade_upgrade/ash`.
        crate::fire::adjust_fire_stacks(world, target, 1.0);
        crate::fire::ignite_mob(world, target);
    }
    crate::antag::heretic::paths::on_blade(world, mob, target);
}

/// `/datum/status_effect/eldritch/ash/on_effect`: stamina and burn by repetitions, the mark
/// jumps to a carbon next to it with one repetition less.
pub(super) fn trigger_ash_mark(world: &mut World, target: Entity) -> bool {
    let t = now(world);
    let Some(mark) = world
        .get::<AshMark>(target)
        .copied()
        .filter(|m| m.until > t)
    else {
        return false;
    };
    world.entity_mut(target).remove::<AshMark>();
    let reps = mark.repetitions as f32;
    crate::status_effects::adjust_stamina(world, target, 6.0 * reps);
    hurt(world, target, DamageType::Burn, 3.0 * reps);
    let Some(&pos) = world.get::<TilePos>(target) else {
        return true;
    };
    let mut victims: Vec<Entity> = Vec::new();
    {
        let index = world.resource::<ss13_world::SpatialIndex>();
        for dx in -1..=1 {
            for dy in -1..=1 {
                for &e in index.on_tile(TilePos::new(pos.x + dx, pos.y + dy, pos.z)) {
                    if e != target && world.get::<HumanLook>(e).is_some() {
                        victims.push(e);
                    }
                }
            }
        }
    }
    victims.retain(|&v| !is_heretic(world, v));
    if !victims.is_empty() {
        let v = victims[rand(world, victims.len())];
        world.entity_mut(v).insert(AshMark {
            until: t + MARK_SECS,
            repetitions: mark.repetitions.saturating_sub(1).max(1),
        });
    }
    true
}

/// `mansus_grasp/cast_on_hand_hit` and the spell's after-cast: the invocation, the cooldown, a
/// charge, the hand gone.
pub(super) fn grasp_hit(world: &mut World, mob: Entity, hand: Entity, target: Entity) {
    let Some(owner) = world.get::<MansusFist>(hand).map(|f| f.owner) else {
        return;
    };
    if owner != mob || !is_heretic(world, mob) {
        despawn_held(world, mob, hand);
        return;
    }
    say(world, mob, "R'CH T'H TR'TH!");
    // `COMSIG_HERETIC_MANSUS_GRASP_ATTACK`: the starting knowledge's mark.
    let ash = world.get::<Heretic>(mob).is_some_and(|h| h.has(ASH_START));
    let t = now(world);
    crate::antag::heretic::paths::on_grasp(world, mob, target);
    if ash && world.get::<Stat>(target) != Some(&Stat::Dead) {
        world
            .entity_mut(target)
            .remove::<crate::antag::heretic::paths::PathMark>();
        world.entity_mut(target).insert(AshMark {
            until: t + MARK_SECS,
            repetitions: 5,
        });
    }
    // `base_ash/on_mansus_grasp`: the eyes burn unless blind or missing.
    if ash
        && !crate::vision::is_blind(world, target)
        && crate::medical::organs::organ(world, target, crate::medical::organs::Slot::Eyes)
            .is_some()
    {
        chat_to(
            world,
            target,
            Span::Danger,
            "Яркий зеленый свет ужасно жжет глаза!",
        );
        crate::medical::organs::adjust_organ_loss(
            world,
            target,
            crate::medical::organs::Slot::Eyes,
            15.0,
        );
        crate::vision::set_eye_blur_if_lower(world, target, 20.0);
    }
    hurt(world, target, DamageType::Brute, 10.0);
    if world.get::<HumanLook>(target).is_some() {
        if crate::antag::cult::server::is_cultist(world, target) {
            crate::status_effects::knockdown(world, target, 0.5, 0.0);
            if let Some(mut e) = effects_mut(world, target) {
                e.confusion = (e.confusion + 1.5).min(3.0).max(e.confusion);
            }
            let victim = name_of(world, target);
            let user = name_of(world, mob);
            chat_to(
                world,
                mob,
                Span::Warning,
                format!(
                    "Нечестивая сила вмешивается, когда вы наносите {victim}, поглощая большую \
                     часть эффекта!"
                ),
            );
            chat_to(
                world,
                target,
                Span::Warning,
                format!(
                    "Когда {user} хватает вас потусторонними силами, ваша кровавая магия \
                     поглощает большую часть эффектов!"
                ),
            );
        } else {
            crate::status_effects::knockdown(world, target, 5.0, 3.0);
            crate::status_effects::adjust_timed(
                world,
                target,
                crate::status_effects::Timed::Slurring(crate::status_effects::SlurKind::Heretic),
                4.0,
                None,
            );
            crate::status_effects::adjust_stamina(world, target, 80.0);
        }
    }
    despawn_held(world, mob, hand);
    spend_charge(world, mob, BASIC);
    if let Some(mut h) = world.get_mut::<Heretic>(mob) {
        h.set_cooldown(&spell_id(BASIC), t + GRASP_COOLDOWN);
    }
    refresh(world, mob);
}

pub(in crate::antag::heretic) fn spend_charge(world: &mut World, mob: Entity, path: &str) {
    if let Some(mut h) = world.get_mut::<Heretic>(mob) {
        if h.ascended {
            return;
        }
        if let Some(r) = h.get_mut(path) {
            r.charges = r.charges.saturating_sub(1);
        }
    }
}
