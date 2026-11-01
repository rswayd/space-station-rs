//! The in-memory transport against the real server and client plugins: rejection, hostile frames,
//! kicks and leaving. The game-level join/move/replicate check is `ss13_game`'s `it` `loopback::`.

use bevy::prelude::*;
use ss13_shared::protocol::{ClientIntent, PROTOCOL_VERSION};

use crate::client::{HandshakeVersion, NetClientConfig, NetClientPlugin, NetClientState};
use crate::loopback::{LoopbackConnector, listener};
use crate::protocol::{Hello, MAX_CLIENT_FRAME, MAX_HELLO_FRAME, close, encode_frame};
use crate::server::{
    ClientConnected, ClientDisconnected, IntentReceived, KickClient, NetServerPlugin,
};

#[derive(Resource, Default)]
struct Log {
    connected: Vec<Entity>,
    disconnected: Vec<String>,
    intents: Vec<ClientIntent>,
}

fn server_app() -> (App, LoopbackConnector) {
    let (listener, connector) = listener();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(listener)
        .add_plugins(NetServerPlugin)
        .init_resource::<Log>()
        .add_systems(
            Update,
            |mut connected: MessageReader<ClientConnected>,
             mut gone: MessageReader<ClientDisconnected>,
             mut intents: MessageReader<IntentReceived>,
             mut log: ResMut<Log>| {
                log.connected.extend(connected.read().map(|c| c.client));
                log.disconnected
                    .extend(gone.read().map(|d| d.reason.clone()));
                log.intents.extend(intents.read().map(|i| i.intent.clone()));
            },
        );
    app.finish();
    app.cleanup();
    (app, connector)
}

fn client_app(connector: &LoopbackConnector, version: u32) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(connector.clone())
        .insert_resource(NetClientConfig {
            token: "dev".into(),
            ..default()
        })
        .add_plugins(NetClientPlugin)
        .insert_resource(HandshakeVersion(version));
    app.finish();
    app.cleanup();
    app
}

/// Everything is pumped by `update`: a few frames settle any exchange.
fn step(apps: &mut [&mut App]) {
    for _ in 0..4 {
        for app in apps.iter_mut() {
            app.update();
        }
    }
}

fn state(app: &App) -> NetClientState {
    app.world().resource::<NetClientState>().clone()
}

#[test]
fn old_client_is_rejected_with_the_reason() {
    let (mut server, connector) = server_app();
    let mut old = client_app(&connector, PROTOCOL_VERSION + 1);
    step(&mut [&mut server, &mut old]);
    let NetClientState::Disconnected { reason } = state(&old) else {
        panic!("{:?}", state(&old));
    };
    assert!(reason.contains("protocol version mismatch"), "{reason}");
    assert!(server.world().resource::<Log>().connected.is_empty());
}

#[test]
fn hostile_frames_close_the_session() {
    let (mut server, connector) = server_app();

    // A hello over its limit, then one whose prefix lies about its length.
    let mut big = connector.connect().unwrap();
    let token = "x".repeat(MAX_HELLO_FRAME);
    big.send_frame(&encode_frame(&Hello { version: 1, token }, usize::MAX).unwrap());
    let mut lying = connector.connect().unwrap();
    lying.send_frame(&[200, 0, 0, 0, 1, 2]);
    step(&mut [&mut server]);
    for end in [&mut big, &mut lying] {
        let (code, reason) = end.recv_frame().unwrap_err();
        assert_eq!(code, close::PROTOCOL, "{reason}");
    }

    // An accepted client sending an undecodable message.
    let mut raw = connector.connect().unwrap();
    let hello = Hello {
        version: PROTOCOL_VERSION,
        token: "raw".into(),
    };
    raw.send_frame(&encode_frame(&hello, MAX_HELLO_FRAME).unwrap());
    step(&mut [&mut server]);
    assert!(raw.recv_frame().unwrap().is_some(), "hello reply");
    raw.send_frame(&encode_frame(&u8::MAX, MAX_CLIENT_FRAME).unwrap());
    step(&mut [&mut server]);
    assert_eq!(raw.recv_frame().unwrap_err().0, close::PROTOCOL);
    let log = server.world().resource::<Log>();
    assert_eq!(log.connected.len(), 1);
    assert_eq!(log.disconnected.len(), 1);
}

#[test]
fn intents_arrive_a_kick_is_final_and_leaving_disconnects() {
    let (mut server, connector) = server_app();
    let mut a = client_app(&connector, PROTOCOL_VERSION);
    let mut b = client_app(&connector, PROTOCOL_VERSION);
    step(&mut [&mut server, &mut a, &mut b]);
    assert_eq!(state(&a), NetClientState::Connected);
    assert_eq!(state(&b), NetClientState::Connected);
    a.world_mut()
        .write_message(crate::client::SendIntent(ClientIntent::Say("hi".into())));
    step(&mut [&mut server, &mut a, &mut b]);
    assert_eq!(
        server.world().resource::<Log>().intents,
        [ClientIntent::Say("hi".into())]
    );

    let [first, second] = server.world().resource::<Log>().connected[..] else {
        panic!("two clients");
    };
    server.world_mut().write_message(KickClient {
        client: first,
        reason: "bye".into(),
    });
    step(&mut [&mut server, &mut a, &mut b]);
    assert_eq!(
        state(&a),
        NetClientState::Disconnected {
            reason: "rejected by server: bye".into()
        }
    );
    drop(b);
    step(&mut [&mut server, &mut a]);
    let log = server.world().resource::<Log>();
    assert_eq!(log.disconnected.len(), 2, "{:?}", log.disconnected);
    // No reconnect after a kick.
    step(&mut [&mut server, &mut a]);
    assert_eq!(server.world().resource::<Log>().connected, [first, second]);
}
