//! The ghosts' minigames (server), after tg `/datum/minigames_menu` and the games it opens.
//!
//! - The ghost HUD's "Мини-игры" button (`crate::ghost_verbs`) opens the minigames menu
//!   ([`MinigamesMenuUi`]) on the ghost; a game's button closes it and opens that game's lobby.
//! - [`basketball`]: the basketball lobby and game, its hoops and balls.
// shortcut: Mafia, Capture the Flag and Deathmatch are not ported; the menu offers basketball
// only (see `docs/parity/round/minigames.md`).

pub mod basketball;

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::ecs::query::QueryItem;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_shared::ui::UiStatus;
use ss13_tgui::server::{AppTguiExt, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow};

use crate::click_handlers::{AddClickHandler, ClickPhase};
use crate::ghost::Ghost;
use crate::minigames_ui::*;
use crate::server::{PlayerMob, Players};

pub struct MinigamesPlugin;

impl Plugin for MinigamesPlugin {
    fn build(&self, app: &mut App) {
        // Balls passed to carbons, items dunked into hoops.
        app.add_click_handler(ClickPhase::HeldOnTarget, 3663, |w, c| {
            basketball::held_on_target(w, c)
        });
        // A ball its scorer may not pick up yet, a grabbed mob dunked into a hoop.
        app.add_click_handler(ClickPhase::EmptyHand, 3613, |w, c| {
            basketball::empty_hand(w, c)
        });
        app.register_ui::<MinigamesMenuUi>()
            .register_ui::<BasketballPanelUi>()
            .add_systems(
                Update,
                (menu_acts, basketball::lobby_acts).in_set(UiSystems::Handle),
            )
            .add_systems(
                Update,
                (basketball::refresh_signups, basketball::tick).before(UiSystems::Open),
            );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAct {
    Basketball,
}

impl ServerUi for MinigamesMenuUi {
    type Source = &'static Ghost;
    type UserParam = ();
    type Act = MenuAct;

    fn static_data(_: &Ghost) {}

    fn data(_: &Ghost, _: Entity, _: &()) {}

    fn parse_act(action: &str, _: &UiParams) -> Result<MenuAct, UiReject> {
        match action {
            MENU_BASKETBALL => Ok(MenuAct::Basketball),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

impl ServerUi for BasketballPanelUi {
    type Source = &'static basketball::BasketballLobby;
    type UserParam = ();
    type Act = basketball::LobbyAct;

    fn static_data(_: QueryItem<'_, '_, Self::Source>) {}

    fn data(
        lobby: QueryItem<'_, '_, Self::Source>,
        _: Entity,
        _: &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> BasketballPanelData {
        basketball::panel_data(lobby)
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<basketball::LobbyAct, UiReject> {
        match action {
            BASKETBALL_SIGNUP => Ok(basketball::LobbyAct::Signup),
            BASKETBALL_START => Ok(basketball::LobbyAct::Start),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// The menu's acts on the client's own ghost: `ui.close()`, then the game's window.
fn menu_acts(world: &mut World, mut cursor: Local<MessageCursor<UiActed<MinigamesMenuUi>>>) {
    let acts: Vec<(Entity, Entity, Entity, Entity, MenuAct)> = cursor
        .read(world.resource::<Messages<UiActed<MinigamesMenuUi>>>())
        .map(|a| (a.window, a.client, a.actor, a.source, a.act))
        .collect();
    for (window, client, ghost, source, act) in acts {
        let own = ghost == source
            && world.get::<Ghost>(ghost).is_some()
            && world.get::<PlayerMob>(ghost).map(|p| p.client) == Some(client)
            && world.resource::<Players>().mob(client) == Some(ghost);
        if !own {
            continue;
        }
        if let Some(mut w) = world.get_mut::<UiWindow>(window) {
            w.cap_status(UiStatus::Close);
        }
        match act {
            MenuAct::Basketball => basketball::open_lobby(world, client, ghost),
        }
    }
}
