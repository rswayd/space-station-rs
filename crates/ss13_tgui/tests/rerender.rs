//! tgui windows re-rendered by data updates in a headless Bevy app with real UI layout and text
//! shaping (no GPU, no OS window): no frame may show a layout that is still settling (label and
//! column widths, auto scrollbar tracks fix themselves a frame after the first layout), the window
//! keeps its geometry, and updates that change nothing respawn nothing.

use std::time::Duration;

use bevy::render::RenderPlugin;
use bevy::render::settings::{RenderCreation, WgpuSettings};
use bevy::time::TimeUpdateStrategy;
use bevy::ui::{ComputedNode, UiGlobalTransform};
use bevy::winit::WinitPlugin;
use ss13_net::client::{NetClientState, SendIntent};
use ss13_net::ui::client::{SendUi, UiMessageReceived};
use ss13_shared::ui::{UiServerMessage, UiStatus};
use ss13_tgui::client::{AppTguiClientExt, ClientUi, ClientWindows, Memo, TguiClientPlugin, memo};
use ss13_tgui::interfaces::thermo_machine::{T20C, ThermoMachineData};
use ss13_tgui::{UiInterface, encode};
use ss13_ui_kit::UiKitPlugin;
use ss13_ui_kit::prelude::*;

/// The NtOS window's shape: a clock in a header that changes every second, a scrollable
/// section (auto scrollbar track) with a table (synced column widths).
struct ClockUi;

impl UiInterface for ClockUi {
    const NAME: &'static str = "Clock";
    type Data = String;
    type StaticData = ();
}

impl ClientUi for ClockUi {
    const WIDTH: f32 = 400.0;
    const HEIGHT: f32 = 300.0;

    fn content(_: &(), clock: &String) -> impl Scene {
        let clock = clock.clone();
        let rows: Vec<_> = ["Messenger", "Notepad", "Crew Manifest"]
            .into_iter()
            .map(|name| {
                bsn! {
                    @TableRow
                    Children [
                        @TableCell { @content: name }
                        --
                        @TableCell { @collapsing: true }
                        Children [ @Button { @icon: "times" } ]
                    ]
                }
            })
            .collect();
        bsn! {
            @WindowContent
            Children [
                @Stack { @vertical: true, @fill: true }
                Children [
                    @StackItem
                    Children [ Node Children [ Text(clock) ] ]
                    --
                    @StackItem { @grow: 1.0 }
                    Node { min_height: px(0), flex_direction: FlexDirection::Column }
                    Children [
                        @Section { @fill: true, @scrollable: true }
                        Children [ @Table Children [ {rows} ] ]
                    ]
                ]
            ]
        }
    }
}

/// The Techweb's shape: a counter that changes every second over a grid of buttons that never
/// changes, memoized.
struct GridUi;

impl UiInterface for GridUi {
    const NAME: &'static str = "Grid";
    type Data = String;
    type StaticData = ();
}

impl ClientUi for GridUi {
    const WIDTH: f32 = 400.0;
    const HEIGHT: f32 = 300.0;

    fn content(_: &(), points: &String) -> impl Scene {
        let points = points.clone();
        let grid = memo(1, || {
            let buttons: Vec<_> = (0..60)
                .map(|i| bsn! { @Button { @content: {format!("b{i}")} } })
                .collect();
            bsn! {
                Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap }
                Children [ {buttons} ]
            }
        });
        bsn! {
            @WindowContent
            Children [
                Node Children [ Text(points) ]
                --
                {grid}
            ]
        }
    }
}

fn headless() -> App {
    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .disable::<bevy::log::LogPlugin>()
            .set(RenderPlugin {
                render_creation: RenderCreation::Automatic(Box::new(WgpuSettings {
                    backends: None,
                    ..default()
                })),
                ..default()
            }),
        UiKitPlugin,
        TguiClientPlugin,
    ))
    .register_ui_view::<ClockUi>()
    .register_ui_view::<GridUi>()
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        16,
    )))
    .insert_resource(NetClientState::Connected)
    .add_message::<UiMessageReceived>()
    .add_message::<SendUi>()
    .add_message::<SendIntent>();
    app.world_mut().spawn(Camera2d);
    for _ in 0..3 {
        app.update();
    }
    app
}

