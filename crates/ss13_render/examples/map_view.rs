//! Generated 64x64 test map drawn through `RenderPlugin` from a synthetic atlas set.
//!
//! cargo run -p ss13_render --example map_view [-- --screenshot out.png] [--assets DIR]
//!     [--bench N [--area N] [--still]] [--frames N]
//!
//! `--assets DIR` uses real importer output (`ss13-import dmi --out DIR`).
//!
//! Keys: `=`/`-` zoom. `--bench N` adds N sprites stacked on the visible tiles, disables vsync
//! and prints frame-time statistics.

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::window::{PresentMode, WindowResolution};
use ss13_assets::manifest::AssetManifest;
use ss13_render::synthetic;
use ss13_render::{CameraTarget, RenderPlugin, WorldCamera};
use ss13_shared::{Appearance, Dir, Facing, IconStateId, Layer, Overlay, TilePos};

const MAP: i32 = 64;
const Z: u16 = 1;

#[derive(Resource, Default, Clone)]
struct Args {
    screenshot: Option<String>,
    /// Real importer output (`ss13-import dmi --out DIR`) instead of the synthetic set.
    assets: Option<String>,
    bench: usize,
    /// Bench sprites spread over an `area x area` square instead of the visible tiles.
    area: Option<i32>,
    /// Bench without animated states.
    still: bool,
    frames: Option<u32>,
}

fn parse_args() -> Args {
    let mut args = Args::default();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut value = || it.next().expect("flag needs a value");
        match a.as_str() {
            "--screenshot" => args.screenshot = Some(value()),
            "--assets" => args.assets = Some(value()),
            "--bench" => args.bench = value().parse().expect("--bench N"),
            "--frames" => args.frames = Some(value().parse().expect("--frames N")),
            "--area" => args.area = Some(value().parse().expect("--area N")),
            "--still" => args.still = true,
            other => panic!("unknown argument {other}"),
        }
    }
    args
}

fn main() -> AppExit {
    let args = parse_args();
    let (root, palette) = match &args.assets {
        Some(dir) => (dir.into(), Palette::real(dir)),
        None => {
            let root = std::env::temp_dir().join("ss13_map_view_assets");
            synthetic::write(&root).expect("write synthetic atlas");
            (root, Palette::synthetic())
        }
    };

    let bench = args.bench > 0;
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "ss13_render map_view".into(),
                    resolution: WindowResolution::new(1280, 720).with_scale_factor_override(1.0),
                    present_mode: if bench {
                        PresentMode::AutoNoVsync
                    } else {
                        PresentMode::AutoVsync
                    },
                    ..default()
                }),
                ..default()
            }),
            RenderPlugin { asset_root: root },
        ))
        .insert_resource(args)
        .insert_resource(palette)
        .add_systems(Startup, (spawn_map, spawn_bench))
        .add_systems(Update, (walk, zoom_keys, frame_stats, finish))
        .run()
}

/// Which icon states the test map uses.
#[derive(Resource)]
struct Palette {
    floor: IconStateId,
    wall: IconStateId,
    /// 4 dirs.
    mob: IconStateId,
    /// Animated, looping.
    blink: IconStateId,
    /// Animated, finite loop or rewind.
    pulse: IconStateId,
    /// 8 dirs where available.
    pipe: IconStateId,
    big: IconStateId,
    walker: IconStateId,
    overlays: Vec<Overlay>,
}

/// Follows the parent's facing and color.
fn overlay(state: IconStateId) -> Overlay {
    Overlay::new(state)
}

impl Palette {
    fn synthetic() -> Self {
        use synthetic::*;
        Self {
            floor: FLOOR,
            wall: WALL,
            mob: MOB,
            blink: BLINK,
            pulse: PULSE,
            pipe: PIPE,
            big: BIG,
            walker: WALKER,
            // The badge keeps its own dir and color and sits 4 px right, 2 px down.
            overlays: vec![
                overlay(HAT),
                Overlay {
                    dir: Some(Dir::South),
                    color: Some([255, 255, 0, 255]),
                    pixel_offset: [4, -2],
                    ..overlay(BADGE)
                },
            ],
        }
    }

