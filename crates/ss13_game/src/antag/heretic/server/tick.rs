//! Ticking, and the round end report.

use super::*;

/// Every tick: the passive point, marks, the sacrifice chain, the fire blasts, the Ash Lord's
/// fire.
pub(super) fn heretic_tick(world: &mut World) {
    let t = now(world);
    // `passive_influence_gain`.
    let mut q = world.query::<(Entity, &Heretic)>();
    let due: Vec<Entity> = q
        .iter(world)
        .filter(|(_, h)| t >= h.next_passive_gain)
        .map(|(e, _)| e)
        .collect();
    for mob in due {
        if let Some(mut h) = world.get_mut::<Heretic>(mob) {
            h.next_passive_gain += PASSIVE_GAIN_SECS;
        }
        adjust_points(world, mob, 1);
        if world.get::<Stat>(mob).is_some_and(|s| *s < Stat::HardCrit) {
            let drain = pick_str(world, INFLUENCE_DRAIN_MESSAGE);
            chat_to(
                world,
                mob,
                Span::Mansus,
                format!("Вы слышите шепот... {drain}"),
            );
        }
        update_ui(world, mob);
    }
    // Marks run out.
    let mut q = world.query::<(Entity, &AshMark)>();
    let expired: Vec<Entity> = q
        .iter(world)
        .filter(|(_, m)| m.until <= t)
        .map(|(e, _)| e)
        .collect();
    for e in expired {
        world.entity_mut(e).remove::<AshMark>();
    }
    // The sacrificed come back.
    let mut q = world.query::<(Entity, &Sacrificed)>();
    let woke: Vec<(Entity, Sacrificed)> = q
        .iter(world)
        .filter(|(_, s)| s.wake_at <= t)
        .map(|(e, s)| (e, *s))
        .collect();
    for (victim, chain) in woke {
        return_sacrifice(world, victim, chain);
    }
    tick_fire_blasts(world, t);
    tick_fire_rings(world, t);
    tick_cascades(world, t);
    crate::antag::heretic::paths::tick(world, t);
}

pub(super) fn tick_fire_blasts(world: &mut World, t: f64) {
    let mut q = world.query::<(Entity, &FireBlasted)>();
    let all: Vec<(Entity, FireBlasted)> = q.iter(world).map(|(e, f)| (e, *f)).collect();
    for (e, mut f) in all {
        if let Some((caster, bounces, at)) = f.bounce
            && t >= at
        {
            f.bounce = None;
            // `continue_beam`: only while they still burn.
            if crate::fire::on_fire(world, e)
                && let Some(next) = fire_blast_target(world, e, caster)
            {
                world.entity_mut(e).insert(f);
                send_fire_beam(world, caster, next, bounces - 1);
            }
        }
        while f.next_tick <= t.min(f.until) {
            f.next_tick += 0.6;
            let dmg = f.tick_damage * 0.6;
            if dmg > 0.0 {
                hurt(world, e, DamageType::Burn, dmg);
            } else {
                heal(world, e, -dmg);
            }
            crate::status_effects::adjust_stamina(world, e, 2.0 * dmg);
        }
        if t >= f.until && f.bounce.is_none() {
            world.entity_mut(e).remove::<FireBlasted>();
        } else if world.get_entity(e).is_ok() {
            world.entity_mut(e).insert(f);
        }
    }
}

/// `fire_ring/tick` every 0.2 s: the 3x3 around burns, 2.5 burn/s to the others there.
pub(super) fn tick_fire_rings(world: &mut World, t: f64) {
    let mut q = world.query::<(Entity, &FireRing)>();
    let rings: Vec<(Entity, FireRing)> = q.iter(world).map(|(e, r)| (e, *r)).collect();
    for (owner, mut ring) in rings {
        if t >= ring.until || world.get::<Stat>(owner) == Some(&Stat::Dead) {
            world.entity_mut(owner).remove::<FireRing>();
            continue;
        }
        while ring.next_tick <= t {
            ring.next_tick += 0.2;
            let Some(&pos) = world.get::<TilePos>(owner) else {
                break;
            };
            burn_tiles(world, owner, pos, 1, 25.0 * 0.2, 2.5 * 0.2);
        }
        world.entity_mut(owner).insert(ring);
    }
}

pub(super) fn burn_tiles(
    world: &mut World,
    owner: Entity,
    center: TilePos,
    radius: i32,
    volume: f64,
    burn: f32,
) {
    let mut tiles = Vec::new();
    for dx in -radius..=radius {
        for dy in -radius..=radius {
            tiles.push(TilePos::new(center.x + dx, center.y + dy, center.z));
        }
    }
    burn_list(world, owner, &tiles, volume, burn);
}