const WINDOW: u32 = 7;

/// One interface under test: its name, data for step `n` and the text that step shows.
struct Case {
    interface: &'static str,
    data: fn(u32) -> Vec<u8>,
    label: fn(u32) -> String,
}

const THERMO: Case = Case {
    interface: "ThermoMachine",
    data: |n| {
        encode(&ThermoMachineData {
            on: true,
            min: 73.15,
            max: 573.15,
            target: 200.0,
            initial: T20C,
            temperature: 250.5 + n as f32,
            pressure: 101.325,
        })
    },
    label: |n| format!("{:.2} K", 250.5 + n as f32),
};

const CLOCK: Case = Case {
    interface: "Clock",
    data: |n| encode(&(CLOCK.label)(n)),
    label: |n| format!("12:00:{n:02}"),
};

fn send(app: &mut App, message: UiServerMessage) {
    app.world_mut().write_message(UiMessageReceived(message));
}

fn open(app: &mut App, case: &Case) {
    send(
        app,
        UiServerMessage::Open {
            window: WINDOW,
            interface: case.interface.into(),
            title: case.interface.into(),
            status: UiStatus::Interactive,
            static_data: Vec::new(),
            data: (case.data)(0),
        },
    );
}

fn update(app: &mut App, status: UiStatus, data: Option<Vec<u8>>) {
    send(
        app,
        UiServerMessage::Update {
            window: WINDOW,
            status,
            data,
        },
    );
}

fn root(app: &App) -> Option<Entity> {
    app.world()
        .resource::<ClientWindows>()
        .0
        .get(&WINDOW)
        .copied()
}

/// Drawn entities of the window in tree order.
fn drawn(app: &App) -> Vec<Entity> {
    let world = app.world();
    let mut out = Vec::new();
    let mut stack: Vec<Entity> = root(app).into_iter().collect();
    while let Some(e) = stack.pop() {
        if world.get::<InheritedVisibility>(e).is_some_and(|v| v.get()) {
            out.push(e);
        }
        if let Some(children) = world.get::<Children>(e) {
            stack.extend(children.iter().rev());
        }
    }
    out
}

fn rect(app: &App, e: Entity) -> Option<[i32; 4]> {
    let node = app.world().get::<ComputedNode>(e)?;
    let t = app.world().get::<UiGlobalTransform>(e)?;
    let size = node.size * node.inverse_scale_factor;
    let min = t.translation * node.inverse_scale_factor - size / 2.0;
    Some([min.x, min.y, size.x, size.y].map(|v| v.round() as i32))
}

/// What the frame shows: every drawn node of the window in tree order, as a rounded rect.
fn frame(app: &App) -> Vec<[i32; 4]> {
    drawn(app)
        .into_iter()
        .filter_map(|e| rect(app, e))
        .collect()
}

fn shows(app: &App, needle: &str) -> bool {
    drawn(app).into_iter().any(|e| {
        app.world()
            .get::<Text>(e)
            .is_some_and(|t| t.0.contains(needle))
    })
}

fn frames(app: &mut App, n: usize) -> Vec<Vec<[i32; 4]>> {
    (0..n)
        .map(|_| {
            app.update();
            frame(app)
        })
        .collect()
}

/// Opens the case's window and lets it settle; every frame that showed the window had to show
/// the settled layout.
fn open_settled(app: &mut App, case: &Case) -> Vec<[i32; 4]> {
    open(app, case);
    let shown = frames(app, 20);
    let settled = shown.last().unwrap().clone();
    assert!(
        !settled.is_empty(),
        "{}: window never shown",
        case.interface
    );
    let bad: Vec<usize> = (0..shown.len())
        .filter(|&i| !shown[i].is_empty() && shown[i] != settled)
        .collect();
    assert!(
        bad.is_empty(),
        "{}: frames {bad:?} after open drawn before the layout settled",
        case.interface
    );
    settled
}