    fn real(dir: &str) -> Self {
        let bytes = std::fs::read(format!("{dir}/manifest.postcard")).expect("read manifest");
        let manifest: AssetManifest = postcard::from_bytes(&bytes).expect("decode manifest");
        // First non-movement match: `AssetManifest::index` keeps the last duplicate.
        let find = |dmi: &str, state: &str| {
            let i = manifest
                .states
                .iter()
                .position(|s| s.dmi == dmi && s.state == state && !s.movement)
                .unwrap_or_else(|| panic!("{dmi} / {state} not in manifest"));
            IconStateId(i as u32)
        };
        let computer = "icons/obj/machines/computer.dmi";
        Self {
            floor: find("icons/turf/floors.dmi", "floor"),
            wall: find("icons/turf/walls/wall.dmi", "wall-0"),
            mob: find("icons/mob/simple/pets.dmi", "corgi"),
            blink: find(computer, "generic"),
            pulse: find("icons/obj/doors/airlocks/station/public.dmi", "opening"),
            pipe: find(computer, "computer"),
            big: find("icons/obj/doors/airlocks/station/public.dmi", "closed"),
            walker: find("icons/mob/simple/pets.dmi", "corgi"),
            overlays: vec![overlay(find(computer, "generic"))],
        }
    }
}

fn appearance(state: IconStateId, dir: Dir, layer: f32) -> (Facing, Appearance) {
    let a = Appearance {
        icon_state: state,
        layer: Layer(layer),
        plane: 0,
        color: None,
        pixel_offset: [0, 0],
        rotation: 0,
        mouse_opacity: Default::default(),
        overlays: vec![],
    };
    (Facing(dir), a)
}

fn spawn_map(mut commands: Commands, p: Res<Palette>) {
    for x in 0..MAP {
        for y in 0..MAP {
            let border = x == 0 || y == 0 || x == MAP - 1 || y == MAP - 1;
            let room = (x == 20 || y == 20) && (x <= 20 && y <= 20) && x != 10 && y != 10;
            let state = if border || room { p.wall } else { p.floor };
            commands.spawn((TilePos::new(x, y, Z), appearance(state, Dir::South, 2.0)));
        }
    }
    // 8-dir pipes, one per direction, under objects.
    for (i, d) in Dir::DMI_ORDER.into_iter().enumerate() {
        commands.spawn((
            TilePos::new(24 + i as i32, 30, Z),
            appearance(p.pipe, d, 2.4),
        ));
    }
    // 4-dir mobs in all 8 dirs: diagonals show EAST/WEST.
    for (i, d) in Dir::DMI_ORDER.into_iter().enumerate() {
        commands.spawn((
            TilePos::new(24 + i as i32, 28, Z),
            appearance(p.mob, d, 4.0),
        ));
    }
    for i in 0..6 {
        commands.spawn((
            TilePos::new(24 + i, 26, Z),
            appearance(p.blink, Dir::South, 3.0),
        ));
        commands.spawn((
            TilePos::new(24 + i, 25, Z),
            appearance(p.pulse, Dir::South, 3.0),
        ));
    }
    commands.spawn((TilePos::new(34, 26, Z), appearance(p.big, Dir::South, 3.0)));
    // Tinted walkers with overlays.
    for (i, color) in [
        [255, 255, 255, 255],
        [255, 120, 120, 255],
        [120, 255, 120, 200],
    ]
    .into_iter()
    .enumerate()
    {
        let (facing, mut a) = appearance(p.walker, Dir::DMI_ORDER[i], 4.0);
        a.color = Some(color);
        a.overlays = p.overlays.clone();
        // BYOND pixel_y: the last walker floats 8 px up.
        a.pixel_offset = [0, 8 * (i as i16 / 2)];
        commands.spawn((TilePos::new(24 + i as i32, 23, Z), facing, a));
    }
    // Another z-level: must not be drawn.
    commands.spawn((
        TilePos::new(30, 30, Z + 1),
        appearance(p.big, Dir::South, 10.0),
    ));

    let (facing, mut player) = appearance(p.walker, Dir::East, 4.0);
    player.overlays = p.overlays[..1].to_vec();
    commands.spawn((TilePos::new(28, 32, Z), facing, player, CameraTarget));
}

