//! BandaStation's lobby admin buttons (`modular_bandastation/title_screen`: `new_player/Topic`
//! `start_now`/`delay`/`notice`, `change_title_screen_notice`): shown to clients whose active
//! rights include `R_ADMIN` or `R_DEBUG` ([`LobbyAdmin`], kept in sync here), applied here after
//! the rights are checked again; a client without them gets BandaStation's exploit warning.
// shortcut: no background change or YouTube/RuTube video (the HTML title screen and its image
// pool are not ported, see docs/parity/round/lobby.md "Экран-заставка"); "start now" asks no
// confirmation (as the admin panel's); the delay and notice play no sound.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_game::round::{LobbyAdmin, LobbyAdminAct, LobbyAdminRequest, NewPlayer};
use ss13_game::votes::{AdminNotice, VoteRights};
use ss13_net::server::ClientToken;
use ss13_shared::admin::rights::{R_ADMIN, R_DEBUG, R_SERVER};
use ss13_shared::chat::{ChatChannel, Span};

use crate::rights::has;
use crate::server::{
    AdminSession, admin_action, key_name, log_admin, message_admins, rights_of, tell,
};

pub(crate) fn build(app: &mut App) {
    app.add_systems(Update, (sync_lobby_admin, lobby_admin_acts).chain())
        .add_systems(Update, (sync_vote_rights, forward_admin_notices));
}

/// [`LobbyAdmin`] on the lobby clients whose active rights have `R_ADMIN` or `R_DEBUG`. Runs over
/// the lobby only (a few dozen clients at most).
fn sync_lobby_admin(
    players: Query<(Entity, Option<&AdminSession>, Has<LobbyAdmin>), With<NewPlayer>>,
    mut commands: Commands,
) {
    for (client, session, marked) in &players {
        let admin = session.is_some_and(|s| s.active_rights() & (R_ADMIN | R_DEBUG) != 0);
        if admin && !marked {
            commands.entity(client).insert(LobbyAdmin);
        } else if !admin && marked {
            commands.entity(client).remove::<LobbyAdmin>();
        }
    }
}

/// [`VoteRights`] on every client from its active rights. Runs over the connected clients.
fn sync_vote_rights(
    clients: Query<(Entity, Option<&AdminSession>, Option<&VoteRights>), With<ClientToken>>,
    mut commands: Commands,
) {
    for (client, session, current) in &clients {
        let rights = session.map_or(0, AdminSession::active_rights);
        let wanted = VoteRights {
            holder: rights != 0,
            admin: rights & R_ADMIN != 0,
            server: rights & R_SERVER != 0,
        };
        if current != Some(&wanted) {
            commands.entity(client).insert(wanted);
        }
    }
}

/// `message_admins` for what the game crate reports ([`AdminNotice`]).
fn forward_admin_notices(world: &mut World, mut cursor: Local<MessageCursor<AdminNotice>>) {
    let Some(messages) = world.get_resource::<bevy::ecs::message::Messages<AdminNotice>>() else {
        return;
    };
    let notices: Vec<String> = cursor.read(messages).map(|n| n.0.clone()).collect();
    for text in notices {
        message_admins(world, &text);
    }
}

fn lobby_admin_acts(world: &mut World, mut cursor: Local<MessageCursor<LobbyAdminAct>>) {
    let acts: Vec<LobbyAdminAct> = cursor
        .read(world.resource::<Messages<LobbyAdminAct>>())
        .cloned()
        .collect();
    for LobbyAdminAct { client, request } in acts {
        if world.get_entity(client).is_err() {
            continue;
        }
        let rights = rights_of(world, client);
        let key = key_name(world, client);
        if rights & (R_ADMIN | R_DEBUG) == 0 {
            tell(
                world,
                client,
                ChatChannel::System,
                Span::Warning,
                "ТЫ пытаешься использовать АДМИНСКИЕ кнопки! У тебя ничего не выйдет, но мы \
                 оповестили администрацию.",
            );
            let text = format!("Title Screen: Possible href exploit attempt by {key}!");
            log_admin(world, "ADMIN", &text);
            message_admins(world, &text);
            continue;
        }
        match request {
            // `dynamic_invoke_verb(/datum/admin_verb/start_now)`: the verb's own `R_SERVER`.
            LobbyAdminRequest::StartNow => {
                if !has(rights, R_SERVER) {
                    tell(
                        world,
                        client,
                        ChatChannel::System,
                        Span::Warning,
                        "Недостаточно прав! Необходимы права R_SERVER.",
                    );
                    continue;
                }
                match ss13_game::round::start_now(world) {
                    Ok(true) => admin_action(world, &format!("{key} has started the game.")),
                    Ok(false) => admin_action(
                        world,
                        &format!(
                            "{key} has cancelled immediate game start. Game will start in 3 \
                             minutes."
                        ),
                    ),
                    Err(e) => tell(world, client, ChatChannel::System, Span::Warning, e),
                }
            }
            LobbyAdminRequest::Delay => match ss13_game::round::delay_start(world) {
                Ok(secs) => {
                    let text = format!(
                        "{key} set the pre-game delay to {}.",
                        ss13_game::jobs::display_time_text(secs)
                    );
                    log_admin(world, "ADMIN", &text);
                }
                Err(e) => tell(world, client, ChatChannel::System, Span::Warning, e),
            },
            // `change_title_screen_notice`: `R_ADMIN`.
            LobbyAdminRequest::Notice(text) => {
                let Some(text) = text.filter(|_| has(rights, R_ADMIN)) else {
                    continue;
                };
                ss13_game::round::set_notice(world, &text);
                admin_action(
                    world,
                    &format!(
                        "Title Screen: {key} set the title screen notice, which contains: {text}"
                    ),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut app = App::new();
        app.add_message::<LobbyAdminAct>();
        app.add_systems(Update, sync_lobby_admin);
        app
    }

    #[test]
    fn only_active_admin_or_debug_rights_mark_lobby_clients() {
        let mut app = app();
        let session = |rights, active| AdminSession {
            ckey: "a".into(),
            rank: "r".into(),
            rights,
            active,
        };
        let admin = app
            .world_mut()
            .spawn((NewPlayer::default(), session(R_ADMIN, true)))
            .id();
        let debug = app
            .world_mut()
            .spawn((NewPlayer::default(), session(R_DEBUG, true)))
            .id();
        let deadminned = app
            .world_mut()
            .spawn((NewPlayer::default(), session(R_ADMIN, false)))
            .id();
        let fun = app
            .world_mut()
            .spawn((
                NewPlayer::default(),
                session(ss13_shared::admin::rights::R_FUN, true),
            ))
            .id();
        let player = app.world_mut().spawn(NewPlayer::default()).id();
        app.update();
        let marked = |app: &App, e| app.world().get::<LobbyAdmin>(e).is_some();
        assert!(marked(&app, admin) && marked(&app, debug));
        assert!(!marked(&app, deadminned) && !marked(&app, fun) && !marked(&app, player));
        app.world_mut()
            .get_mut::<AdminSession>(admin)
            .unwrap()
            .active = false;
        app.update();
        assert!(!marked(&app, admin));
    }
}
