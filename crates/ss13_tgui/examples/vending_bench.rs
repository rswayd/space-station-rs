//! CPU cost of the Vending window, headless (no GPU): open it with 60 products, then time 200
//! data updates (one purchase each: a product's stock and the balance change) and idle frames.
//! It also counts the entities the updates spawn and the components changed per idle frame.
//! `cargo run --release -p ss13_tgui --features client --example vending_bench`.

use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::settings::{RenderCreation, WgpuSettings};
use bevy::time::TimeUpdateStrategy;
use bevy::winit::WinitPlugin;
use ss13_net::client::{NetClientState, SendIntent};
use ss13_net::ui::client::{SendUi, UiMessageReceived};
use ss13_shared::ui::{UiServerMessage, UiStatus};
use ss13_tgui::client::TguiClientPlugin;
use ss13_tgui::encode;
use ss13_tgui::interfaces::vending::{
    ProductRecord, StockItem, UserData, VendingData, VendingStatic,
};
use ss13_ui_kit::UiKitPlugin;

const PRODUCTS: u32 = 60;
const CATEGORIES: [(&str, &str); 6] = [
    ("Drinks", "mug-hot"),
    ("Snacks", "cookie"),
    ("Tools", "wrench"),
    ("Clothing", "shirt"),
    ("Medical", "kit-medical"),
    ("Misc", "box"),
];

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

fn static_data() -> VendingStatic {
    let product_records = (0..PRODUCTS)
        .map(|i| ProductRecord {
            path: format!("product-{i}"),
            name: format!("Product number {i}"),
            price: 10 + i,
            reference: i,
            category: CATEGORIES[i as usize % CATEGORIES.len()].0.to_owned(),
            colorable: false,
            premium: false,
            icon: None,
        })
        .collect();
    VendingStatic {
        ad: "Have a nice day!".into(),
        displayed_currency_name: " ¢".into(),
        product_records,
        categories: CATEGORIES
            .iter()
            .map(|(n, i)| ((*n).to_owned(), (*i).to_owned()))
            .collect(),
        ..default()
    }
}

fn data(step: u32) -> VendingData {
    VendingData {
        user: Some(UserData {
            name: "Tester".into(),
            cash: Some(1000 - step),
            job: "Assistant".into(),
            department: "Service".into(),
            is_captain_card: false,
        }),
        // Step `n` bought one of product `n % PRODUCTS` (each starts with 10).
        stock: (0..PRODUCTS)
            .map(|i| StockItem {
                amount: 10 - (step / PRODUCTS + u32::from(i < step % PRODUCTS)).min(10),
                free: false,
            })
            .collect(),
        extended_inventory: false,
    }
}

/// UI nodes spawned so far (a full re-render spawns the whole window content again).
#[derive(Resource, Default)]
struct Spawned(usize);

fn count_spawned(added: Query<(), Added<Node>>, mut spawned: ResMut<Spawned>) {
    spawned.0 += added.iter().count();
}

/// Entities whose layout or text inputs changed this frame (each makes bevy_ui redo work).
#[derive(Resource, Default)]
struct Changes(Vec<[usize; 4]>);

fn count_changes(
    nodes: Query<(), Changed<Node>>,
    fonts: Query<(), Changed<TextFont>>,
    colors: Query<(), Changed<TextColor>>,
    backgrounds: Query<(), Changed<BackgroundColor>>,
    mut changes: ResMut<Changes>,
) {
    changes.0.push([
        nodes.iter().count(),
        fonts.iter().count(),
        colors.iter().count(),
        backgrounds.iter().count(),
    ]);
}

fn send(app: &mut App, message: UiServerMessage) {
    app.world_mut().write_message(UiMessageReceived(message));
}

fn timed(app: &mut App, frames: usize, mut each: impl FnMut(&mut App, usize)) -> Vec<f64> {
    (0..frames)
        .map(|n| {
            each(app, n);
            let start = Instant::now();
            app.update();
            start.elapsed().as_secs_f64() * 1000.0
        })
        .collect()
}

fn report(what: &str, mut ms: Vec<f64>) {
    ms.sort_by(f64::total_cmp);
    let mean = ms.iter().sum::<f64>() / ms.len() as f64;
    let p = |q: f64| ms[((ms.len() - 1) as f64 * q) as usize];
    println!(
        "{what}: mean {mean:.3} ms, min {:.3}, p10 {:.3}, p50 {:.3}, p95 {:.3}, max {:.3} ({} frames)",
        p(0.0),
        p(0.1),
        p(0.5),
        p(0.95),
        p(1.0),
        ms.len()
    );
}

fn main() {
    let mut app = headless();
    let open = Instant::now();
    send(
        &mut app,
        UiServerMessage::Open {
            window: 1,
            interface: "Vending".into(),
            title: "Vending".into(),
            status: UiStatus::Interactive,
            static_data: encode(&static_data()),
            data: encode(&data(0)),
        },
    );
    for _ in 0..10 {
        app.update();
    }
    println!(
        "open + 10 frames: {:.1} ms",
        open.elapsed().as_secs_f64() * 1000.0
    );
    let entities = app.world().entities().len();
    println!("entities after open: {entities}");
    // An update every 4th frame (the swap waits for a settled layout), 200 updates.
    app.init_resource::<Spawned>()
        .add_systems(Last, count_spawned);
    let updates = timed(&mut app, 800, |app, n| {
        if n % 4 == 0 {
            send(
                app,
                UiServerMessage::Update {
                    window: 1,
                    status: UiStatus::Interactive,
                    data: Some(encode(&data(n as u32 / 4 + 1))),
                },
            );
        }
    });
    report("frames while updating", updates);
    println!(
        "UI nodes spawned per update: {:.0}",
        app.world().resource::<Spawned>().0 as f64 / 200.0
    );
    app.init_resource::<Changes>()
        .add_systems(Last, count_changes);
    let idle = timed(&mut app, 1000, |_, _| {});
    report("idle frames", idle);
    let changes = &app.world().resource::<Changes>().0;
    let avg = |i: usize| changes.iter().map(|c| c[i]).sum::<usize>() as f64 / changes.len() as f64;
    println!(
        "changed per idle frame: Node {:.0}, TextFont {:.0}, TextColor {:.0}, BackgroundColor {:.0}",
        avg(0),
        avg(1),
        avg(2),
        avg(3)
    );
}
