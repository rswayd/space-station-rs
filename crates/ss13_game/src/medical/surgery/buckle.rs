//! Surgical drapes and buckling to operating tables and beds.

use super::*;

/// tg `/datum/element/surgery_aid/surgery_prep`: drapes toggle the selected bodypart (the chest
/// when the zone has none).
pub fn drapes(world: &mut World, actor: Actor, mob: Entity) {
    let zone = world
        .get::<ZoneSelected>(actor.mob)
        .copied()
        .unwrap_or_default()
        .0
        .check_zone();
    let Some(mut limbs) = world.get::<Limbs>(mob).cloned() else {
        return;
    };
    let limb = limbs.limb_mut(zone);
    let text = if limb.prepped {
        limb.prepped = false;
        "операция отменена".to_owned()
    } else {
        if !limbs.parts.iter().any(|l| l.prepped) {
            limbs.prep_moves = 0;
        }
        limbs.limb_mut(zone).prepped = true;
        format!("{} подготовлена для операции", capitalize(zone.ru().0))
    };
    world.entity_mut(mob).insert(limbs);
    tell(world, actor.client, Span::Notice, text);
}

/// `/datum/status_effect/surgery_prepped/on_move`: four steps standing and the drapes fall off.
pub(super) fn drapes_fall_off(
    mut moved: MessageReader<ss13_world::Moved>,
    mut mobs: Query<(&mut Limbs, &BodyPosition, Option<&Name>)>,
    mut commands: Commands,
) {
    for m in moved.read() {
        let Ok((mut limbs, posture, name)) = mobs.get_mut(m.entity) else {
            continue;
        };
        if !limbs.parts.iter().any(|l| l.prepped) || posture.lying() {
            continue;
        }
        limbs.prep_moves += 1;
        if limbs.prep_moves < 4 {
            continue;
        }
        for limb in &mut limbs.parts {
            limb.prepped = false;
        }
        let mob = m.entity;
        let name = name.map_or_else(|| "кого-то".to_owned(), |n| n.as_str().to_owned());
        commands.queue(move |world: &mut World| {
            let text = format!("Простынь спадает с {name}!");
            super::super::visible(world, mob, &text, &text, Span::Warning);
        });
    }
}

/// A mob buckled to an operating table, a bed (lying) or a chair (sitting, with
/// [`crate::furniture::Seated`]): tg `buckled`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Buckled(pub Entity);

/// The furniture a mob can be buckled to lying down (`can_buckle` with `buckle_lying`).
pub fn buckles_lying(path: &str) -> bool {
    is_under(path, "/obj/structure/table/optable")
        || is_under(path, "/obj/structure/bed")
        || super::super::stasis::is_bed(path)
        || is_under(path, "/obj/structure/altar")
}

/// tg `mouse_drop_receive` -> `mouse_buckle_handling` -> `user_buckle_mob` of a table or bed.
pub fn mouse_drop_receive(
    world: &mut World,
    actor: Actor,
    furniture: Entity,
    dropped: Entity,
) -> bool {
    let path = crate::construction::path_of(world, furniture).unwrap_or_default();
    if !crate::furniture::can_buckle(&path) || world.get::<Health>(dropped).is_none() {
        return false;
    }
    // `nest/user_buckle_mob`: only aliens wrap someone in a nest.
    if crate::antag::xeno::reproduction::nest_refuses(world, actor.mob, &path) {
        return true;
    }
    if furniture_taken(world, furniture) || world.get::<Buckled>(dropped).is_some() {
        return true;
    }
    let same_tile = world.get::<TilePos>(dropped) == world.get::<TilePos>(furniture);
    if dropped != actor.mob && !same_tile {
        let (user, target) = (name_of(world, actor.mob), name_of(world, dropped));
        let what = crate::construction::path_of(world, furniture)
            .map(|_| name_of(world, furniture))
            .unwrap_or_default();
        super::super::visible(
            world,
            actor.mob,
            &format!("Вы начинаете пристегивать {target} к {what}!"),
            &format!("{user} начинает пристегивать {target} к {what}!"),
            Span::Warning,
        );
        do_after(
            world,
            actor,
            dropped,
            BUCKLE_DELAY,
            0,
            crate::construction::Job::Medical(MedJob::Buckle { to: furniture }),
        );
        return true;
    }
    buckle(world, actor, dropped, furniture);
    true
}

pub(super) fn furniture_taken(world: &mut World, furniture: Entity) -> bool {
    world
        .query::<&Buckled>()
        .iter(world)
        .any(|b| b.0 == furniture)
}

