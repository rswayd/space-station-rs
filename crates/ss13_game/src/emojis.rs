//! The emoji set of tg `emoji_parse` (`ss13_chat::emoji`): every icon state of `EMOJI_SET` in
//! the manifest, by lower-case name, refreshed whenever the icon index is (map load).

use bevy::prelude::*;
use ss13_chat::emoji::{EMOJI_SET, Emojis};
use ss13_mapload::IconIndex;

pub struct EmojisPlugin;

impl Plugin for EmojisPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Emojis>().add_systems(
            Update,
            collect_emojis.run_if(resource_exists_and_changed::<IconIndex>),
        );
    }
}

/// Cost: one pass over the icon index per map load.
fn collect_emojis(icons: Res<IconIndex>, mut emojis: ResMut<Emojis>) {
    emojis.0 = emoji_states(&icons);
}

pub fn emoji_states(
    icons: &IconIndex,
) -> std::collections::HashMap<String, ss13_shared::IconStateId> {
    icons
        .0
        .iter()
        .filter(|((dmi, _), _)| dmi == EMOJI_SET)
        .map(|((_, state), &id)| (state.to_lowercase(), id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_shared::IconStateId;

    #[test]
    fn only_the_emoji_set_is_collected() {
        let icons = IconIndex(
            [
                ((EMOJI_SET.to_owned(), "ai".to_owned()), IconStateId(3)),
                (
                    ("icons/obj/doors.dmi".to_owned(), "ai".to_owned()),
                    IconStateId(4),
                ),
            ]
            .into_iter()
            .collect(),
        );
        let got = emoji_states(&icons);
        assert_eq!(got.len(), 1);
        assert_eq!(got["ai"], IconStateId(3));
    }
}
