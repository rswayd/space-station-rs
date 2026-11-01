//! Hostile and failing peers over real QUIC on 127.0.0.1: raw `wtransport` clients that break the
//! protocol, and a server restart under a real client.

use core::future::Future;
use core::net::{Ipv4Addr, SocketAddr};
use core::time::Duration;
use std::time::Instant;

use bevy::prelude::*;
use ss13_shared::protocol::PROTOCOL_VERSION;
use wtransport::endpoint::endpoint_side::Client;
use wtransport::{ClientConfig, Connection, Endpoint, Identity, RecvStream, SendStream};

use crate::ServerNotice;
use crate::client::{NetClientConfig, NetClientPlugin, NetClientState};
use crate::client_quic::parse_digest;
use crate::protocol::{
    Hello, HelloReply, MAX_CLIENT_FRAME, MAX_HELLO_FRAME, close, encode_frame, read_frame, runtime,
};
use crate::server::{
    ClientConnected, ClientDisconnected, ClientLink, NetServerConfig, NetServerPlugin,
    SEND_QUEUE_CAP, SEND_STALL_TIMEOUT, SendNotice, ServerCertificateDigest, TlsFiles,
};

/// Port 0: the server binds a free one, read back from `NetServerConfig` after `Startup`.
fn free_port() -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, 0))
}

#[derive(Resource, Default)]
struct Log {
    connected: Vec<Entity>,
    disconnected: Vec<(Entity, String)>,
    /// Largest send queue of any client seen at the end of a frame.
    max_queued: usize,
}

/// Bytes of notice text to send every tick to every connected client.
#[derive(Resource, Default)]
struct Flood(usize);

fn server_app(config: NetServerConfig) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(config)
        .add_plugins(NetServerPlugin)
        .init_resource::<Log>()
        .init_resource::<Flood>()
        .add_systems(
            Update,
            |mut connected: MessageReader<ClientConnected>,
             mut gone: MessageReader<ClientDisconnected>,
             mut log: ResMut<Log>,
             flood: Res<Flood>,
             mut notices: MessageWriter<SendNotice>| {
                log.connected.extend(connected.read().map(|c| c.client));
                log.disconnected
                    .extend(gone.read().map(|d| (d.client, d.reason.clone())));
                if flood.0 > 0 {
                    let alive: Vec<Entity> = log
                        .connected
                        .iter()
                        .copied()
                        .filter(|c| !log.disconnected.iter().any(|(d, _)| d == c))
                        .collect();
                    for client in alive {
                        notices.write(SendNotice {
                            client,
                            notice: ServerNotice {
                                text: "x".repeat(flood.0),
                            },
                        });
                    }
                }
            },
        )
        .add_systems(Last, |links: Query<&ClientLink>, mut log: ResMut<Log>| {
            for link in &links {
                log.max_queued = log.max_queued.max(link.queued());
            }
        });
    app.finish();
    app.cleanup();
    app.update();
    app
}

fn digest(server: &App) -> String {
    server
        .world()
        .resource::<ServerCertificateDigest>()
        .0
        .clone()
}

/// Runs `future` on the transport runtime while updating `server`; panics after `secs`.
fn drive<T: Send + 'static>(
    server: &mut App,
    secs: u64,
    future: impl Future<Output = T> + Send + 'static,
) -> T {
    let task = runtime().spawn(future);
    let start = Instant::now();
    while !task.is_finished() {
        assert!(start.elapsed() < Duration::from_secs(secs), "timed out");
        server.update();
        std::thread::sleep(Duration::from_millis(5));
    }
    runtime().block_on(task).unwrap()
}

/// A WebTransport session without our handshake. The endpoint must outlive the connection.
async fn raw_session(
    addr: SocketAddr,
    digest: String,
) -> Result<(Endpoint<Client>, Connection), String> {
    let config = ClientConfig::builder()
        .with_bind_default()
        .with_server_certificate_hashes([parse_digest(&digest).unwrap()])
        .build();
    let endpoint = Endpoint::client(config).map_err(|e| e.to_string())?;
    let conn = endpoint
        .connect(format!("https://{addr}"))
        .await
        .map_err(|e| e.to_string())?;
    Ok((endpoint, conn))
}

async fn raw_stream(conn: &Connection) -> (SendStream, RecvStream) {
    conn.open_bi().await.unwrap().await.unwrap()
}

async fn hello(send: &mut SendStream, recv: &mut RecvStream, token: &str) -> HelloReply {
    let frame = encode_frame(
        &Hello {
            version: PROTOCOL_VERSION,
            token: token.into(),
        },
        MAX_HELLO_FRAME,
    )
    .unwrap();
    send.write_all(&frame).await.unwrap();
    read_frame(recv, MAX_HELLO_FRAME, &mut Vec::new())
        .await
        .unwrap()
}

