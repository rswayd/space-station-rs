//! A tgui window end to end over real WebTransport on 127.0.0.1: open, state, act, update, and
//! every hostile act the server must drop.

use core::net::{Ipv4Addr, SocketAddr};
use core::time::Duration;
use std::time::Instant;

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy::scene::ScenePlugin;
use bevy::ui_widgets::Activate;
use ss13_net::client::{NetClientConfig, NetClientPlugin, NetClientState, SendIntent};
use ss13_net::server::{
    ClientConnected, IntentReceived, NetServerConfig, NetServerPlugin, ServerCertificateDigest,
};
use ss13_shared::TilePos;
use ss13_shared::protocol::{ClickModifiers, ClientIntent, NetId};
use ss13_shared::ui::UiStatus;
use ss13_tgui::client::{ActOnChange, ClientWindow, ClientWindows, TguiClientPlugin};
use ss13_tgui::interfaces::thermo_machine::{
    self, ThermoMachine, ThermoMachineAct, ThermoMachineUi,
};
use ss13_tgui::server::{
    OpenUi, TguiServerPlugin, UiActRejected, UiActed, UiReject, UiSystems, UiWindow,
};
use ss13_ui_kit::{Act, ActParams, NumberInputChanged, UiKitPlugin, WindowClose};

const MACHINE: NetId = NetId(1);

#[derive(Resource, Default)]
struct ServerLog {
    /// Connection entity → that client's mob, in connection order.
    mobs: Vec<(Entity, Entity)>,
    acted: Vec<(Entity, ThermoMachineAct)>,
    rejected: Vec<(Entity, String, UiReject)>,
}

#[derive(Resource)]
struct Machine(Entity);

fn server_app(bind_addr: SocketAddr) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(NetServerConfig {
            bind_addr,
            ..default()
        })
        .add_plugins((NetServerPlugin, TguiServerPlugin, thermo_machine::plugin))
        .init_resource::<ServerLog>()
        .add_systems(
            Update,
            (
                spawn_mobs,
                click_opens,
                // Same tick as the effect, so a log check right after a state check holds.
                log_acts.after(UiSystems::Handle),
            ),
        );
    let machine = app
        .world_mut()
        .spawn((TilePos::new(5, 5, 1), ThermoMachine::default()))
        .id();
    app.insert_resource(Machine(machine));
    app.finish();
    app.cleanup();
    app
}

/// Stand-in for the world: every client gets a mob next to the machine.
fn spawn_mobs(
    mut connected: MessageReader<ClientConnected>,
    mut log: ResMut<ServerLog>,
    mut commands: Commands,
) {
    for &ClientConnected { client } in connected.read() {
        let mob = commands.spawn(TilePos::new(5, 6, 1)).id();
        log.mobs.push((client, mob));
    }
}

/// Stand-in for the click handler (`attack_hand` → `ui_interact`) until `NetId`s are replicated.
fn click_opens(
    mut intents: MessageReader<IntentReceived>,
    log: Res<ServerLog>,
    machine: Res<Machine>,
    mut open: MessageWriter<OpenUi>,
) {
    for IntentReceived { client, intent } in intents.read() {
        if let ClientIntent::Click { target, .. } = intent
            && *target == MACHINE
            && let Some(&(_, mob)) = log.mobs.iter().find(|(c, _)| c == client)
        {
            open.write(OpenUi::new::<ThermoMachineUi>(
                *client,
                mob,
                machine.0,
                "thermomachine",
            ));
        }
    }
}

fn log_acts(
    mut acted: MessageReader<UiActed<ThermoMachineUi>>,
    mut rejected: MessageReader<UiActRejected>,
    mut log: ResMut<ServerLog>,
) {
    for a in acted.read() {
        log.acted.push((a.client, a.act));
    }
    for r in rejected.read() {
        log.rejected
            .push((r.client, r.action.clone(), r.reason.clone()));
    }
}

