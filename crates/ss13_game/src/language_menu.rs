//! The language menu, after tg `/datum/language_menu` (`code/modules/language/_language_menu.dm`,
//! tgui `LanguageMenu`) and its HUD button `/atom/movable/screen/language_menu` (`talk_wheel` at
//! `ui_human_language`).
//!
//! - Every player mob gets the button: the headless interface [`LanguageButtonUi`] on the mob
//!   itself, drawn by `crate::language_menu_client`. Its [`OPEN`] act opens the menu.
//! - The menu ([`LanguageMenuUi`], on the mob, for its own player only: tg
//!   `language_menu_state`) lists the languages the mob speaks or understands (even partly):
//!   can it speak it (and physically could it), does it understand it, the `,x` key, and for a
//!   living mob the default language. [`SELECT_DEFAULT`] picks a language the mob can speak.
// shortcut: no admin mode (granting/removing languages, omnitongue): it belongs to the admin
// panel; no language icons (`DmIcon` of `icons/ui/chat/language.dmi`).

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

/// The HUD button (no window).
pub struct LanguageButtonUi;

impl UiInterface for LanguageButtonUi {
    const NAME: &'static str = "LanguageMenuButton";
    type Data = ();
    type StaticData = ();
}

/// [`LanguageButtonUi`]'s act: a click on the button (`Click` -> `open_language_menu`).
pub const OPEN: &str = "click";

/// tgui `LanguageMenu`.
pub struct LanguageMenuUi;

impl UiInterface for LanguageMenuUi {
    const NAME: &'static str = "LanguageMenu";
    type Data = LanguageMenuData;
    type StaticData = ();
}

/// `act('select_default', { language_name })`.
pub const SELECT_DEFAULT: &str = "select_default";
pub const LANGUAGE_NAME: &str = "language_name";

/// `/datum/language_menu/ui_data` without the admin fields.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanguageMenuData {
    /// `isliving(speaker)`: the default language column.
    pub is_living: bool,
    /// Every language (`GLOB.all_languages`).
    pub languages: Vec<LanguageEntry>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanguageEntry {
    pub name: String,
    pub desc: String,
    /// The `,x` key; `None`: only by selection.
    pub key: Option<char>,
    pub is_default: bool,
    /// Mentally: the mob knows how to speak it.
    pub can_speak: bool,
    /// Physically: the tongue allows it (or omnitongue).
    pub could_speak: bool,
    pub can_understand: bool,
    /// Percent understood without knowing it.
    pub partial_understanding: u8,
}

impl LanguageMenuData {
    /// `shown_languages`: the ones spoken, understood or partly understood; spoken ones first,
    /// then understood ones, then by name.
    // tg's sort comparator (`(a.can_speak - b.can_speak) * -2 + (a.name > b.name)`) is not a
    // consistent ordering; this is the order its comment describes.
    pub fn shown(&self) -> Vec<&LanguageEntry> {
        let mut shown: Vec<&LanguageEntry> = self
            .languages
            .iter()
            .filter(|l| l.can_speak || l.can_understand || l.partial_understanding > 0)
            .collect();
        shown.sort_by(|a, b| {
            (!a.can_speak, !a.can_understand, &a.name).cmp(&(
                !b.can_speak,
                !b.can_understand,
                &b.name,
            ))
        });
        shown
    }
}

#[cfg(feature = "server")]
pub use server::*;

#[cfg(feature = "server")]
mod server {
    use bevy::ecs::message::MessageCursor;
    use bevy::ecs::query::QueryItem;
    use bevy::prelude::*;
    use ss13_chat::language::{
        self, LanguageHolder, LanguageId, SPOKEN, Tongue, UNDERSTOOD, basic_understanding,
    };
    use ss13_chat::speech::Tongueless;
    use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

    use super::*;
    use crate::ghost::Ghost;
    use crate::server::PlayerMob;

