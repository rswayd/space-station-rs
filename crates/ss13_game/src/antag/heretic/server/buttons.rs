//! The action buttons and the spells.

use super::*;

pub(in crate::antag::heretic) fn spell_id(path: &str) -> String {
    format!("{SPELL_PREFIX}{path}")
}

/// The heretic's buttons: its spells (charges as maptext), the living heart, the Ash Lord's.
pub fn refresh(world: &mut World, mob: Entity) {
    let Some(h) = world.get::<Heretic>(mob).cloned() else {
        return;
    };
    let (background, border) = (
        icon(world, BACKGROUNDS_DMI, "bg_heretic"),
        icon(world, BACKGROUNDS_DMI, "bg_heretic_border"),
    );
    let in_hand = hands(world, mob)
        .iter()
        .any(|&e| world.get::<MansusFist>(e).is_some());
    let mut buttons = Vec::new();
    let mut add =
        |id: String, name: &str, icon, maptext: Option<String>, active: bool, ready: f64| {
            buttons.push((
                ActionButton {
                    id,
                    name: name.to_owned(),
                    icon,
                    background,
                    border,
                    available: true,
                    cooldown: 0.0,
                    active,
                    maptext,
                },
                ready,
            ));
        };
    for r in &h.researched {
        let def = r.def();
        if def.kind != Kind::Spell || def.action.is_empty() {
            continue;
        }
        let id = spell_id(def.path);
        let maptext = (def.max_charges != u32::MAX && r.charges < 100 && !h.ascended)
            .then(|| r.charges.to_string());
        let active = def.action == GRASP_ACTION && in_hand;
        add(
            id.clone(),
            def.action_name,
            icon(world, def.icon.0, def.icon.1),
            maptext,
            active,
            h.cooldown(&id),
        );
    }
    if h.living_heart {
        add(
            HEART_ID.into(),
            "Биение живого сердца",
            icon(world, "icons/obj/antags/eldritch.dmi", "living_heart"),
            None,
            false,
            h.cooldown(HEART_ID),
        );
    }
    if h.ash_ascended_spells {
        add(
            FIRE_SWORN_ID.into(),
            "Клятва Пламени",
            icon(world, ECULT_DMI, "fire_ring"),
            None,
            false,
            h.cooldown(FIRE_SWORN_ID),
        );
        add(
            FIRE_CASCADE_ID.into(),
            "Великий огненный каскад",
            icon(world, ECULT_DMI, "fire_ring"),
            None,
            false,
            h.cooldown(FIRE_CASCADE_ID),
        );
    }
    set_buttons(world, mob, buttons);
}

pub(super) fn trigger_buttons(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>,
) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        if actor != source
            || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
            || !is_heretic(world, actor)
            || !can_act(world, actor)
        {
            continue;
        }
        let t = now(world);
        let ready = world.get::<Heretic>(actor).map_or(0.0, |h| h.cooldown(&id));
        if t < ready {
            continue;
        }
        if id == HEART_ID {
            track_target(world, client, actor);
        } else if id == FIRE_SWORN_ID {
            whisper(world, actor, "FL'MS.");
            world.entity_mut(actor).insert(FireRing {
                until: t + 60.0,
                next_tick: t,
            });
            set_cooldown(world, actor, &id, t + 70.0);
        } else if id == FIRE_CASCADE_ID {
            whisper(world, actor, "C'SC'D.");
            if let Some(&center) = world.get::<TilePos>(actor) {
                world.spawn(FireCascade {
                    caster: actor,
                    center,
                    step: 0,
                    radius: 6,
                    next_at: t,
                });
            }
            set_cooldown(world, actor, &id, t + 30.0);
        } else if let Some(path) = id.strip_prefix(SPELL_PREFIX) {
            let Some(def) = knowledge::def(path) else {
                continue;
            };
            cast_spell(world, client, actor, def);
        }
    }
}

pub(in crate::antag::heretic) fn set_cooldown(
    world: &mut World,
    mob: Entity,
    id: &str,
    ready: f64,
) {
    // The crimson medallion's `cooldown_time *= 0.5`.
    let ready = if hastened(world, mob) {
        let t = now(world);
        t + (ready - t) * 0.5
    } else {
        ready
    };
    if let Some(mut h) = world.get_mut::<Heretic>(mob) {
        h.set_cooldown(id, ready);
    }
    refresh(world, mob);
}