/// tg `buckle_mob` + `buckle_feedback` of the operating table, a bed or a chair
/// ([`crate::furniture::post_buckle`]).
pub fn buckle(world: &mut World, actor: Actor, mob: Entity, furniture: Entity) {
    let Some(&pos) = world.get::<TilePos>(furniture) else {
        return;
    };
    let near = world
        .get::<TilePos>(mob)
        .and_then(|p| p.chebyshev(pos))
        .is_some_and(|d| d <= 1);
    if !near || furniture_taken(world, furniture) || world.get::<Buckled>(mob).is_some() {
        return;
    }
    world.entity_mut(mob).insert((pos, Buckled(furniture)));
    if crate::furniture::post_buckle(world, actor, mob, furniture) {
        return;
    }
    let (user, target, what) = (
        name_of(world, actor.mob),
        name_of(world, mob),
        name_of(world, furniture),
    );
    if mob == actor.mob {
        let text = format!("{user} ложится на {what}.");
        super::super::visible(
            world,
            mob,
            &format!("Вы ложитесь на {what}."),
            &text,
            Span::Notice,
        );
    } else {
        super::super::visible(
            world,
            actor.mob,
            &format!("Вы кладёте {target} на {what}."),
            &format!("{user} кладёт {target} на {what}."),
            Span::Notice,
        );
    }
}

/// tg `user_unbuckle_mob` (an empty hand on the furniture, or the buckled mob resisting).
pub fn unbuckle(world: &mut World, actor: Actor, mob: Entity) {
    let Some(Buckled(furniture)) = world.get::<Buckled>(mob).copied() else {
        return;
    };
    if crate::antag::xeno::reproduction::nest_unbuckle(world, actor, mob, furniture) {
        return;
    }
    world.entity_mut(mob).remove::<Buckled>();
    // A rider off a human (`crate::pull::carry`).
    if crate::pull::carry::post_unbuckle(world, actor, mob, furniture) {
        return;
    }
    if crate::furniture::post_unbuckle(world, actor, mob, furniture) {
        return;
    }
    let (user, target, what) = (
        name_of(world, actor.mob),
        name_of(world, mob),
        name_of(world, furniture),
    );
    if mob == actor.mob {
        super::super::visible(
            world,
            mob,
            &format!("Вы встаёте с {what}."),
            &format!("{user} встаёт с {what}."),
            Span::Notice,
        );
    } else {
        super::super::visible(
            world,
            actor.mob,
            &format!("Вы отстёгиваете {target} с {what}."),
            &format!("{user} отстёгивает {target} с {what}."),
            Span::Notice,
        );
    }
}

/// The mob buckled to `furniture`, if any.
pub fn buckled_to(world: &mut World, furniture: Entity) -> Option<Entity> {
    world
        .query::<(Entity, &Buckled)>()
        .iter(world)
        .find(|(_, b)| b.0 == furniture)
        .map(|(e, _)| e)
}

/// tg `/atom/movable/Moved` -> `handle_buckled_mob_movement`: furniture that was pulled or
/// pushed (a medical bed, an office chair) takes its buckled mob along; a vehicle's rider is
/// `crate::vehicles`', a carried mob `crate::pull::carry`'s.
// Cost: per tick, the buckled mobs (a handful) against the moves of the tick.
pub(super) fn carry_buckled(
    mut moved: MessageReader<ss13_world::Moved>,
    mut mobs: Query<
        (&Buckled, &mut TilePos),
        (
            Without<crate::vehicles::Rider>,
            Without<crate::pull::carry::Carried>,
        ),
    >,
) {
    let moved: Vec<(Entity, TilePos)> = moved.read().map(|m| (m.entity, m.to)).collect();
    if moved.is_empty() {
        return;
    }
    for (Buckled(to), mut pos) in &mut mobs {
        if let Some(&(_, dest)) = moved.iter().rev().find(|(e, _)| e == to)
            && *pos != dest
        {
            *pos = dest;
        }
    }
}

/// A buckled mob that left the furniture's tile (teleported, the furniture gone) is free; a
/// vehicle's rider is `crate::vehicles`' (it moves with the vehicle), a carried mob
/// `crate::pull::carry`'s.
pub(super) fn check_buckles(
    mobs: Query<
        (Entity, &Buckled, &TilePos),
        (
            Without<crate::vehicles::Rider>,
            Without<crate::pull::carry::Carried>,
        ),
    >,
    furniture: Query<&TilePos>,
    mut commands: Commands,
) {
    for (mob, Buckled(to), pos) in &mobs {
        if furniture.get(*to).ok() != Some(pos) {
            commands.entity(mob).remove::<Buckled>();
        }
    }
}

/// A buckled mob resisting gets up (tg `resist` -> `resist_buckle`); a cuffed one is left to
/// `crate::restraints`.
pub(super) fn resist_buckle(
    mut intents: MessageReader<ss13_net::server::IntentReceived>,
    players: Res<crate::server::Players>,
    mobs: Query<(), (With<Buckled>, Without<crate::restraints::Handcuffed>)>,
    mut commands: Commands,
) {
    for intent in intents.read() {
        if !matches!(intent.intent, ss13_shared::protocol::ClientIntent::Resist) {
            continue;
        }
        let client = intent.client;
        let Some(mob) = players.mob(client).filter(|m| mobs.contains(*m)) else {
            continue;
        };
        commands.queue(move |world: &mut World| {
            let actor = Actor {
                client,
                mob,
                item: None,
            };
            unbuckle(world, actor, mob);
        });
    }
}
