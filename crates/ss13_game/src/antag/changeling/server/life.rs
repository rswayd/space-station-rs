//! Life: chemicals and the hivemind.

use super::*;

/// tg `COMSIG_LIVING_LIFE` -> `on_life` each `SSmobs` fire: the chemicals.
// Cost: per changeling per 2 s, a few component reads.
pub(super) fn changeling_life(world: &mut World, mut last_fired: Local<Option<u64>>) {
    let fired = world.resource::<crate::breath::MobsClock>().times_fired;
    let fires = fired.saturating_sub(last_fired.unwrap_or(fired));
    *last_fired = Some(fired);
    if fires == 0 {
        return;
    }
    let secs = crate::breath::SSMOBS_WAIT_SECS as f32 * fires as f32;
    let mobs: Vec<Entity> = world
        .query_filtered::<Entity, With<Changeling>>()
        .iter(world)
        .collect();
    for mob in mobs {
        let stasis = world
            .get::<Changeling>(mob)
            .map(|c| c.stasis)
            .unwrap_or_default();
        let dead = world.get::<Stat>(mob) == Some(&Stat::Dead)
            && (world.get::<DeathComa>(mob).is_none() || stasis.died());
        void_check_environment(world, mob);
        let on_fire = crate::fire::on_fire(world, mob);
        if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
            ling.regenerate_chemicals(secs, dead, on_fire);
        }
        refresh(world, mob);
    }
}

/// Per tick: interrupted absorptions, the stasis timer and early revivals, fleshmend's healing.
// Cost: per tick, one pass over the changelings.
pub(super) fn tick_changelings(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let delta = world.resource::<Time>().delta_secs();
    let mobs: Vec<Entity> = world
        .query_filtered::<Entity, With<Changeling>>()
        .iter(world)
        .collect();
    for mob in mobs {
        let Some(ling) = world.get::<Changeling>(mob).cloned() else {
            continue;
        };
        // `do_after` failed (moved, incapacitated, target moved): "прервано!".
        let absorbing = world
            .get::<DoAfter>(mob)
            .is_some_and(|d| d.job == Job::Changeling(ChangelingJob::Absorb));
        if ling.absorbing.is_some() && !absorbing {
            absorb_interrupted(world, mob);
        }
        match ling.stasis {
            // `on_stat_change`: revived from death by other means (a defibrillator, an admin).
            Stasis::Waiting { .. } | Stasis::Ready { .. }
                if world.get::<Stat>(mob) != Some(&Stat::Dead) =>
            {
                world.entity_mut(mob).remove::<DeathComa>();
                if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
                    ling.stasis = Stasis::None;
                }
                chat_to(
                    world,
                    mob,
                    Span::Changeling,
                    "Мы выходим из стазиса раньше.",
                );
                refresh(world, mob);
            }
            // `ready_to_regenerate`.
            Stasis::Waiting { ready_at, died } if now >= ready_at => {
                if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
                    ling.stasis = Stasis::Ready { died };
                }
                chat_to(world, mob, Span::Changeling, "Мы готовы возродиться.");
                refresh(world, mob);
            }
            _ => {}
        }
        // `/datum/status_effect/fleshmend/tick`, frame-independent; paused while burning.
        if ling.fleshmend_until > now
            && world.get::<Stat>(mob) != Some(&Stat::Dead)
            && !crate::fire::on_fire(world, mob)
        {
            for (kind, rate) in FLESHMEND_HEAL {
                hurt(world, mob, kind, -rate * delta);
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Hivemind

/// `/datum/saymode/changeling/handle_message`: every hivemind member and ghost reads
/// `<id>: message`.
pub(super) fn hivemind_chat(
    mut spoken: MessageReader<Spoke>,
    members: Query<Entity, With<ChangelingHivemind>>,
    lings: Query<&Changeling>,
    ghosts: Query<Entity, With<Ghost>>,
    listeners: Res<Listeners>,
    mut chat: MessageWriter<SendChat>,
) {
    for spoke in spoken.read() {
        if spoke.radio != Some(RadioTarget::Changeling) {
            continue;
        }
        let id = lings
            .get(spoke.speaker)
            .map_or_else(|_| spoke.name.clone(), |c| c.id_name.clone());
        let clients: Vec<Entity> = members
            .iter()
            .chain(ghosts.iter())
            .filter_map(|e| listeners.client_of(e))
            .collect();
        info!(target: "say", "hivemind {id} ({}): {}", spoke.name, spoke.text);
        chat.write(SendChat {
            clients,
            message: hivemind_message(&id, &spoke.text),
        });
    }
}

/// `span_changeling("<b>[id]:</b> [message]")`.
pub fn hivemind_message(id: &str, text: &str) -> ChatMessage {
    ChatMessage {
        channel: ChatChannel::Radio,
        segments: vec![
            ChatSegment::new(Span::Changeling, format!("{id}: ")),
            ChatSegment::new(Span::Changeling, text),
        ],
    }
}
