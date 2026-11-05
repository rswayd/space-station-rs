//! Flicks and transform animations from the server (client): each [`AnimationReceived`] is put
//! on the local entity it names, for `ss13_render::animate` to draw.

use bevy::ecs::entity::EntityHashMap;
use bevy::prelude::*;
use ss13_net::animation::client::AnimationReceived;
use ss13_net::client::NetEntityMap;
use ss13_render::animate::{Flick, TransformAnims};
use ss13_render::{Animated, IconAtlas, IconSprite};
use ss13_shared::Appearance;
use ss13_shared::animation::Animation;

/// A flick of a state missing from the manifest, or of zero length, shows this long, seconds.
const FALLBACK_FLICK_SECS: f64 = 0.1;

pub struct AnimationClientPlugin;

impl Plugin for AnimationClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, start_animations);
    }
}

fn start_animations(
    mut received: MessageReader<AnimationReceived>,
    map: Res<NetEntityMap>,
    time: Res<Time>,
    atlas: Option<Res<IconAtlas>>,
    mut drawn: Query<(
        Option<&mut Appearance>,
        Option<&IconSprite>,
        Option<&mut Animated>,
        Option<&mut TransformAnims>,
    )>,
    mut commands: Commands,
    mut fresh: Local<EntityHashMap<TransformAnims>>,
) {
    let now = time.elapsed_secs_f64();
    for AnimationReceived(event) in received.read() {
        // Despawned since (a later batch of the same frame): nothing to draw.
        let Some(entity) = map.get(event.target) else {
            continue;
        };
        let Ok((appearance, icon, frames, anims)) = drawn.get_mut(entity) else {
            continue;
        };
        match &event.animation {
            &Animation::Flick { state, duration_ds } => {
                let secs = match duration_ds {
                    Some(ds) => f64::from(ds) / 10.0,
                    None => atlas
                        .as_ref()
                        .and_then(|a| a.manifest.get(state))
                        .map_or(FALLBACK_FLICK_SECS, |info| {
                            f64::from(ss13_render::anim::cycle_secs(info))
                        }),
                };
                commands.entity(entity).try_insert(Flick {
                    state,
                    until: now + secs.max(FALLBACK_FLICK_SECS),
                });
                // The sprite keeps its frame clock while its state stays: a flick of the state
                // already shown starts over from its first frame.
                if icon.is_some_and(|i| i.state == state)
                    && let Some(mut frames) = frames
                {
                    frames.start = now;
                    frames.next_change = now;
                }
                if let Some(mut a) = appearance {
                    a.set_changed();
                }
            }
            Animation::Transform(anim) => match anims {
                Some(mut anims) => anims.play(anim.clone(), now),
                // Inserted once all are read: two in one frame both run.
                None => fresh.entry(entity).or_default().play(anim.clone(), now),
            },
            Animation::StopTransforms => {
                if let Some(mut anims) = anims {
                    anims.stop();
                }
                fresh.remove(&entity);
            }
        }
    }
    for (entity, anims) in fresh.drain() {
        commands.entity(entity).try_insert(anims);
    }
}
