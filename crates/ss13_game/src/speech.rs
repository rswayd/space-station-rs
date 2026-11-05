//! The mob's stat for speech (server): `ss13_chat` reads [`SpeakerStat`] to whisper from soft
//! crit, silence hard crit, send the dead and ghosts to deadchat and check emotes. A mob knocked
//! out by `Unconscious` or `Sleeping` (tg stat `UNCONSCIOUS`) speaks and emotes as in hard crit.

use bevy::prelude::*;
use ss13_chat::speech::SpeakerStat;

use crate::ghost::Ghost;
use crate::health::{HealthSystems, Stat};
use crate::status_effects::Incapacitation;

pub struct SpeechServerPlugin;

impl Plugin for SpeechServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, sync_speaker_stat.after(HealthSystems))
            .add_observer(ghost_speaks_dead);
    }
}

pub fn speaker_stat(stat: Stat) -> SpeakerStat {
    match stat {
        Stat::Conscious => SpeakerStat::Conscious,
        Stat::SoftCrit => SpeakerStat::SoftCrit,
        Stat::HardCrit => SpeakerStat::HardCrit,
        Stat::Dead => SpeakerStat::Dead,
    }
}

/// Follows stat and knock-out changes (and spawns) only.
fn sync_speaker_stat(
    mut mobs: Query<
        (
            Entity,
            &Stat,
            Option<&Incapacitation>,
            Option<&mut SpeakerStat>,
        ),
        Or<(Changed<Stat>, Changed<Incapacitation>)>,
    >,
    mut commands: Commands,
) {
    for (mob, &stat, traits, speaker) in &mut mobs {
        let mut wanted = speaker_stat(stat);
        if traits.is_some_and(|t| t.knocked_out) && wanted < SpeakerStat::HardCrit {
            wanted = SpeakerStat::HardCrit;
        }
        match speaker {
            Some(mut speaker) => {
                speaker.set_if_neq(wanted);
            }
            None => {
                commands.entity(mob).insert(wanted);
            }
        }
    }
}

fn ghost_speaks_dead(add: On<Add<Ghost>>, mut commands: Commands) {
    commands.entity(add.entity).try_insert(SpeakerStat::Dead);
}
