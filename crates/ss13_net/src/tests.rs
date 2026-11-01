//! In-process end-to-end check over real WebTransport on 127.0.0.1.

use core::net::{Ipv4Addr, SocketAddr};
use core::time::Duration;
use std::time::Instant;

use bevy::prelude::*;
use ss13_shared::Dir;
use ss13_shared::protocol::{ClientIntent, PROTOCOL_VERSION};

use crate::ServerNotice;
use crate::client::{
    HandshakeVersion, NetClientConfig, NetClientPlugin, NetClientState, NoticeReceived, SendIntent,
};
use crate::server::{
    ClientConnected, IntentReceived, NetServerConfig, NetServerPlugin, SendNotice,
    ServerCertificateDigest,
};

#[derive(Resource, Default)]
struct ServerLog {
    connected: Vec<Entity>,
    intents: Vec<(Entity, ClientIntent)>,
}

#[derive(Resource, Default)]
struct Notices(Vec<String>);

fn server_app(bind_addr: SocketAddr) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(NetServerConfig {
            bind_addr,
            ..default()
        })
        .add_plugins(NetServerPlugin)
        .init_resource::<ServerLog>()
        .add_systems(
            Update,
            |mut connected: MessageReader<ClientConnected>,
             mut intents: MessageReader<IntentReceived>,
             mut notices: MessageWriter<SendNotice>,
             mut log: ResMut<ServerLog>| {
                for &ClientConnected { client } in connected.read() {
                    log.connected.push(client);
                    notices.write(SendNotice {
                        client,
                        notice: ServerNotice {
                            text: "welcome".into(),
                        },
                    });
                }
                for IntentReceived { client, intent } in intents.read() {
                    log.intents.push((*client, intent.clone()));
                }
            },
        );
    app.finish();
    app.cleanup();
    app
}

fn client_app(config: NetClientConfig, version: u32) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(config)
        .add_plugins(NetClientPlugin)
        .insert_resource(HandshakeVersion(version))
        .init_resource::<Notices>()
        .add_systems(
            Update,
            |mut received: MessageReader<NoticeReceived>, mut notices: ResMut<Notices>| {
                notices
                    .0
                    .extend(received.read().map(|NoticeReceived(n)| n.text.clone()));
            },
        );
    app.finish();
    app.cleanup();
    app
}

fn run_until(apps: &mut [&mut App], what: &str, mut done: impl FnMut(&[&mut App]) -> bool) {
    let start = Instant::now();
    while !done(apps) {
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "timed out: {what}"
        );
        for app in apps.iter_mut() {
            app.update();
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn state(app: &App) -> &NetClientState {
    app.world().resource::<NetClientState>()
}

fn send(app: &mut App, intent: ClientIntent) {
    app.world_mut().write_message(SendIntent(intent));
}

#[test]
fn two_clients_exchange_intents_and_old_client_is_rejected() {
    let server_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
    let mut server = server_app(server_addr);
    server.update();
    let server_addr = server.world().resource::<NetServerConfig>().bind_addr;
    let digest = server
        .world()
        .resource::<ServerCertificateDigest>()
        .0
        .clone();
    let config = |token: &str| NetClientConfig {
        server_addr,
        certificate_digest: digest.clone(),
        token: token.into(),
    };

    let mut a = client_app(config("a"), PROTOCOL_VERSION);
    let mut b = client_app(config("b"), PROTOCOL_VERSION);
    let mut old = client_app(config("old"), PROTOCOL_VERSION + 1);
    let mut no_token = client_app(config(""), PROTOCOL_VERSION);

    run_until(
        &mut [&mut server, &mut a, &mut b, &mut old, &mut no_token],
        "handshakes",
        |apps| {
            *state(apps[1]) == NetClientState::Connected
                && *state(apps[2]) == NetClientState::Connected
                && matches!(state(apps[3]), NetClientState::Disconnected { .. })
                && matches!(state(apps[4]), NetClientState::Disconnected { .. })
        },
    );
    let NetClientState::Disconnected { reason } = state(&old) else {
        unreachable!()
    };
    assert!(reason.contains("protocol version mismatch"), "{reason}");
    let NetClientState::Disconnected { reason } = state(&no_token) else {
        unreachable!()
    };
    assert!(reason.contains("empty auth token"), "{reason}");

    send(&mut a, ClientIntent::Say("from a".into()));
    send(
        &mut a,
        ClientIntent::Move {
            dir: Dir::North,
            seq: 1,
        },
    );
    send(&mut b, ClientIntent::Say("from b".into()));
    // Dropped locally: the client is not connected.
    send(&mut old, ClientIntent::Say("from old".into()));

    run_until(&mut [&mut server, &mut a, &mut b], "intents", |apps| {
        apps[0].world().resource::<ServerLog>().intents.len() >= 3
            && apps[1..]
                .iter()
                .all(|app| !app.world().resource::<Notices>().0.is_empty())
    });

    let log = server.world().resource::<ServerLog>();
    assert_eq!(log.connected.len(), 2);
    let sender_of = |said: &str| {
        log.intents
            .iter()
            .find(|(_, intent)| *intent == ClientIntent::Say(said.into()))
            .map(|(client, _)| *client)
            .unwrap()
    };
    let (client_a, client_b) = (sender_of("from a"), sender_of("from b"));
    assert_ne!(client_a, client_b);
    assert!(log.connected.contains(&client_a) && log.connected.contains(&client_b));
    assert!(log.intents.contains(&(
        client_a,
        ClientIntent::Move {
            dir: Dir::North,
            seq: 1
        }
    )));
    assert_eq!(log.intents.len(), 3);
    assert_eq!(a.world().resource::<Notices>().0, ["welcome"]);
}