pub(super) fn burn_list(
    world: &mut World,
    owner: Entity,
    tiles: &[TilePos],
    volume: f64,
    burn: f32,
) {
    let mut victims = Vec::new();
    {
        let index = world.resource::<ss13_world::SpatialIndex>();
        for &p in tiles {
            for &e in index.on_tile(p) {
                if e != owner && world.get::<Health>(e).is_some() {
                    victims.push(e);
                }
            }
        }
    }
    if let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() {
        for &p in tiles {
            atmos.hotspot_expose(p, 750.0, volume, true);
        }
    }
    for v in victims {
        hurt(world, v, DamageType::Burn, burn);
    }
}

/// `fire_cascade`: rings of fire spreading out one a 0.3 s.
pub(super) fn tick_cascades(world: &mut World, t: f64) {
    let mut q = world.query::<(Entity, &FireCascade)>();
    let all: Vec<(Entity, FireCascade)> = q.iter(world).map(|(e, c)| (e, *c)).collect();
    for (e, mut c) in all {
        if t < c.next_at {
            continue;
        }
        let r = c.step + 1;
        let mut ring = Vec::new();
        for dx in -r..=r {
            for dy in -r..=r {
                ring.push(TilePos::new(c.center.x + dx, c.center.y + dy, c.center.z));
            }
        }
        burn_list(world, c.caster, &ring, 50.0, 5.0);
        c.step += 1;
        c.next_at = t + 0.3;
        if c.step > c.radius {
            world.despawn(e);
        } else {
            world.entity_mut(e).insert(c);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Round end

/// `/datum/antagonist/heretic/roundend_report` of every heretic.
pub fn roundend_report(world: &mut World, _end: RoundEnd) -> Vec<ChatMessage> {
    let mut q = world.query::<(Entity, &Heretic)>();
    let mut heretics: Vec<(Entity, Heretic)> = q.iter(world).map(|(e, h)| (e, h.clone())).collect();
    if heretics.is_empty() {
        return Vec::new();
    }
    heretics.sort_by_key(|(e, _)| *e);
    let line = |segments: Vec<ChatSegment>| ChatMessage {
        channel: ChatChannel::System,
        segments,
    };
    let mut out = vec![line(vec![ChatSegment::new(
        Span::Bold,
        format!("{ROUNDEND_CATEGORY} были:"),
    )])];
    for (mob, h) in heretics {
        out.push(crate::antag::traitor::print_player(world, mob, &h.key));
        out.push(line(vec![
            ChatSegment::new(Span::Bold, "Жертвоприношений сделано:"),
            ChatSegment::new(Span::Plain, format!(" {}", h.total_sacrifices)),
        ]));
        let targets = if h.all_sac_targets.is_empty() {
            "Никто".to_owned()
        } else {
            english_list(&h.all_sac_targets)
        };
        out.push(line(vec![ChatSegment::new(
            Span::Plain,
            format!("Целями для жертвоприношения были: {targets}."),
        )]));
        let mut succeeded = true;
        for (i, o) in h.objectives.iter().enumerate() {
            let done = o.complete(&h);
            succeeded &= done;
            out.push(line(vec![
                ChatSegment::new(Span::Bold, format!("{OBJECTIVE_NAME} #{}", i + 1)),
                ChatSegment::new(Span::Plain, format!(": {} ", o.explanation())),
                if done {
                    ChatSegment::new(Span::GreenText, "Успех!")
                } else {
                    ChatSegment::new(Span::RedText, "Провал.")
                },
            ]));
        }
        if h.feast_of_owls {
            out.push(line(vec![ChatSegment::new(
                Span::GreenText,
                "Вознесение отринуто",
            )]));
        }
        out.push(line(vec![if h.ascended {
            ChatSegment::new(Span::GreenText, "ЕРЕТИК ВОЗНЕССЯ!")
        } else if succeeded {
            ChatSegment::new(Span::GreenText, "Еретик преуспел, но не вознёсся!")
        } else {
            ChatSegment::new(Span::RedText, "Еретик провалился.")
        }]));
        let names: Vec<String> = h
            .researched
            .iter()
            .map(|r| r.def().name.to_owned())
            .collect();
        out.push(line(vec![
            ChatSegment::new(Span::Bold, "Исследованные знания: "),
            ChatSegment::new(Span::Plain, english_list(&names)),
        ]));
    }
    out
}