fn client_app(config: NetClientConfig) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        ScenePlugin,
        UiKitPlugin,
    ))
    // Headless: no `TextPlugin`, but the theme loads its fonts into the asset store.
    .init_asset::<Font>()
    .init_resource::<bevy::input_focus::InputFocus>()
    .insert_resource(config)
    .add_plugins((NetClientPlugin, TguiClientPlugin));
    app.finish();
    app.cleanup();
    app
}

fn tick(apps: &mut [&mut App]) {
    for app in apps.iter_mut() {
        app.update();
    }
    std::thread::sleep(Duration::from_millis(5));
}

fn run_until(apps: &mut [&mut App], what: &str, mut done: impl FnMut(&mut [&mut App]) -> bool) {
    let start = Instant::now();
    while !done(apps) {
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "timed out: {what}"
        );
        tick(apps);
    }
}

fn run_for(apps: &mut [&mut App], duration: Duration) {
    let start = Instant::now();
    while start.elapsed() < duration {
        tick(apps);
    }
}

fn send(app: &mut App, intent: ClientIntent) {
    app.world_mut().write_message(SendIntent(intent));
}

fn click(app: &mut App) {
    send(
        app,
        ClientIntent::Click {
            target: MACHINE,
            modifiers: ClickModifiers::default(),
        },
    );
}

fn act(window: u32, action: &str, params: &[(&str, &str)]) -> ClientIntent {
    ClientIntent::UiAct {
        window,
        action: action.into(),
        params: params
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    }
}

/// The client's only window: (server id, root, status).
fn client_window(app: &mut App) -> Option<(u32, Entity, UiStatus)> {
    let world = app.world_mut();
    let windows: Vec<_> = world
        .query::<(Entity, &ClientWindow)>()
        .iter(world)
        .map(|(e, w)| (w.id, e, w.status))
        .collect();
    assert!(windows.len() <= 1);
    windows.first().copied()
}

fn server_windows(server: &mut App) -> usize {
    let world = server.world_mut();
    world.query::<&UiWindow>().iter(world).count()
}

/// Whether `e` is in a hidden subtree (a re-render still being laid out). Read from `Visibility`:
/// this app has no visibility propagation.
fn hidden(world: &World, e: Entity) -> bool {
    std::iter::successors(Some(e), |e| world.get::<ChildOf>(*e).map(ChildOf::parent))
        .any(|e| world.get::<Visibility>(e) == Some(&Visibility::Hidden))
}

fn texts_under(world: &World, root: Entity) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(e) = stack.pop() {
        if hidden(world, e) {
            continue;
        }
        if let Some(text) = world.get::<Text>(e) {
            out.push(text.0.clone());
        }
        if let Some(children) = world.get::<Children>(e) {
            stack.extend(children.iter());
        }
    }
    out
}

fn shows(app: &App, text: &str) -> bool {
    app.world()
        .resource::<ClientWindows>()
        .0
        .values()
        .any(|root| texts_under(app.world(), *root).iter().any(|t| t == text))
}

/// The client entity with `Act(action)` and, if given, exactly these params.
fn widget(app: &mut App, action: &str, params: Option<&[(&str, &str)]>) -> Entity {
    let world = app.world_mut();
    let found: Vec<Entity> = world
        .query::<(Entity, &Act, Option<&ActParams>)>()
        .iter(world)
        .filter(|(e, a, p)| {
            !hidden(world, *e)
                && a.0 == action
                && params.is_none_or(|want| {
                    p.is_some_and(|p| {
                        p.0.iter()
                            .map(|(k, v)| (k.as_str(), v.as_str()))
                            .eq(want.iter().copied())
                    })
                })
        })
        .map(|(e, ..)| e)
        .collect();
    assert_eq!(found.len(), 1, "widget {action} {params:?}");
    found[0]
}

fn machine(server: &App) -> ThermoMachine {
    let entity = server.world().resource::<Machine>().0;
    server.world().get::<ThermoMachine>(entity).unwrap().clone()
}

fn rejected(server: &App) -> Vec<(Entity, String, UiReject)> {
    server.world().resource::<ServerLog>().rejected.clone()
}

fn move_mob(server: &mut App, client: Entity, to: TilePos) {
    let mob = server
        .world()
        .resource::<ServerLog>()
        .mobs
        .iter()
        .find(|(c, _)| *c == client)
        .unwrap()
        .1;
    server.world_mut().entity_mut(mob).insert(to);
}