/// A spell button: the charge check (`check_charges`), then the spell.
pub(super) fn cast_spell(
    world: &mut World,
    client: Entity,
    mob: Entity,
    def: &'static KnowledgeDef,
) {
    let Some(h) = world.get::<Heretic>(mob) else {
        return;
    };
    let Some(r) = h.get(def.path) else {
        return;
    };
    let has_hand = hands(world, mob)
        .iter()
        .any(|&e| world.get::<MansusFist>(e).is_some());
    if r.charges == 0 && !h.ascended && !(def.action == GRASP_ACTION && has_hand) {
        tell(
            world,
            client,
            Span::Mansus,
            format!(
                "У вас недостаточно зарядов, чтобы произнести это заклинание! {}",
                def.transmute_text
            ),
        );
        return;
    }
    match def.action {
        GRASP_ACTION => toggle_grasp(world, client, mob),
        FIRE_BLAST => {
            // `channel_time` 2.5 s.
            let actor = Actor {
                client,
                mob,
                item: world.get::<Inventory>(mob).and_then(Inventory::active_item),
            };
            do_after(
                world,
                actor,
                mob,
                25.0,
                0,
                Job::Heretic(HereticJob::FireBlast),
            );
        }
        FIERY_REBIRTH => cast_fiery_rebirth(world, mob),
        _ if crate::antag::heretic::paths::cast(world, client, mob, def) => {}
        _ => tell(
            world,
            client,
            Span::Warning,
            "Это заклинание ещё не перенесено в ss13-rs.",
        ),
    }
}

/// `touch/Activate`: a hand into an empty hand, or the hand back.
pub(super) fn toggle_grasp(world: &mut World, client: Entity, mob: Entity) {
    if let Some(hand) = hands(world, mob)
        .into_iter()
        .find(|&e| world.get::<MansusFist>(e).is_some())
    {
        despawn_held(world, mob, hand);
        tell(
            world,
            client,
            Span::Notice,
            "You draw the power out of your hand.",
        );
        refresh(world, mob);
        return;
    }
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let Some(hand) = crate::construction::spawn_obj(world, MANSUS_FIST, pos) else {
        return;
    };
    world
        .entity_mut(hand)
        .insert((NoDrop, MansusFist { owner: mob }));
    if !put_in_hands(world, mob, hand) {
        world.despawn(hand);
        crate::construction::tile_changed(world, pos);
        tell(world, client, Span::Warning, "Ваши руки заняты!");
        return;
    }
    tell(
        world,
        client,
        Span::Notice,
        "You channel the power of the spell to your hand.",
    );
    refresh(world, mob);
}

/// Carbons around `center` within `radius` (`range`): (entity, tile).
pub(in crate::antag::heretic) fn carbons_in_range(
    world: &World,
    center: TilePos,
    radius: i32,
) -> Vec<Entity> {
    let index = world.resource::<ss13_world::SpatialIndex>();
    let mut out: Vec<Entity> = index
        .in_rect(
            center.z,
            (center.x - radius, center.y - radius),
            (center.x + radius, center.y + radius),
        )
        .filter(|&e| world.get::<HumanLook>(e).is_some() && is_living(world, e))
        .collect();
    out.sort();
    out
}

/// `fire_blast/get_target`: a carbon in view, not blasted, not a heretic; the burning first.
pub(super) fn fire_blast_target(
    world: &mut World,
    center: Entity,
    caster: Entity,
) -> Option<Entity> {
    let &pos = world.get::<TilePos>(center)?;
    let t = now(world);
    let mut possible = Vec::new();
    let mut priority = Vec::new();
    for e in carbons_in_range(world, pos, 7) {
        if e == center || e == caster || is_heretic(world, e) {
            continue;
        }
        if world.get::<FireBlasted>(e).is_some_and(|f| f.until > t) {
            continue;
        }
        // shortcut: within 7 tiles, without tg's `view()` and `get_path_to` checks.
        possible.push(e);
        if crate::fire::on_fire(world, e) && world.get::<Stat>(e) != Some(&Stat::Dead) {
            priority.push(e);
        }
    }
    let list = if priority.is_empty() {
        possible
    } else {
        priority
    };
    if list.is_empty() {
        return None;
    }
    let i = rand(world, list.len());
    Some(list[i])
}