/// Close code and reason the server gave.
async fn closed(conn: &Connection) -> (u32, String) {
    match conn.closed().await {
        wtransport::error::ConnectionError::ApplicationClosed(close) => (
            close.code().into_inner() as u32,
            String::from_utf8_lossy(close.reason()).into_owned(),
        ),
        other => (u32::MAX, other.to_string()),
    }
}

#[test]
fn oversized_and_garbage_frames_close_the_session() {
    let addr = free_port();
    let mut server = server_app(NetServerConfig {
        bind_addr: addr,
        ..default()
    });
    let addr = server.world().resource::<NetServerConfig>().bind_addr;
    let digest = digest(&server);

    // Before the handshake: a 4 GiB length prefix is refused from the prefix alone.
    let d = digest.clone();
    let (code, reason) = drive(&mut server, 10, async move {
        let (_endpoint, conn) = raw_session(addr, d).await.unwrap();
        let (mut send, _recv) = raw_stream(&conn).await;
        send.write_all(&u32::MAX.to_le_bytes()).await.unwrap();
        closed(&conn).await
    });
    assert_eq!(code, close::PROTOCOL, "{reason}");
    assert!(
        reason.contains("4294967295 bytes is over the limit"),
        "{reason}"
    );

    // After the handshake: one byte over the client frame limit, then bytes that do not decode.
    for payload in [
        ((MAX_CLIENT_FRAME + 1) as u32).to_le_bytes().to_vec(),
        [5u32.to_le_bytes().to_vec(), vec![0xff; 5]].concat(),
    ] {
        let d = digest.clone();
        let (code, reason) = drive(&mut server, 10, async move {
            let (_endpoint, conn) = raw_session(addr, d).await.unwrap();
            let (mut send, mut recv) = raw_stream(&conn).await;
            assert_eq!(hello(&mut send, &mut recv, "t").await, HelloReply::Accepted);
            send.write_all(&payload).await.unwrap();
            closed(&conn).await
        });
        assert_eq!(code, close::PROTOCOL, "{reason}");
    }
    // Both accepted clients were reported gone.
    for _ in 0..10 {
        server.update();
    }
    let log = server.world().resource::<Log>();
    assert_eq!((log.connected.len(), log.disconnected.len()), (2, 2));
}

#[test]
fn handshake_timeout_and_per_ip_cap() {
    let addr = free_port();
    let mut server = server_app(NetServerConfig {
        bind_addr: addr,
        max_clients_per_ip: 4,
        ..default()
    });
    let addr = server.world().resource::<NetServerConfig>().bind_addr;
    let digest = digest(&server);

    // Ten sessions at once from one address: four admitted, held without a hello, and closed by
    // the handshake timeout; the other six refused at the first packet.
    let start = Instant::now();
    let d = digest.clone();
    let outcomes = drive(&mut server, 30, async move {
        let attempts = (0..10).map(|_| {
            let d = d.clone();
            tokio::spawn(async move {
                match raw_session(addr, d).await {
                    Ok((_endpoint, conn)) => {
                        let _stream = raw_stream(&conn).await;
                        Ok(closed(&conn).await)
                    }
                    Err(e) => Err(e),
                }
            })
        });
        let mut outcomes = Vec::new();
        for attempt in attempts.collect::<Vec<_>>() {
            outcomes.push(attempt.await.unwrap());
        }
        outcomes
    });
    let elapsed = start.elapsed();
    let admitted: Vec<_> = outcomes.iter().filter_map(|o| o.as_ref().ok()).collect();
    assert_eq!(admitted.len(), 4, "{outcomes:?}");
    for (code, reason) in admitted {
        assert_eq!(*code, close::HANDSHAKE_TIMEOUT, "{reason}");
    }
    assert!(
        elapsed >= Duration::from_secs(4) && elapsed < Duration::from_secs(8),
        "{elapsed:?}"
    );

    // The slots are free again.
    let reply = drive(&mut server, 10, async move {
        let (_endpoint, conn) = raw_session(addr, digest).await.unwrap();
        let (mut send, mut recv) = raw_stream(&conn).await;
        hello(&mut send, &mut recv, "t").await
    });
    assert_eq!(reply, HelloReply::Accepted);
}