#[test]
fn thermomachine_window_end_to_end() {
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
    let mut a = client_app(config("a"));
    let mut b = client_app(config("b"));
    let apps = &mut [&mut server, &mut a, &mut b];
    run_until(apps, "handshakes", |apps| {
        apps[1..]
            .iter()
            .all(|app| *app.world().resource::<NetClientState>() == NetClientState::Connected)
            && apps[0].world().resource::<ServerLog>().mobs.len() == 2
    });

    // Client A clicks the machine; the server opens the window and sends the state.
    click(apps[1]);
    run_until(apps, "window open", |apps| {
        shows(apps[1], "293.15 K") && shows(apps[1], "Off")
    });
    let (window, _, status) = client_window(apps[1]).unwrap();
    assert_eq!(status, UiStatus::Interactive);
    assert!(client_window(apps[2]).is_none());
    let (client_a, client_b) = {
        let log = apps[0].world().resource::<ServerLog>();
        (log.mobs[0].0, log.mobs[1].0)
    };
    let owner = {
        let world = apps[0].world_mut();
        let w = world.query::<&UiWindow>().single(world).unwrap();
        assert_eq!(w.id, window);
        w.client
    };
    // Connection order may differ from app order.
    let (client_a, client_b) = if owner == client_a {
        (client_a, client_b)
    } else {
        (client_b, client_a)
    };
    // Clicking again refreshes the same window instead of opening a second one.
    click(apps[1]);
    run_for(apps, Duration::from_millis(100));
    assert_eq!(server_windows(apps[0]), 1);

    // Power button → act → server toggles → update re-renders "On".
    let power = widget(apps[1], "power", None);
    apps[1].world_mut().trigger(Activate { entity: power });
    run_until(apps, "power on", |apps| shows(apps[1], "On"));
    assert!(machine(apps[0]).on);

    // Preset button with params: target = max.
    let max = machine(apps[0]).max_temperature.to_string();
    let to_max = widget(apps[1], "target", Some(&[("target", &max)]));
    apps[1].world_mut().trigger(Activate { entity: to_max });
    run_until(apps, "target max", |apps| {
        machine(apps[0]).target_temperature == 573.15
    });

    // NumberInput onChange → act('target', {target: value}).
    let number = {
        let world = apps[1].world_mut();
        let shown: Vec<Entity> = world
            .query_filtered::<Entity, With<ActOnChange>>()
            .iter(world)
            .filter(|e| !hidden(world, *e))
            .collect();
        assert_eq!(shown.len(), 1, "one shown NumberInput");
        shown[0]
    };
    apps[1].world_mut().trigger(NumberInputChanged {
        entity: number,
        value: 300.0,
    });
    run_until(apps, "target 300", |apps| {
        machine(apps[0]).target_temperature == 300.0
    });
    // The stub machine warms toward 300 K; the autoupdate shows it without any act.
    run_until(apps, "autoupdate", |apps| !shows(apps[1], "293.15 K"));
    assert_eq!(apps[0].world().resource::<ServerLog>().acted.len(), 3);

    // Hostile acts: each is rejected and leaves the machine as it was.
    let before = machine(apps[0]);
    send(apps[1], act(window + 1000, "power", &[]));
    send(apps[2], act(window, "power", &[]));
    send(apps[1], act(window, "explode", &[]));
    send(apps[1], act(window, "target", &[("target", "abc")]));
    send(apps[1], act(window, "target", &[("target", "inf")]));
    send(apps[1], act(window, "target", &[]));
    send(
        apps[1],
        act(window, "target", &[("target", "100"), ("target", "200")]),
    );
    let long = "x".repeat(1000);
    send(apps[1], act(window, &long, &[]));
    run_until(apps, "hostile acts", |apps| rejected(apps[0]).len() >= 8);
    let got = rejected(apps[0]);
    let reasons = |client: Entity, action: &str| {
        got.iter()
            .filter(|(c, a, _)| *c == client && a == action)
            .map(|(.., r)| r.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(reasons(client_a, "power"), [UiReject::UnknownWindow]);
    assert_eq!(reasons(client_b, "power"), [UiReject::UnknownWindow]);
    assert_eq!(reasons(client_a, "explode"), [UiReject::UnknownAction]);
    let target = reasons(client_a, "target");
    assert_eq!(target.len(), 4, "{target:?}");
    // Shape checks (duplicate keys) run before the interface's parser, so order is not kept.
    let bad = target
        .iter()
        .filter(|r| matches!(r, UiReject::BadParams(_)))
        .count();
    assert_eq!(bad, 3, "{target:?}");
    assert!(target.contains(&UiReject::Malformed));
    // Rejections carry the action clipped to the cap, not the 1000 bytes the client sent.
    assert_eq!(
        reasons(client_a, &long[..ss13_tgui::server::MAX_ACTION_LEN]),
        [UiReject::Malformed]
    );
    let after = machine(apps[0]);
    assert_eq!(
        (after.on, after.target_temperature),
        (before.on, before.target_temperature)
    );

    // Two tiles away: the window degrades to update-only and acts are refused.
    move_mob(apps[0], client_a, TilePos::new(5, 7, 1));
    run_until(apps, "status update", |apps| {
        client_window(apps[1]).is_some_and(|(.., s)| s == UiStatus::Update)
    });
    // A's previous acts used up this second's budget.
    run_for(apps, Duration::from_millis(1100));
    send(apps[1], act(window, "power", &[]));
    run_until(apps, "not interactive", |apps| rejected(apps[0]).len() >= 9);
    assert_eq!(
        rejected(apps[0]).last().unwrap().2,
        UiReject::NotInteractive(UiStatus::Update)
    );
    assert!(machine(apps[0]).on);

    // Back in reach, after the one-second window: of a burst of 15, exactly 10 get through.
    move_mob(apps[0], client_a, TilePos::new(5, 6, 1));
    run_until(apps, "interactive again", |apps| {
        client_window(apps[1]).is_some_and(|(.., s)| s == UiStatus::Interactive)
    });
    run_for(apps, Duration::from_millis(1100));
    let accepted = apps[0].world().resource::<ServerLog>().acted.len();
    let refused = rejected(apps[0]).len();
    for _ in 0..15 {
        send(apps[1], act(window, "power", &[]));
    }
    run_until(apps, "spam", |apps| {
        let log = apps[0].world().resource::<ServerLog>();
        log.acted.len() + log.rejected.len() >= accepted + refused + 15
    });
    let log = apps[0].world().resource::<ServerLog>();
    assert_eq!(log.acted.len() - accepted, 10);
    assert_eq!(log.rejected.len() - refused, 5);
    assert!(
        log.rejected[refused..]
            .iter()
            .all(|(c, _, r)| *c == client_a && *r == UiReject::RateLimited)
    );
    // Ten toggles: power is where it was.
    assert!(machine(apps[0]).on);

    // Out of range: the server closes the window and the client despawns it.
    move_mob(apps[0], client_a, TilePos::new(5, 20, 1));
    run_until(apps, "closed", |apps| client_window(apps[1]).is_none());
    assert_eq!(server_windows(apps[0]), 0);

    // Reopened in reach, then closed by the player: the server forgets the window.
    move_mob(apps[0], client_a, TilePos::new(5, 6, 1));
    click(apps[1]);
    run_until(apps, "reopened", |apps| client_window(apps[1]).is_some());
    let (window, root, _) = client_window(apps[1]).unwrap();
    apps[1].world_mut().trigger(WindowClose { entity: root });
    apps[1].world_mut().entity_mut(root).despawn();
    run_until(apps, "server forgets", |apps| server_windows(apps[0]) == 0);
    // A late act on the closed window is refused (after the spam's rate window).
    run_for(apps, Duration::from_millis(1100));
    let count = rejected(apps[0]).len();
    send(apps[1], act(window, "power", &[]));
    run_until(apps, "late act", |apps| rejected(apps[0]).len() > count);
    assert_eq!(rejected(apps[0]).last().unwrap().2, UiReject::UnknownWindow);
}
