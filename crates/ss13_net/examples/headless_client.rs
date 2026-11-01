//! Headless client for a manual real-network check.
//!
//! ```text
//! cargo run -p ss13_server            # logs "certificate digest <hex>"
//! SS13_CERT_DIGEST=<hex> cargo run -p ss13_net --features client --example headless_client
//! ```
//! Also reads `SS13_SERVER_ADDR` and `SS13_TOKEN`. Sends a few intents once connected, then exits.

use std::time::Duration;

use bevy::app::ScheduleRunnerPlugin;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use ss13_net::client::{NetClientPlugin, NetClientState, NoticeReceived, SendIntent};
use ss13_shared::Dir;
use ss13_shared::protocol::ClientIntent;

fn main() -> AppExit {
    App::new()
        .add_plugins((
            MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_millis(50))),
            LogPlugin::default(),
            NetClientPlugin,
        ))
        .add_systems(Update, (log_notices, drive))
        .run()
}

fn log_notices(mut notices: MessageReader<NoticeReceived>) {
    for NoticeReceived(notice) in notices.read() {
        info!("server notice: {}", notice.text);
    }
}

/// Sends one intent per second while connected; exits after five, or on disconnect.
fn drive(
    state: Res<NetClientState>,
    time: Res<Time>,
    mut next_send: Local<f32>,
    mut sent: Local<u32>,
    mut intents: MessageWriter<SendIntent>,
    mut exit: MessageWriter<AppExit>,
) {
    match &*state {
        NetClientState::Connecting => {}
        NetClientState::Disconnected { reason } => {
            if *sent > 0 || reason != "not connected" {
                info!("exiting: {reason}");
                exit.write(AppExit::Success);
            }
        }
        NetClientState::Connected => {
            if *sent == 5 {
                info!("sent {} intents, exiting", *sent);
                exit.write(AppExit::Success);
            } else if time.elapsed_secs() >= *next_send {
                *next_send = time.elapsed_secs() + 1.0;
                let intent = if sent.is_multiple_of(2) {
                    ClientIntent::Say(format!("hello #{}", *sent))
                } else {
                    ClientIntent::Move {
                        dir: Dir::North,
                        seq: *sent,
                    }
                };
                info!("sending {intent:?}");
                intents.write(SendIntent(intent));
                *sent += 1;
            }
        }
    }
}