/// `fire_blast/cast` after the channel: the caster is "blasted" (heals), the beam goes out.
pub(super) fn cast_fire_blast(world: &mut World, mob: Entity) {
    if !is_heretic(world, mob) {
        return;
    }
    shout(world, mob, "V'LC'N!");
    let t = now(world);
    world.entity_mut(mob).insert(FireBlasted {
        until: t + 2.0,
        next_tick: t + 0.6,
        tick_damage: -2.0,
        bounce: None,
    });
    let path = "/datum/heretic_knowledge/spell/fire_blast";
    if let Some(target) = fire_blast_target(world, mob, mob) {
        send_fire_beam(world, mob, target, 4);
    }
    spend_charge(world, mob, path);
    set_cooldown(world, mob, &spell_id(path), t + 45.0);
}

pub(super) fn shout(world: &mut World, mob: Entity, text: &str) {
    say(world, mob, text);
}

/// `send_beam`: 20 burn, 3 fire stacks and fire; it bounces on in a second while they burn.
pub(super) fn send_fire_beam(world: &mut World, caster: Entity, target: Entity, bounces: u32) {
    let t = now(world);
    hurt(world, target, DamageType::Burn, 20.0);
    crate::fire::adjust_fire_stacks(world, target, 3.0);
    crate::fire::ignite_mob(world, target);
    world.entity_mut(target).insert(FireBlasted {
        until: t + 5.0,
        next_tick: t + 0.6,
        tick_damage: 1.0,
        bounce: (bounces >= 1).then_some((caster, bounces, t + 1.0)),
    });
    if bounces == 0 {
        // The chain's end: everyone around is knocked down and set on fire.
        let Some(&pos) = world.get::<TilePos>(target) else {
            return;
        };
        for e in carbons_in_range(world, pos, 1) {
            if e == caster || is_heretic(world, e) {
                continue;
            }
            crate::status_effects::knockdown(world, e, 0.8, 0.0);
            hurt(world, e, DamageType::Burn, 15.0);
            crate::fire::adjust_fire_stacks(world, e, 2.0);
            crate::fire::ignite_mob(world, e);
        }
    }
}

/// `fiery_rebirth`: the caster is put out; every burning living non-heretic within 14 tiles is
/// drained (dies in crit, else 20 burn and put out) and heals the caster 10 of each damage.
pub(super) fn cast_fiery_rebirth(world: &mut World, mob: Entity) {
    whisper(world, mob, "GL'RY T' TH' N'GHT'W'TCH'ER.");
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    extinguish(world, mob);
    let mut victims = 0u32;
    for e in carbons_in_range(world, pos, 14) {
        if e == mob || is_heretic(world, e) || world.get::<Stat>(e) == Some(&Stat::Dead) {
            continue;
        }
        if !crate::fire::on_fire(world, e) {
            continue;
        }
        victims += 1;
        if world.get::<Stat>(e).is_some_and(|s| *s >= Stat::SoftCrit) {
            // `CAN_SUCCUMB`: the death mark.
            world.entity_mut(e).insert(Stat::Dead);
        }
        hurt(world, e, DamageType::Burn, 20.0);
        extinguish(world, e);
        heal(world, mob, 10.0);
    }
    let t = now(world);
    let path = "/datum/heretic_knowledge/spell/flame_birth";
    let cooldown = if victims == 0 {
        60.0
    } else {
        (60.0 - f64::from(victims) * 10.0).max(9.0)
    };
    let cooldown = if world.get::<Heretic>(mob).is_some_and(|h| h.ascended) {
        cooldown * 0.16
    } else {
        cooldown
    };
    spend_charge(world, mob, path);
    set_cooldown(world, mob, &spell_id(path), t + cooldown);
}

pub(super) fn extinguish(world: &mut World, mob: Entity) {
    let stacks = world
        .get::<crate::fire::FireStacks>(mob)
        .map_or(0.0, |f| f.stacks);
    if stacks > 0.0 {
        crate::fire::adjust_fire_stacks(world, mob, -stacks);
    }
}

