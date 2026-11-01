//! Server and client in one process over real WebTransport: the client clicks a thermomachine,
//! the server opens its `ThermoMachine` window, and every button goes through `ui_act`.
//!
//! `cargo run -p ss13_tgui --all-features --example thermo_demo`
//! `... --example thermo_demo -- --screenshot out.png` presses "power" once, saves a PNG, exits.

use core::net::{Ipv4Addr, SocketAddr};
use std::sync::mpsc;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::ui_widgets::Activate;
use ss13_net::client::{NetClientConfig, NetClientPlugin, NetClientState, SendIntent};
use ss13_net::server::{
    ClientConnected, IntentReceived, NetServerConfig, NetServerPlugin, ServerCertificateDigest,
};
use ss13_shared::TilePos;
use ss13_shared::protocol::{ClickModifiers, ClientIntent, NetId, TICK_DURATION};
use ss13_tgui::client::{ClientWindows, TguiClientPlugin};
use ss13_tgui::interfaces::thermo_machine::{self, ThermoMachine, ThermoMachineUi};
use ss13_tgui::server::{OpenUi, TguiServerPlugin};
use ss13_ui_kit::{Act, UiKitPlugin};

const MACHINE: NetId = NetId(1);

fn main() {
    let screenshot = std::env::args().skip_while(|a| a != "--screenshot").nth(1);
    let addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 7787);
    let (digest_tx, digest_rx) = mpsc::channel();
    std::thread::spawn(move || run_server(addr, digest_tx));
    let digest = digest_rx.recv().expect("server started");

    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins.set(WindowPlugin {
            primary_window: Some(bevy::window::Window {
                title: "ss13_tgui: ThermoMachine".into(),
                resolution: (420, 420).into(),
                ..default()
            }),
            ..default()
        }),
        UiKitPlugin,
    ))
    .insert_resource(ClearColor(Color::srgb(0.05, 0.06, 0.08)))
    .insert_resource(NetClientConfig {
        server_addr: addr,
        certificate_digest: digest,
        token: "demo".into(),
    })
    .add_plugins((NetClientPlugin, TguiClientPlugin))
    .add_systems(Startup, |mut commands: Commands| {
        commands.spawn(Camera2d);
    })
    .add_systems(Update, click_machine_once);
    if let Some(path) = screenshot {
        app.add_systems(Update, script_screenshot(path));
    }
    app.run();
}

fn click_machine_once(
    state: Res<NetClientState>,
    mut sent: Local<bool>,
    mut intents: MessageWriter<SendIntent>,
) {
    if !*sent && *state == NetClientState::Connected {
        *sent = true;
        intents.write(SendIntent(ClientIntent::Click {
            target: MACHINE,
            modifiers: ClickModifiers::default(),
        }));
    }
}

/// Presses "power" once the window is up, then screenshots after the update arrived.
fn script_screenshot(
    path: String,
) -> impl FnMut(Res<ClientWindows>, Query<(Entity, &Act)>, Local<u32>, Commands, MessageWriter<AppExit>)
{
    move |windows, acts, mut frames, mut commands, mut exit| {
        if windows.0.is_empty() {
            return;
        }
        *frames += 1;
        if *frames == 20
            && let Some((power, _)) = acts.iter().find(|(_, a)| a.0 == "power")
        {
            commands.trigger(Activate { entity: power });
        }
        if *frames == 60 {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path.clone()));
        }
        if *frames == 70 {
            exit.write(AppExit::Success);
        }
    }
}

/// A 20 Hz headless server on its own thread; logs go through the client's `LogPlugin`.
fn run_server(addr: SocketAddr, digest: mpsc::Sender<String>) {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(NetServerConfig {
            bind_addr: addr,
            ..default()
        })
        .add_plugins((NetServerPlugin, TguiServerPlugin, thermo_machine::plugin))
        .add_systems(Update, (spawn_mob, open_on_click));
    app.world_mut()
        .spawn((TilePos::new(5, 5, 1), ThermoMachine::default()));
    app.finish();
    app.cleanup();
    app.update();
    let _ = digest.send(app.world().resource::<ServerCertificateDigest>().0.clone());
    loop {
        app.update();
        std::thread::sleep(TICK_DURATION);
    }
}

#[derive(Component)]
struct Mob(Entity);

fn spawn_mob(mut connected: MessageReader<ClientConnected>, mut commands: Commands) {
    for &ClientConnected { client } in connected.read() {
        commands.spawn((TilePos::new(5, 6, 1), Mob(client)));
    }
}

fn open_on_click(
    mut intents: MessageReader<IntentReceived>,
    mobs: Query<(Entity, &Mob)>,
    machines: Query<Entity, With<ThermoMachine>>,
    mut open: MessageWriter<OpenUi>,
) {
    for IntentReceived { client, intent } in intents.read() {
        if let ClientIntent::Click { target, .. } = intent
            && *target == MACHINE
            && let Some((mob, _)) = mobs.iter().find(|(_, m)| m.0 == *client)
            && let Ok(machine) = machines.single()
        {
            open.write(OpenUi::new::<ThermoMachineUi>(
                *client,
                mob,
                machine,
                "thermomachine",
            ));
        }
    }
}