#[test]
fn client_that_does_not_read_is_dropped_with_bounded_memory() {
    let addr = free_port();
    let mut server = server_app(NetServerConfig {
        bind_addr: addr,
        ..default()
    });
    let addr = server.world().resource::<NetServerConfig>().bind_addr;
    let digest = digest(&server);
    let notice = 60 * 1024;
    server.insert_resource(Flood(notice));

    // 1.2 MB/s to a client that never reads its stream.
    let start = Instant::now();
    let (code, reason) = drive(&mut server, SEND_STALL_TIMEOUT.as_secs() + 10, async move {
        let (_endpoint, conn) = raw_session(addr, digest).await.unwrap();
        let (mut send, mut recv) = raw_stream(&conn).await;
        assert_eq!(hello(&mut send, &mut recv, "t").await, HelloReply::Accepted);
        closed(&conn).await
    });
    assert_eq!(code, close::SLOW_READER, "{reason}");
    assert!(
        start.elapsed() < SEND_STALL_TIMEOUT,
        "{:?}",
        start.elapsed()
    );
    let log = server.world().resource::<Log>();
    // One frame may go in while the queue is just under the cap.
    assert!(
        log.max_queued <= SEND_QUEUE_CAP + notice + 64,
        "{}",
        log.max_queued
    );
    println!(
        "max queued {} bytes, dropped after {:?}",
        log.max_queued,
        start.elapsed()
    );
}

#[test]
fn client_whose_reads_stall_is_dropped_after_the_deadline() {
    let addr = free_port();
    let mut server = server_app(NetServerConfig {
        bind_addr: addr,
        ..default()
    });
    let addr = server.world().resource::<NetServerConfig>().bind_addr;
    let digest = digest(&server);

    let d = digest.clone();
    let (conn, _endpoint, _streams) = drive(&mut server, 10, async move {
        let (endpoint, conn) = raw_session(addr, d).await.unwrap();
        let (mut send, mut recv) = raw_stream(&conn).await;
        assert_eq!(hello(&mut send, &mut recv, "t").await, HelloReply::Accepted);
        (conn, endpoint, (send, recv))
    });
    // One burst over the client's stream window (1.25 MB) and below the queue cap, then nothing:
    // only the stalled write can tell.
    server.insert_resource(Flood(60 * 1024));
    for _ in 0..30 {
        server.update();
    }
    server.insert_resource(Flood(0));
    let start = Instant::now();
    let (code, reason) = drive(&mut server, SEND_STALL_TIMEOUT.as_secs() + 10, async move {
        closed(&conn).await
    });
    assert_eq!(code, close::SLOW_READER, "{reason}");
    assert!(reason.contains("did not read"), "{reason}");
    assert!(start.elapsed() <= SEND_STALL_TIMEOUT + Duration::from_secs(2));
}

#[test]
fn client_reconnects_after_server_restart() {
    let dir = std::env::temp_dir().join(format!("ss13-net-restart-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let tls = TlsFiles {
        cert_chain: dir.join("cert.pem"),
        private_key: dir.join("key.pem"),
    };
    let identity = Identity::self_signed(["localhost", "127.0.0.1"]).unwrap();
    runtime().block_on(async {
        identity
            .certificate_chain()
            .store_pemfile(&tls.cert_chain)
            .await
            .unwrap();
        identity
            .private_key()
            .store_secret_pemfile(&tls.private_key)
            .await
            .unwrap();
    });
    let addr = free_port();
    let config = NetServerConfig {
        bind_addr: addr,
        tls: Some(tls),
        ..default()
    };
    let mut server = server_app(config);
    // With the port the first server bound, for the restart.
    let config = server.world().resource::<NetServerConfig>().clone();
    let addr = config.bind_addr;
    let digest = digest(&server);

    let mut client = App::new();
    client
        .add_plugins(MinimalPlugins)
        .insert_resource(NetClientConfig {
            server_addr: addr,
            certificate_digest: digest.clone(),
            token: "t".into(),
        })
        .add_plugins(NetClientPlugin);
    client.finish();
    client.cleanup();
    let state = |client: &App| client.world().resource::<NetClientState>().clone();
    let run = |apps: &mut [&mut App], what: &str, done: &dyn Fn(&App) -> bool| {
        let start = Instant::now();
        while !done(apps[apps.len() - 1]) {
            assert!(
                start.elapsed() < Duration::from_secs(20),
                "timed out: {what}"
            );
            for app in apps.iter_mut() {
                app.update();
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    run(&mut [&mut server, &mut client], "first connect", &|c| {
        state(c) == NetClientState::Connected
    });

    drop(server);
    run(&mut [&mut client], "disconnect", &|c| {
        matches!(state(c), NetClientState::Disconnected { .. })
    });
    let NetClientState::Disconnected { reason } = state(&client) else {
        unreachable!()
    };
    // Whichever the app drops first: the endpoint or the client's link.
    assert!(
        reason == "server shutting down" || reason == "server closed the session",
        "{reason}"
    );

    // Same port and certificate; the client retries on its own.
    std::thread::sleep(Duration::from_millis(300));
    let mut server = server_app(config);
    assert_eq!(super::transport_tests::digest(&server), digest);
    run(&mut [&mut server, &mut client], "reconnect", &|c| {
        state(c) == NetClientState::Connected
    });
    assert_eq!(server.world().resource::<Log>().connected.len(), 1);
    std::fs::remove_dir_all(&dir).ok();
}
