//! Contact sheet of player humans: one row per look, columns facing south, north, east, west,
//! drawn by `RenderPlugin` from real importer output and saved with a Bevy screenshot.
//!
//! cargo run -p ss13_game --all-features --example human_sheet -- --assets DIR --out sheet.png

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::window::WindowResolution;
use ss13_game::human::{HumanAppearances, HumanLook};
use ss13_render::{CameraTarget, RenderPlugin, WorldCamera};
use ss13_shared::{Appearance, Dir, Facing, Layer, TILE_SIZE, TilePos};

const ZOOM: u32 = 4;
const COLUMNS: [Dir; 4] = [Dir::South, Dir::North, Dir::East, Dir::West];

#[derive(Resource)]
struct Out(String);

fn main() -> AppExit {
    let mut assets = None;
    let mut out = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--assets" => assets = args.next(),
            "--out" => out = args.next(),
            other => panic!("unknown argument {other}"),
        }
    }
    let assets = assets.expect("--assets DIR");
    let out = out.expect("--out FILE.png");

    let manifest = ss13_mapload::read_manifest(assets.as_ref()).expect("manifest");
    let index = manifest.index();
    let floor = index[&("icons/turf/floors.dmi", "floor")];
    let mob = Appearance {
        icon_state: floor,
        layer: Layer(4.0),
        plane: 0,
        color: None,
        pixel_offset: [0, 0],
        rotation: 0,
        mouse_opacity: Default::default(),
        glide: 0,
        overlays: vec![],
    };
    let mut humans = HumanAppearances::new(&manifest, mob);
    // Three looks: dark skin and bald, two random assistants.
    let mut bald = HumanLook::random_assistant(7);
    bald.skin_tone = "african2".into();
    bald.hair_style = None;
    let looks = [
        HumanLook::random_assistant(1),
        HumanLook::random_assistant(2),
        bald,
    ];
    let mut sheet = Vec::new();
    for look in &looks {
        println!("{look:?}");
        sheet.push(humans.get(look).unwrap_or_else(|e| panic!("{e}")).clone());
    }

    let (w, h) = (COLUMNS.len() as u32, looks.len() as u32);
    let px = TILE_SIZE * ZOOM;
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "human_sheet".into(),
                    resolution: WindowResolution::new(w * px, h * px)
                        .with_scale_factor_override(1.0),
                    ..default()
                }),
                ..default()
            }),
            RenderPlugin {
                asset_root: assets.into(),
            },
        ))
        .insert_resource(Out(out))
        .add_systems(Startup, move |mut commands: Commands| {
            for (row, appearance) in sheet.iter().enumerate() {
                for (col, dir) in COLUMNS.into_iter().enumerate() {
                    let pos = TilePos::new(col as i32, (h as usize - 1 - row) as i32, 1);
                    commands.spawn((pos, floor_appearance(floor)));
                    commands.spawn((pos, Facing(dir), appearance.clone()));
                }
            }
            // An undrawn target whose transform puts the sheet's centre in the middle.
            let tile = TILE_SIZE as f32;
            commands.spawn((
                TilePos::new(0, 0, 1),
                CameraTarget,
                Transform::from_xyz(
                    w as f32 * tile / 2.0 - tile / 2.0,
                    h as f32 * tile / 2.0 - tile / 2.0,
                    0.0,
                ),
            ));
        })
        .add_systems(Update, (zoom, finish))
        .run()
}

fn floor_appearance(floor: ss13_shared::IconStateId) -> Appearance {
    Appearance {
        icon_state: floor,
        layer: Layer(2.0),
        plane: 0,
        color: None,
        pixel_offset: [0, 0],
        rotation: 0,
        mouse_opacity: Default::default(),
        glide: 0,
        overlays: vec![],
    }
}

fn zoom(mut cameras: Query<&mut WorldCamera>) {
    for mut c in &mut cameras {
        c.zoom = ZOOM;
    }
}

fn finish(
    mut commands: Commands,
    out: Res<Out>,
    frame: Res<bevy::diagnostic::FrameCount>,
    mut exit: MessageWriter<AppExit>,
) {
    if frame.0 == 30 {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(out.0.clone()));
    }
    if frame.0 >= 60 {
        exit.write(AppExit::Success);
    }
}
