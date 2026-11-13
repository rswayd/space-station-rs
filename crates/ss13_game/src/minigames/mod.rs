//! The ghosts' minigames (server), after tg `/datum/minigames_menu` and the games it opens.
//!
//! - The ghost HUD's "Мини-игры" button (`crate::ghost_verbs`) opens the minigames menu
//!   ([`MinigamesMenuUi`]) on the ghost; a game's button closes it and opens that game's lobby.
//! - [`basketball`]: the basketball lobby and game, its hoops and balls.
//! - [`deathmatch`]: the deathmatch lobbies and games.
// shortcut: Mafia and Capture the Flag are not ported; the menu offers basketball and
// deathmatch only (see `docs/parity/round/minigames.md`).

pub mod basketball;
pub mod deathmatch;

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::ecs::query::QueryItem;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_shared::ui::UiStatus;
use ss13_tgui::server::{AppTguiExt, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow};

use crate::click_handlers::{AddClickHandler, ClickPhase};
use crate::ghost::Ghost;
use crate::minigames_ui::*;
use ss13_net::server::ClientToken;

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
            .register_ui::<DeathmatchPanelUi>()
            .register_ui::<DeathmatchLobbyUi>()
            .add_systems(
                Update,
                (
                    menu_acts,
                    basketball::lobby_acts,
                    deathmatch::panel_acts,
                    deathmatch::lobby_acts,
                )
                    .in_set(UiSystems::Handle),
            )
            .add_systems(
                Update,
                (
                    basketball::refresh_signups,
                    basketball::tick,
                    deathmatch::refresh_members,
                    deathmatch::tick,
                )
                    .before(UiSystems::Open),
            );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAct {
    Basketball,
    Deathmatch,
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
            MENU_DEATHMATCH => Ok(MenuAct::Deathmatch),
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

/// The viewer's key: its body's client's token.
fn viewer_key(
    user: Entity,
    mobs: &Query<'_, '_, &'static PlayerMob>,
    tokens: &Query<'_, '_, &'static ClientToken>,
) -> Option<String> {
    let client = mobs.get(user).ok()?.client;
    tokens.get(client).ok().map(|t| t.0.clone())
}

fn param(params: &UiParams, key: &str) -> Result<String, UiReject> {
    params
        .get(key)
        .map(str::to_owned)
        .ok_or_else(|| UiReject::BadParams(format!("{key}: missing")))
}

impl ServerUi for DeathmatchPanelUi {
    type Source = &'static deathmatch::DeathmatchController;
    type UserParam = (
        Query<'static, 'static, &'static deathmatch::DeathmatchLobby>,
        Query<'static, 'static, &'static PlayerMob>,
        Query<'static, 'static, &'static ClientToken>,
    );
    type Act = deathmatch::PanelAct;

    fn static_data(_: &deathmatch::DeathmatchController) {}

    fn data(
        _: &deathmatch::DeathmatchController,
        user: Entity,
        (lobbies, mobs, tokens): &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> DeathmatchPanelData {
        let key = viewer_key(user, mobs, tokens);
        deathmatch::panel_rows(lobbies.iter(), key.as_deref())
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<deathmatch::PanelAct, UiReject> {
        match action {
            DM_HOST => Ok(deathmatch::PanelAct::Host),
            DM_JOIN => Ok(deathmatch::PanelAct::Join(param(params, "id")?)),
            DM_SPECTATE => Ok(deathmatch::PanelAct::Spectate(param(params, "id")?)),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

impl ServerUi for DeathmatchLobbyUi {
    type Source = &'static deathmatch::DeathmatchLobby;
    type UserParam = (
        Query<'static, 'static, &'static PlayerMob>,
        Query<'static, 'static, &'static ClientToken>,
    );
    type Act = deathmatch::LobbyAct;

    fn static_data(_: &deathmatch::DeathmatchLobby) -> DeathmatchLobbyStatic {
        DeathmatchLobbyStatic {
            maps: deathmatch::MAPS.iter().map(|m| m.name.to_owned()).collect(),
        }
    }

    fn data(
        lobby: &deathmatch::DeathmatchLobby,
        user: Entity,
        (mobs, tokens): &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> DeathmatchLobbyData {
        let key = viewer_key(user, mobs, tokens);
        deathmatch::lobby_data(lobby, key.as_deref())
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<deathmatch::LobbyAct, UiReject> {
        use deathmatch::{HostFunc, LobbyAct};
        Ok(match action {
            DM_START => LobbyAct::Start,
            DM_LEAVE => LobbyAct::Leave,
            DM_OBSERVE => LobbyAct::Observe,
            DM_READY => LobbyAct::Ready,
            DM_CHANGE_LOADOUT => LobbyAct::ChangeLoadout {
                player: param(params, "player")?,
                loadout: param(params, "loadout")?,
            },
            DM_HOST_FUNC => LobbyAct::Host(match param(params, "func")?.as_str() {
                DM_FUNC_KICK => HostFunc::Kick(param(params, "id")?),
                DM_FUNC_TRANSFER => HostFunc::TransferHost(param(params, "id")?),
                DM_FUNC_OBSERVE => HostFunc::ToggleObserve(param(params, "id")?),
                DM_FUNC_MAP => HostFunc::ChangeMap(param(params, "map")?),
                _ => return Err(UiReject::BadParams("func".into())),
            }),
            _ => return Err(UiReject::UnknownAction),
        })
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
            MenuAct::Deathmatch => deathmatch::open_panel(world, client, ghost),
        }
    }
}
