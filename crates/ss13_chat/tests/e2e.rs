//! Say end to end over real WebTransport on 127.0.0.1: hearing range and z-level, every hostile
//! say the server must drop, and markup shown literally.

use core::net::{Ipv4Addr, SocketAddr};
use core::time::Duration;
use std::time::Instant;

use bevy::prelude::*;
use ss13_chat::client::{ChatClientPlugin, ChatLog};
use ss13_chat::server::{ChatServerPlugin, SAY_MAX_PER_WINDOW, SayReject, SayRejected};
use ss13_net::ServerNotice;
use ss13_net::client::{NetClientConfig, NetClientPlugin, NetClientState, SendIntent};
use ss13_net::server::{
    ClientConnected, ControlledEntity, NetServerConfig, NetServerPlugin, SendNotice,
    ServerCertificateDigest,
};
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, MAX_MESSAGE_LEN, Span};
use ss13_shared::protocol::ClientIntent;
use ss13_world::{SpatialIndex, WorldPlugin};

const SPEAKER: usize = 0;
/// Chebyshev distance 7: a corner of the hearing box.
const NEAR: usize = 1;
/// Distance 8 from the speaker, 7 from NEAR.
const FAR: usize = 2;
/// Same x/y as the speaker, another z-level.
const OTHER_Z: usize = 3;
/// Connected, controls nothing.
const NO_BODY: usize = 4;
const CLIENTS: usize = 5;

fn slot_pos(slot: usize) -> Option<TilePos> {
    match slot {
        SPEAKER => Some(TilePos::new(20, 20, 1)),
        NEAR => Some(TilePos::new(27, 13, 1)),
        FAR => Some(TilePos::new(28, 20, 1)),
        OTHER_Z => Some(TilePos::new(20, 20, 2)),
        _ => None,
    }
}

#[derive(Resource, Default)]
struct ServerLog {
    slots: Vec<Entity>,
    rejected: Vec<(usize, SayReject)>,
}

fn server_app(bind_addr: SocketAddr) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(NetServerConfig {
            bind_addr,
            ..default()
        })
        .add_plugins((WorldPlugin, NetServerPlugin, ChatServerPlugin))
        .init_resource::<ServerLog>()
        .add_systems(Update, (assign_slots, log_rejected));
    let mut index = app.world_mut().resource_mut::<SpatialIndex>();
    index.create_level(1, 50, 50).unwrap();
    index.create_level(2, 50, 50).unwrap();
    app.finish();
    app.cleanup();
    app
}

/// Stand-in for the mob spawner: bodies by connection order; each client is told its slot.
fn assign_slots(
    mut connected: MessageReader<ClientConnected>,
    mut log: ResMut<ServerLog>,
    mut notices: MessageWriter<SendNotice>,
    mut commands: Commands,
) {
    for &ClientConnected { client } in connected.read() {
        let slot = log.slots.len();
        log.slots.push(client);
        if let Some(pos) = slot_pos(slot) {
            let body = commands
                .spawn((pos, Name::new(format!("Игрок {slot}"))))
                .id();
            commands.entity(client).insert(ControlledEntity(body));
        }
        notices.write(SendNotice {
            client,
            notice: ServerNotice {
                text: format!("slot {slot}"),
            },
        });
    }
}

fn log_rejected(mut rejected: MessageReader<SayRejected>, mut log: ResMut<ServerLog>) {
    for r in rejected.read() {
        let slot = log.slots.iter().position(|c| *c == r.client).unwrap();
        log.rejected.push((slot, r.reason));
    }
}

fn client_app(config: NetClientConfig) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(config)
        .add_plugins((NetClientPlugin, ChatClientPlugin));
    app.finish();
    app.cleanup();
    app
}

fn tick(apps: &mut [App]) {
    for app in apps.iter_mut() {
        app.update();
    }
    std::thread::sleep(Duration::from_millis(5));
}

fn run_until(apps: &mut [App], what: &str, mut done: impl FnMut(&[App]) -> bool) {
    let start = Instant::now();
    while !done(apps) {
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "timed out: {what}"
        );
        tick(apps);
    }
}

fn run_for(apps: &mut [App], duration: Duration) {
    let start = Instant::now();
    while start.elapsed() < duration {
        tick(apps);
    }
}

fn messages(app: &App, channel: ChatChannel) -> Vec<ChatMessage> {
    app.world()
        .resource::<ChatLog>()
        .entries()
        .iter()
        .filter(|e| e.message.channel == channel)
        .map(|e| e.message.clone())
        .collect()
}

/// The quoted text of every say this client heard.
fn said(app: &App) -> Vec<String> {
    messages(app, ChatChannel::Local)
        .iter()
        .map(|m| m.segments[2].text.clone())
        .collect()
}

fn warnings(app: &App) -> Vec<String> {
    messages(app, ChatChannel::Warning)
        .iter()
        .map(ChatMessage::plain_text)
        .collect()
}

fn slot_of(app: &App) -> Option<usize> {
    messages(app, ChatChannel::System)
        .iter()
        .find_map(|m| m.plain_text().strip_prefix("slot ")?.parse().ok())
}

fn say(app: &mut App, text: &str) {
    app.world_mut()
        .write_message(SendIntent(ClientIntent::Say(text.into())));
}

fn rejected(server: &App) -> Vec<(usize, SayReject)> {
    server.world().resource::<ServerLog>().rejected.clone()
}