fn updates_never_show_unsettled_frames(case: &Case) {
    let mut app = headless();
    let settled = open_settled(&mut app, case);
    let window = rect(&app, root(&app).unwrap());
    for n in 1..=5 {
        update(&mut app, UiStatus::Interactive, Some((case.data)(n)));
        let label = (case.label)(n);
        let latency = (1..=10)
            .find(|_| {
                app.update();
                shows(&app, &label)
            })
            .expect("update not shown within 10 frames");
        // The spawn frame, then the first frame without a node change; not the settle cap.
        assert!(
            latency <= 3,
            "{}: update {n} took {latency} frames",
            case.interface
        );
        let mut after = vec![frame(&app)];
        after.extend(frames(&mut app, 10));
        for (i, f) in after.iter().enumerate() {
            let diff: Vec<String> = f
                .iter()
                .zip(&settled)
                .enumerate()
                .filter(|(_, (a, b))| a != b)
                .take(3)
                .map(|(k, (a, b))| format!("node {k}: {a:?}, settled {b:?}"))
                .collect();
            assert!(
                *f == settled,
                "{}: update {n}, frame {i} after showing: {} nodes vs {}; {}",
                case.interface,
                f.len(),
                settled.len(),
                diff.join("; ")
            );
        }
        assert_eq!(
            rect(&app, root(&app).unwrap()),
            window,
            "window moved or resized"
        );
    }
}

#[test]
fn labeled_list_updates_never_show_unsettled_frames() {
    updates_never_show_unsettled_frames(&THERMO);
}

#[test]
fn table_in_scrollable_section_updates_never_show_unsettled_frames() {
    updates_never_show_unsettled_frames(&CLOCK);
}

#[test]
fn updates_without_new_data_respawn_nothing() {
    let mut app = headless();
    open_settled(&mut app, &CLOCK);
    let before = drawn(&app);
    // A status change, then the shown data again.
    update(&mut app, UiStatus::Update, None);
    frames(&mut app, 3);
    let dimmed = drawn(&app);
    assert_eq!(
        dimmed.len(),
        before.len() + 1,
        "dimmer shown below interactive"
    );
    update(&mut app, UiStatus::Interactive, Some((CLOCK.data)(0)));
    frames(&mut app, 3);
    assert_eq!(drawn(&app), before, "nodes respawned");
}

fn open_id(app: &mut App, window: u32, case: &Case) {
    send(
        app,
        UiServerMessage::Open {
            window,
            interface: case.interface.into(),
            title: case.interface.into(),
            status: UiStatus::Interactive,
            static_data: Vec::new(),
            data: (case.data)(0),
        },
    );
}

fn corner_of(app: &App, window: u32) -> [i32; 2] {
    let root = app.world().resource::<ClientWindows>().0[&window];
    let r = rect(app, root).unwrap();
    [r[0], r[1]]
}

/// tgui placement: a new window opens centered on the screen, a second one that would cover it
/// exactly is offset, and an interface the player dragged reopens where it was left.
#[test]
fn windows_open_centered_cascade_and_reopen_where_dragged() {
    let mut app = headless();
    let screen = {
        let mut q = app
            .world_mut()
            .query_filtered::<&bevy::window::Window, With<bevy::window::PrimaryWindow>>();
        q.single(app.world()).unwrap().resolution.size()
    };
    open_id(&mut app, 1, &CLOCK);
    open_id(&mut app, 2, &CLOCK);
    frames(&mut app, 5);
    let center = ((screen - Vec2::new(ClockUi::WIDTH, ClockUi::HEIGHT)) / 2.0).round();
    let first = corner_of(&app, 1);
    assert_eq!(first, [center.x as i32, center.y as i32], "centered");
    assert_eq!(
        corner_of(&app, 2),
        [first[0] + 30, first[1] + 30],
        "cascaded"
    );

    // The player drags the first window, closes both, and the interface opens again.
    let root = app.world().resource::<ClientWindows>().0[&1];
    let mut node = app.world_mut().get_mut::<Node>(root).unwrap();
    node.left = px(10);
    node.top = px(20);
    frames(&mut app, 2);
    send(&mut app, UiServerMessage::Close { window: 1 });
    send(&mut app, UiServerMessage::Close { window: 2 });
    frames(&mut app, 2);
    open_id(&mut app, 3, &CLOCK);
    frames(&mut app, 5);
    assert_eq!(corner_of(&app, 3), [10, 20], "reopened where dragged");
}