    /// On a player's mob: the source of its [`LanguageButtonUi`].
    #[derive(Component, Clone, Copy, Debug, Default)]
    pub struct LanguageButton;

    pub struct LanguageMenuServerPlugin;

    impl Plugin for LanguageMenuServerPlugin {
        fn build(&self, app: &mut App) {
            app.register_ui::<LanguageButtonUi>()
                .register_ui::<LanguageMenuUi>()
                .add_observer(give_button)
                .add_observer(take_button)
                .add_systems(
                    Update,
                    (open_menus, select_default)
                        .chain()
                        .in_set(UiSystems::Handle),
                );
        }
    }

    /// A player mob (living or a ghost) gets the button.
    fn give_button(
        add: On<Add<PlayerMob>>,
        players: Query<&PlayerMob>,
        mut open: MessageWriter<OpenUi>,
        mut commands: Commands,
    ) {
        let Ok(player) = players.get(add.entity) else {
            return;
        };
        commands.entity(add.entity).try_insert(LanguageButton);
        open.write(OpenUi::new::<LanguageButtonUi>(
            player.client,
            add.entity,
            add.entity,
            "",
        ));
    }

    /// The mob lost its player: the button's window closes with its source.
    fn take_button(remove: On<Remove<PlayerMob>>, mut commands: Commands) {
        if let Ok(mut mob) = commands.get_entity(remove.entity) {
            mob.try_remove::<LanguageButton>();
        }
    }

    impl ServerUi for LanguageButtonUi {
        type Source = &'static LanguageButton;
        type UserParam = ();
        type Act = ();

        fn static_data(_: &LanguageButton) {}

        fn data(_: &LanguageButton, _: Entity, _: &()) {}

        fn parse_act(action: &str, _: &UiParams) -> Result<(), UiReject> {
            if action == OPEN {
                Ok(())
            } else {
                Err(UiReject::UnknownAction)
            }
        }
    }

    /// `open_language_menu(usr)`: the menu of the clicker's own mob.
    fn open_menus(world: &mut World, mut acts: Local<MessageCursor<UiActed<LanguageButtonUi>>>) {
        let pending: Vec<(Entity, Entity, Entity)> = acts
            .read(world.resource::<Messages<UiActed<LanguageButtonUi>>>())
            .map(|a| (a.client, a.actor, a.source))
            .collect();
        for (client, actor, source) in pending {
            if actor != source || world.get::<LanguageButton>(actor).is_none() {
                continue;
            }
            world.write_message(OpenUi::new::<LanguageMenuUi>(
                client,
                actor,
                actor,
                "Language Menu",
            ));
        }
    }

    /// What the menu reads of the speaker.
    pub type Speaker = (
        Option<&'static LanguageHolder>,
        Option<&'static Tongue>,
        Has<Tongueless>,
        Has<Ghost>,
    );

    /// `ui_data` of a speaker. A body without a holder is tg's `atom_basic` (Galactic Common);
    /// a ghost (tg's universal observer holder) understands everything.
    pub fn menu_data(
        holder: Option<&LanguageHolder>,
        tongue: Option<&Tongue>,
        tongueless: bool,
        ghost: bool,
    ) -> LanguageMenuData {
        let default = language::speaking(holder, tongue, tongueless);
        let omnitongue = holder.is_some_and(|h| h.omnitongue);
        let languages = LanguageId::ALL
            .into_iter()
            .map(|l| {
                let def = l.def();
                let (can_speak, can_understand, partial) = match holder {
                    Some(h) => (
                        h.has(l, SPOKEN),
                        ghost || h.has(l, UNDERSTOOD),
                        h.partial_understanding(l),
                    ),
                    None => {
                        let basic = basic_understanding(l);
                        let common = l == LanguageId::Common;
                        (common, ghost || common, if common { 0 } else { basic })
                    }
                };
                LanguageEntry {
                    name: def.name.to_owned(),
                    desc: def.desc.to_owned(),
                    key: def.key,
                    is_default: default == Some(l),
                    can_speak,
                    could_speak: omnitongue || language::could_speak(tongue, tongueless, l),
                    can_understand,
                    partial_understanding: partial,
                }
            })
            .collect();
        LanguageMenuData {
            is_living: !ghost,
            languages,
        }
    }

