//! tg's visuals of a timed action (`code/datums/progressbar.dm`, `code/datums/cogbar.dm`,
//! `/datum/timed_action`): while a [`DoAfter`] runs,
//!
//! - its player sees a bar over the target (`targets[1] || user`; a carried target shows it
//!   over its carrier) that fills over the action's time and, when the action stops early,
//!   turns to its `_fail` state; it fades out either way ([`Animation::Progress`],
//!   [`Animation::ProgressEnd`], to the player's client only, as tg's `client.images`);
//! - everyone else who sees the mob sees a cog over it when the action takes
//!   [`COG_MIN_SECS`] or more ([`Animation::Cog`]; tg hides it from the user with a blank
//!   override image, the client here skips its own mob).
//!
//! `crate::progressbar_client` draws both.
//!
//! Actions tg runs with `show_progress = FALSE` show neither, those with `cog_icon = null` no
//! cog: the sneaky ones others must not notice ([`visuals`]).
// shortcut: no custom cog icons (`cog_icon`/`cog_iconstate`: every cog is the default one).
// shortcut: a client that starts seeing the mob mid-action gets no cog (animations are not
// replayed to late viewers).

use bevy::prelude::*;
use ss13_shared::animation::Animation;

use crate::construction::Job;
use crate::tools::DoAfter;

/// tg `timed_action/New`: a cog only for actions of `1 SECONDS` or more.
pub const COG_MIN_SECS: f64 = 1.0;

/// What a running [`DoAfter`] shows: the client and the entity its bar is over, and whether
/// the mob has a cog.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shown {
    pub client: Option<Entity>,
    pub bar_on: Entity,
    pub cog: bool,
}