/// `--bench N`: N extra sprites stacked over the tiles visible at zoom 1 (or `--area`),
/// a quarter animated unless `--still`.
fn spawn_bench(mut commands: Commands, args: Res<Args>, p: Res<Palette>) {
    if args.bench == 0 {
        return;
    }
    let states = if args.still {
        [
            p.floor, p.mob, p.wall, p.pipe, p.big, p.mob, p.floor, p.wall,
        ]
    } else {
        [
            p.floor, p.mob, p.blink, p.pipe, p.walker, p.pulse, p.floor, p.wall,
        ]
    };
    // 40x23 tiles are visible in 1280x720 at zoom 1.
    let (w, h) = args.area.map_or((40, 23), |a| (a, a));
    let (x0, y0) = (28 - w / 2, 32 - h / 2);
    let batch: Vec<_> = (0..args.bench)
        .map(|i| {
            let tile = (i % (w * h) as usize) as i32;
            let state = states[i % states.len()];
            let dir = Dir::DMI_ORDER[i % 8];
            let (facing, a) = appearance(state, dir, 5.0 + (i % 7) as f32 * 0.1);
            (TilePos::new(x0 + tile % w, y0 + tile / w, Z), facing, a)
        })
        .collect();
    commands.spawn_batch(batch);
}

/// Walks the camera target around a rectangle, one tile per 0.3 s.
/// `TilePos` is immutable, so a step inserts a new one.
fn walk(
    mut commands: Commands,
    time: Res<Time>,
    mut timer: Local<f32>,
    mut step: Local<usize>,
    mut q: Query<(Entity, &TilePos, &mut Facing), With<CameraTarget>>,
) {
    *timer += time.delta_secs();
    if *timer < 0.3 {
        return;
    }
    *timer = 0.0;
    *step += 1;
    let dir = [Dir::East, Dir::North, Dir::West, Dir::South][(*step / 6) % 4];
    for (entity, pos, mut facing) in &mut q {
        commands.entity(entity).insert(pos.step(dir));
        facing.set_if_neq(Facing(dir));
    }
}

fn zoom_keys(
    args: Res<Args>,
    keys: Res<ButtonInput<KeyCode>>,
    mut cameras: Query<&mut WorldCamera>,
) {
    for mut c in &mut cameras {
        if args.bench > 0 {
            c.zoom = 1;
            continue;
        }
        if keys.just_pressed(KeyCode::Equal) {
            c.zoom += 1;
        }
        if keys.just_pressed(KeyCode::Minus) && c.zoom > 1 {
            c.zoom -= 1;
        }
    }
}

/// Bench: frame times of frames 120..1320 (after pipeline warm-up).
fn frame_stats(
    args: Res<Args>,
    time: Res<Time<Real>>,
    frame: Res<bevy::diagnostic::FrameCount>,
    sprites: Query<(), With<Sprite>>,
    mut samples: Local<Vec<f32>>,
    mut exit: MessageWriter<AppExit>,
) {
    if args.bench == 0 || frame.0 < 120 {
        return;
    }
    samples.push(time.delta_secs() * 1000.0);
    if samples.len() < 1200 {
        return;
    }
    samples.sort_by(f32::total_cmp);
    let mean = samples.iter().sum::<f32>() / samples.len() as f32;
    let pct = |p: f32| samples[((samples.len() - 1) as f32 * p) as usize];
    println!(
        "bench: {} sprites, {} frames: mean {mean:.2} ms, p50 {:.2} ms, p99 {:.2} ms, max {:.2} ms",
        sprites.iter().count(),
        samples.len(),
        pct(0.5),
        pct(0.99),
        pct(1.0)
    );
    exit.write(AppExit::Success);
}

fn finish(
    mut commands: Commands,
    args: Res<Args>,
    frame: Res<bevy::diagnostic::FrameCount>,
    mut exit: MessageWriter<AppExit>,
) {
    if let Some(path) = &args.screenshot
        && frame.0 == 90
    {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path.clone()));
    }
    let limit = args.frames.or(args.screenshot.as_ref().map(|_| 150));
    if limit.is_some_and(|n| frame.0 >= n) {
        exit.write(AppExit::Success);
    }
}