#[test]
fn say_reaches_exactly_the_hearers_and_hostile_says_are_dropped() {
    let server_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
    let mut server = server_app(server_addr);
    server.update();
    let server_addr = server.world().resource::<NetServerConfig>().bind_addr;
    let digest = server
        .world()
        .resource::<ServerCertificateDigest>()
        .0
        .clone();
    let mut apps = vec![server];
    for i in 0..CLIENTS {
        apps.push(client_app(NetClientConfig {
            server_addr,
            certificate_digest: digest.clone(),
            token: format!("c{i}"),
        }));
    }
    // The server notice doubles as the check that notices land in chat as system messages.
    run_until(&mut apps, "handshakes and slots", |apps| {
        apps[1..].iter().all(|app| {
            *app.world().resource::<NetClientState>() == NetClientState::Connected
                && slot_of(app).is_some()
        })
    });
    // Reorder so that apps[1 + slot] is the client in that slot.
    let server = apps.remove(0);
    let mut clients: Vec<(usize, App)> = apps
        .into_iter()
        .map(|app| (slot_of(&app).unwrap(), app))
        .collect();
    clients.sort_by_key(|(slot, _)| *slot);
    let mut apps: Vec<App> = std::iter::once(server)
        .chain(clients.into_iter().map(|(_, app)| app))
        .collect();
    let c = |slot: usize| 1 + slot;

    // Markup in player text arrives as typed, in the say span; only the first letter changes.
    say(
        &mut apps[c(SPEAKER)],
        "привет <b>всем</b> [i]x[/i] <span class='danger'>!</span>",
    );
    run_until(&mut apps, "hearers got the say", |apps| {
        !said(&apps[c(SPEAKER)]).is_empty() && !said(&apps[c(NEAR)]).is_empty()
    });
    run_for(&mut apps, Duration::from_millis(300));
    // BandaStation's IC filter (`sanitize(apply_ic_filter = TRUE)`) cuts the markup characters;
    // what is left stays plain text.
    let expected = "Привет bвсем/b ix/i span class'danger'!/span";
    for slot in [SPEAKER, NEAR] {
        let got = messages(&apps[c(slot)], ChatChannel::Local);
        assert_eq!(got.len(), 1, "slot {slot}");
        let segments = &got[0].segments;
        assert_eq!(
            (segments[0].span, segments[0].text.as_str()),
            (Span::Name, "Игрок 0")
        );
        assert_eq!(
            (segments[2].span, segments[2].text.as_str()),
            (Span::Say, expected)
        );
        assert_eq!(
            got[0].plain_text(),
            format!("Игрок 0 говорит, \"{expected}\"")
        );
    }
    for slot in [FAR, OTHER_Z, NO_BODY] {
        assert!(said(&apps[c(slot)]).is_empty(), "slot {slot} heard the say");
    }

    // Hostile says from the speaker: too long, nothing but controls, controls inside text.
    say(&mut apps[c(SPEAKER)], &"я".repeat(MAX_MESSAGE_LEN + 1));
    say(&mut apps[c(SPEAKER)], "\u{1}\u{7}\u{1b}\u{202e}");
    say(&mut apps[c(SPEAKER)], "a\u{0}b\u{202e}c\nd");
    // A client without a body.
    say(&mut apps[c(NO_BODY)], "меня никто не слышит");
    run_until(&mut apps, "hostile says", |apps| {
        said(&apps[c(NEAR)]).len() == 2 && rejected(&apps[0]).len() >= 3
    });
    run_for(&mut apps, Duration::from_millis(300));
    assert_eq!(said(&apps[c(NEAR)])[1], "Abc d");
    assert_eq!(said(&apps[c(SPEAKER)]).len(), 2);
    let mut got = rejected(&apps[0]);
    got.sort_by_key(|(slot, _)| *slot);
    assert_eq!(
        got,
        [
            (SPEAKER, SayReject::TooLong),
            (SPEAKER, SayReject::Empty),
            (NO_BODY, SayReject::NoBody)
        ]
    );
    // Feedback goes to the sender only; an empty say gets none (tg returns silently).
    assert_eq!(warnings(&apps[c(SPEAKER)]).len(), 1);
    assert!(warnings(&apps[c(SPEAKER)])[0].contains("слишком длинное"));
    assert_eq!(warnings(&apps[c(NO_BODY)]).len(), 1);
    for slot in [FAR, OTHER_Z, NO_BODY] {
        assert!(said(&apps[c(slot)]).is_empty(), "slot {slot}");
        assert!(slot == NO_BODY || warnings(&apps[c(slot)]).is_empty());
    }

    // Spam from the near client: a burst of 12 in one window, 9 pass.
    let before = rejected(&apps[0]).len();
    for i in 0..12 {
        say(&mut apps[c(NEAR)], &format!("спам {i}"));
    }
    let heard = 2 + SAY_MAX_PER_WINDOW as usize;
    run_until(&mut apps, "spam", |apps| {
        rejected(&apps[0]).len() == before + 3 && said(&apps[c(SPEAKER)]).len() == heard
    });
    run_for(&mut apps, Duration::from_millis(300));
    assert!(
        rejected(&apps[0])[before..]
            .iter()
            .all(|r| *r == (NEAR, SayReject::RateLimited))
    );
    assert_eq!(said(&apps[c(SPEAKER)]).len(), heard);
    assert_eq!(said(&apps[c(SPEAKER)]).last().unwrap(), "Спам 8");
    // One warning per window, not one per refused message.
    assert_eq!(warnings(&apps[c(NEAR)]).len(), 1);
    assert!(warnings(&apps[c(NEAR)])[0].contains("слишком часто"));
    // FAR is 7 tiles from NEAR, so it hears NEAR's spam (and only that).
    assert_eq!(said(&apps[c(FAR)]).len(), SAY_MAX_PER_WINDOW as usize);
    for slot in [OTHER_Z, NO_BODY] {
        assert!(said(&apps[c(slot)]).is_empty(), "slot {slot}");
    }
}