/// Whether `job` shows its player a bar and the others a cog (tg `show_progress`, `cog_icon`).
pub fn visuals(job: &Job) -> (bool, bool) {
    use crate::antag::heretic::server::HereticJob;
    use crate::clothing::ClothingJob;
    use crate::clothing::laces::LaceJob;
    use crate::restraints::RestraintJob;
    match job {
        // `show_progress = FALSE`: `/obj/item/chisel` sculpting (`statues.dm`).
        Job::Art(crate::service::ArtJob::Sculpt) => (false, false),
        // `cog_icon = null`.
        // Changeling `absorb`, heretic `draw_rune` and influence `drain`, the spy's theft,
        // the vampire's bite, the voidwalker's wall phasing.
        Job::Changeling(_)
        | Job::Heretic(HereticJob::DrawRune | HereticJob::Drain { .. })
        | Job::Spy(_)
        | Job::Vampire(crate::antag::vampire::server::VampireJob::Bite)
        | Job::Voidwalker(crate::antag::voidwalker::VoidJob::Wall)
        // `cuff_resist` and `resist_buckle` while restrained (`carbon.dm`); applying cuffs
        // shows the cog.
        | Job::Restraints(
            RestraintJob::Resist { .. }
            | RestraintJob::ResistSuit { .. }
            | RestraintJob::ResistLegs { .. }
            | RestraintJob::Break { .. }
            | RestraintJob::Snap { .. },
        )
        // `handle_tying` on someone else's shoes, the chameleon scanner and the agent ID's
        // scan, the syndicate spraycan's rune, escaping an alien nest, `up()`/`down()` flight.
        | Job::Clothing(
            ClothingJob::Laces {
                job: LaceJob::Other,
                ..
            }
            | ClothingJob::ChameleonScan { .. },
        )
        | Job::AgentCardScan { .. }
        | Job::Crayon(crate::crayons::CrayonJob::Rune(_))
        | Job::Xeno(crate::antag::xeno::abilities::XenoJob::NestEscape)
        | Job::Fly(_) => (true, false),
        _ => (true, true),
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.add_observer(start).add_observer(end);
}

fn start(add: On<Add<DoAfter>>, mut commands: Commands) {
    let mob = add.entity;
    // Queued: runs once the insert is done, with the action still on the mob.
    commands.queue(move |world: &mut World| show(world, mob));
}

fn show(world: &mut World, mob: Entity) {
    let Some(action) = world.get::<DoAfter>(mob) else {
        return;
    };
    let secs = action.ends_at - crate::tools::now(world);
    let target = action.target;
    let (bar, cog) = visuals(&action.job);
    let client = crate::tools::client_of(world, mob).filter(|_| bar);
    let bar_on = crate::balloon::atom_on_turf(world, target).unwrap_or(mob);
    let shown = Shown {
        client,
        bar_on,
        cog: cog && secs >= COG_MIN_SECS,
    };
    if let Some(client) = client {
        play_for(
            world,
            client,
            bar_on,
            Animation::Progress {
                duration_ds: (secs.max(0.0) * 10.0).round() as u32,
            },
        );
    }
    if shown.cog {
        crate::animation::play(world, mob, Animation::Cog { on: true });
    }
    world.entity_mut(mob).insert(shown);
}

fn end(
    remove: On<Remove<DoAfter>>,
    actions: Query<(&DoAfter, &Shown)>,
    time: Res<Time>,
    mut commands: Commands,
) {
    let mob = remove.entity;
    let Ok((action, &shown)) = actions.get(mob) else {
        return;
    };
    // tg `end_progress`: the bar is full only when the action ran its whole time.
    let success = time.elapsed_secs_f64() >= action.ends_at;
    commands.queue(move |world: &mut World| {
        if let Ok(mut e) = world.get_entity_mut(mob)
            && e.get::<Shown>() == Some(&shown)
            && e.get::<DoAfter>().is_none()
        {
            e.remove::<Shown>();
        }
        if let Some(client) = shown.client {
            play_for(
                world,
                client,
                shown.bar_on,
                Animation::ProgressEnd { success },
            );
        }
        if shown.cog && world.get_entity(mob).is_ok() {
            crate::animation::play(world, mob, Animation::Cog { on: false });
        }
    });
}

fn play_for(world: &mut World, client: Entity, entity: Entity, animation: Animation) {
    use bevy::ecs::message::Messages;
    use ss13_net::animation::server::PlayAnimationFor;
    if world.contains_resource::<Messages<PlayAnimationFor>>() {
        world.write_message(PlayAnimationFor {
            client,
            entity,
            animation,
        });
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::message::Messages;
    use ss13_net::animation::server::{PlayAnimation, PlayAnimationFor};
    use ss13_shared::TilePos;

    use super::*;
    use crate::construction::Job;

    fn drain_for(app: &mut App) -> Vec<(Entity, Entity, Animation)> {
        app.world_mut()
            .resource_mut::<Messages<PlayAnimationFor>>()
            .drain()
            .map(|m| (m.client, m.entity, m.animation))
            .collect()
    }

    fn drain_all(app: &mut App) -> Vec<(Entity, Animation)> {
        app.world_mut()
            .resource_mut::<Messages<PlayAnimation>>()
            .drain()
            .map(|m| (m.entity, m.animation))
            .collect()
    }

    #[test]
    fn a_timed_action_shows_its_player_a_bar_and_others_a_cog_until_it_stops() {
        let mut app = App::new();
        app.init_resource::<Time>()
            .add_message::<PlayAnimationFor>()
            .add_message::<PlayAnimation>()
            .add_plugins(plugin);
        let at = TilePos::new(1, 1, 1);
        let client = app.world_mut().spawn_empty().id();
        let mob = app
            .world_mut()
            .spawn((at, crate::server::PlayerMob { client }))
            .id();
        let target = app.world_mut().spawn(TilePos::new(2, 1, 1)).id();
        let action = |secs: f64| DoAfter {
            client,
            target,
            target_pos: TilePos::new(2, 1, 1),
            user_pos: at,
            item: None,
            amount: 0,
            ends_at: secs,
            job: Job::GunSuicide,
        };

        app.world_mut().entity_mut(mob).insert(action(3.0));
        app.world_mut().flush();
        assert_eq!(
            drain_for(&mut app),
            vec![(client, target, Animation::Progress { duration_ds: 30 })]
        );
        assert_eq!(
            drain_all(&mut app),
            vec![(mob, Animation::Cog { on: true })]
        );

        // Stopped before its time: a failed bar, and the cog goes.
        app.world_mut().entity_mut(mob).remove::<DoAfter>();
        app.world_mut().flush();
        assert_eq!(
            drain_for(&mut app),
            vec![(client, target, Animation::ProgressEnd { success: false })]
        );
        assert_eq!(
            drain_all(&mut app),
            vec![(mob, Animation::Cog { on: false })]
        );
        assert!(app.world().get::<Shown>(mob).is_none());

        // Under a second: a bar but no cog; done on time: a full bar.
        app.world_mut().entity_mut(mob).insert(action(0.5));
        app.world_mut().flush();
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs(1));
        app.world_mut().entity_mut(mob).remove::<DoAfter>();
        app.world_mut().flush();
        assert_eq!(
            drain_for(&mut app),
            vec![
                (client, target, Animation::Progress { duration_ds: 5 }),
                (client, target, Animation::ProgressEnd { success: true }),
            ]
        );
        assert!(drain_all(&mut app).is_empty());

        // A sneaky action: its player's bar, no cog for the others.
        let mut sneaky = action(5.0);
        sneaky.job = Job::Fly(crate::multiz::Climb { up: true });
        app.world_mut().entity_mut(mob).insert(sneaky);
        app.world_mut().flush();
        assert_eq!(drain_for(&mut app).len(), 1);
        assert!(drain_all(&mut app).is_empty());
        app.world_mut().entity_mut(mob).remove::<DoAfter>();
        app.world_mut().flush();
        assert!(drain_all(&mut app).is_empty());

        // Sculpting shows nothing at all.
        assert_eq!(
            visuals(&Job::Art(crate::service::ArtJob::Sculpt)),
            (false, false)
        );
    }
}