const GRID: Case = Case {
    interface: "Grid",
    data: |n| encode(&(GRID.label)(n)),
    label: |n| format!("points {n}"),
};

fn memo_root(app: &mut App) -> Vec<Entity> {
    let mut q = app.world_mut().query_filtered::<Entity, With<Memo>>();
    q.iter(app.world()).collect()
}

/// A memoized subtree survives data updates as the same entities, and no frame shows it (or the
/// rest) before the layout settled.
#[test]
fn memoized_subtrees_are_reused_without_unsettled_frames() {
    let mut app = headless();
    let settled = open_settled(&mut app, &GRID);
    let grid = memo_root(&mut app);
    assert_eq!(grid.len(), 1);
    for n in 1..=3 {
        update(&mut app, UiStatus::Interactive, Some((GRID.data)(n)));
        let label = (GRID.label)(n);
        let mut shown = Vec::new();
        for _ in 0..10 {
            app.update();
            if !shown.is_empty() || shows(&app, &label) {
                shown.push(frame(&app));
            }
        }
        assert!(!shown.is_empty(), "update {n} never shown");
        assert!(shown.len() >= 6, "update {n} shown too late");
        for f in &shown {
            assert_eq!(
                f, &settled,
                "update {n}: a frame differs from the settled layout"
            );
        }
        assert_eq!(
            memo_root(&mut app),
            grid,
            "update {n}: the grid was respawned"
        );
    }
}

/// A purchase changes one product's stock: that row is drawn again with the new amount, every
/// other row stays the same entities (Vending rows are memoized by what they show).
#[test]
fn vending_purchase_respawns_only_the_bought_row() {
    use ss13_tgui::interfaces::vending::{
        ProductRecord, StockItem, UserData, VendingData, VendingRow, VendingStatic,
    };
    let products = 6;
    let data = |bought: u32| VendingData {
        user: Some(UserData {
            name: "Tester".into(),
            cash: Some(1000),
            job: "Assistant".into(),
            department: "Service".into(),
            is_captain_card: false,
        }),
        stock: (0..products)
            .map(|i| StockItem {
                amount: if i == 2 { 5 - bought } else { 5 },
                free: false,
            })
            .collect(),
        extended_inventory: false,
    };
    let st = VendingStatic {
        displayed_currency_name: " cr".into(),
        product_records: (0..products)
            .map(|i| ProductRecord {
                path: format!("product-{i}"),
                name: format!("product {i}"),
                price: 10,
                reference: i,
                category: "Misc".into(),
                colorable: false,
                premium: false,
                icon: None,
            })
            .collect(),
        ..default()
    };
    let mut app = headless();
    send(
        &mut app,
        UiServerMessage::Open {
            window: WINDOW,
            interface: "Vending".into(),
            title: "Vending".into(),
            status: UiStatus::Interactive,
            static_data: encode(&st),
            data: encode(&data(0)),
        },
    );
    frames(&mut app, 10);
    let rows = |app: &mut App| {
        let mut q = app
            .world_mut()
            .query_filtered::<(Entity, &VendingRow), With<Memo>>();
        let mut rows: Vec<_> = q
            .iter(app.world())
            .map(|(e, row)| (row.name.clone(), e))
            .collect();
        rows.sort();
        rows
    };
    let before = rows(&mut app);
    assert_eq!(before.len(), products as usize, "every row memoized");
    update(&mut app, UiStatus::Interactive, Some(encode(&data(1))));
    frames(&mut app, 10);
    assert!(
        shows(&app, "4 ост.") || shows(&app, "x4"),
        "new stock shown"
    );
    let after = rows(&mut app);
    for ((name, old), (_, new)) in before.iter().zip(&after) {
        assert_eq!(
            old == new,
            name != "product 2",
            "{name}: reused {}",
            old == new
        );
    }
}
