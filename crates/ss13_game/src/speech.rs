//! The mob's stat for speech (server): `ss13_chat` reads [`SpeakerStat`] to whisper from soft
//! crit, silence hard crit, send the dead and ghosts to deadchat and check emotes.

use bevy::prelude::*;
use ss13_chat::speech::SpeakerStat;

use crate::ghost::Ghost;
use crate::health::{HealthSystems, Stat};

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

/// Follows stat changes (and spawns) only.
fn sync_speaker_stat(
    mut mobs: Query<(Entity, &Stat, Option<&mut SpeakerStat>), Changed<Stat>>,
    mut commands: Commands,
) {
    for (mob, &stat, speaker) in &mut mobs {
        let wanted = speaker_stat(stat);
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