/// The living heart (`track_target`): the targets (and the heretic's blades elsewhere) in a
/// list, then where the pick is.
pub(super) fn track_target(world: &mut World, client: Entity, mob: Entity) {
    let Some(h) = world.get::<Heretic>(mob).cloned() else {
        return;
    };
    let t = now(world);
    let targets: Vec<Entity> = h
        .sac_targets
        .iter()
        .copied()
        .filter(|&e| world.get_entity(e).is_ok())
        .collect();
    let blades: Vec<Entity> = h
        .researched
        .iter()
        .filter(|r| r.def().kind == Kind::Starting)
        .flat_map(|r| r.created.clone())
        .filter(|&e| {
            world.get_entity(e).is_ok() && world.get::<CarriedBy>(e).map(|c| c.0) != Some(mob)
        })
        .collect();
    if targets.is_empty() {
        tell(world, client, Span::Warning, "нет цели, посетите руну");
        set_cooldown(world, mob, HEART_ID, t + 1.0);
        return;
    }
    let mut items: Vec<String> = Vec::new();
    let mut values: Vec<Entity> = Vec::new();
    for e in blades.into_iter().chain(targets) {
        let mut name = name_of(world, e);
        let n = items.iter().filter(|i| i.starts_with(&name)).count();
        if n > 0 {
            name = format!("{name} ({})", n + 1);
        }
        items.push(name);
        values.push(e);
    }
    open_prompt(
        world,
        client,
        mob,
        HereticPrompt {
            kind: PromptKind::Track,
            title: "Биение живого сердца".into(),
            message: "Выберите цель для отслеживания".into(),
            items,
            paths: Vec::new(),
            targets: values,
        },
    );
}

/// `get_balloon_message`.
pub fn heart_message(from: TilePos, to: Option<TilePos>, dead: bool) -> String {
    let base = match to {
        None => "в другом мире!".to_owned(),
        Some(to) if to.z != from.z => "в другом мире!".to_owned(),
        Some(to) => {
            let dist = (to.x - from.x).abs().max((to.y - from.y).abs());
            let dir = dir_text(to.x - from.x, to.y - from.y);
            match dist {
                0..=15 => format!("очень близко, {dir}!"),
                16..=31 => format!("близко, {dir}!"),
                32..=127 => format!("далеко, {dir}!"),
                _ => "очень далеко!".to_owned(),
            }
        }
    };
    if dead {
        format!("мертвы, {base}")
    } else {
        base
    }
}

/// DM `dir2text(get_dir())` (BandaStation's are English).
pub(super) fn dir_text(dx: i32, dy: i32) -> &'static str {
    match (dx.signum(), dy.signum()) {
        (0, 1) => "north",
        (0, -1) => "south",
        (1, 0) => "east",
        (-1, 0) => "west",
        (1, 1) => "northeast",
        (-1, 1) => "northwest",
        (1, -1) => "southeast",
        (-1, -1) => "southwest",
        _ => "",
    }
}

pub(super) fn track_pick(world: &mut World, client: Entity, mob: Entity, target: Entity) {
    let Some(&from) = world.get::<TilePos>(mob) else {
        return;
    };
    if world.get_entity(target).is_err() {
        return;
    }
    let to = crate::modpc::pda::turf_of(world, target);
    let dead = world.get::<Stat>(target) == Some(&Stat::Dead);
    let text = heart_message(from, to, dead);
    tell(world, client, Span::Notice, text);
    if dead {
        let name = capitalize(&name_of(world, target));
        let sac = knowledge::def(HUNT_AND_SACRIFICE).map_or("", |k| k.name);
        tell(
            world,
            client,
            Span::Mansus,
            format!(
                "{name} в состоянии смерти. Принесите их к руне трансмутации и вызовите \"{sac}\", \
                 чтобы принести их в жертву!"
            ),
        );
    }
    if let Some(mut h) = world.get_mut::<Heretic>(mob) {
        h.last_tracked = Some(target);
    }
    let t = now(world);
    set_cooldown(world, mob, HEART_ID, t + 4.0);
}