    impl ServerUi for LanguageMenuUi {
        type Source = Speaker;
        type UserParam = ();
        type Act = LanguageId;

        fn static_data(_: QueryItem<'_, '_, Speaker>) {}

        fn data(
            (holder, tongue, tongueless, ghost): QueryItem<'_, '_, Speaker>,
            _: Entity,
            _: &(),
        ) -> LanguageMenuData {
            menu_data(holder, tongue, tongueless, ghost)
        }

        fn parse_act(action: &str, params: &UiParams) -> Result<LanguageId, UiReject> {
            if action != SELECT_DEFAULT {
                return Err(UiReject::UnknownAction);
            }
            let name = params.get(LANGUAGE_NAME).unwrap_or_default();
            LanguageId::ALL
                .into_iter()
                .find(|l| l.def().name == name)
                .ok_or_else(|| UiReject::BadParams("language_name: unknown language".into()))
        }
    }

    /// `select_default`: the mob's own menu, a language it can speak now (`can_speak_language`).
    fn select_default(world: &mut World, mut acts: Local<MessageCursor<UiActed<LanguageMenuUi>>>) {
        let pending: Vec<(Entity, Entity, LanguageId)> = acts
            .read(world.resource::<Messages<UiActed<LanguageMenuUi>>>())
            .map(|a| (a.actor, a.source, a.act))
            .collect();
        for (actor, source, l) in pending {
            if actor != source || world.get::<Ghost>(actor).is_some() {
                continue;
            }
            let can_speak = language::can_speak(
                world.get::<LanguageHolder>(actor),
                world.get::<Tongue>(actor),
                world.get::<Tongueless>(actor).is_some(),
                l,
            );
            if can_speak {
                language::edit_holder(world, actor, |h| h.selected = Some(l));
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn a_basic_body_speaks_common_and_partly_understands_its_relatives() {
            let data = menu_data(None, None, false, false);
            assert!(data.is_living);
            let shown = data.shown();
            assert_eq!(shown[0].name, LanguageId::Common.def().name);
            assert!(shown[0].is_default && shown[0].can_speak && shown[0].can_understand);
            assert!(
                shown[1..]
                    .iter()
                    .all(|l| !l.can_speak && l.partial_understanding > 0)
            );
        }

        #[test]
        fn a_ghost_understands_everything_without_a_default_column() {
            let data = menu_data(None, None, false, true);
            assert!(!data.is_living);
            assert_eq!(data.shown().len(), LanguageId::ALL.len());
        }

        #[test]
        fn the_selection_shows_as_the_default() {
            let mut holder = LanguageHolder::universal();
            holder.selected = Some(LanguageId::Draconic);
            let data = menu_data(Some(&holder), Some(&Tongue::Robot), false, false);
            let defaults: Vec<&str> = data
                .languages
                .iter()
                .filter(|l| l.is_default)
                .map(|l| l.name.as_str())
                .collect();
            assert_eq!(defaults, [LanguageId::Draconic.def().name]);
            // Spoken languages come first.
            assert!(data.shown().iter().all(|l| l.can_speak));
        }

        #[test]
        fn unknown_languages_are_rejected() {
            let params = [(LANGUAGE_NAME.to_owned(), "Klingon".to_owned())];
            assert!(LanguageMenuUi::parse_act(SELECT_DEFAULT, &UiParams::new(&params)).is_err());
            let params = [(
                LANGUAGE_NAME.to_owned(),
                LanguageId::Common.def().name.to_owned(),
            )];
            assert_eq!(
                LanguageMenuUi::parse_act(SELECT_DEFAULT, &UiParams::new(&params)),
                Ok(LanguageId::Common)
            );
        }
    }
}
