//! Incremental updates on a tiny station, without a GPU: the ECS side of the plugin and the
//! baked texels.

use std::time::Instant;

use ss13_shared::Dir;

use super::*;

fn app() -> App {
    let mut app = App::new();
    app.insert_resource(LightingSettings { enabled: true })
        .init_resource::<LightingStats>()
        .init_resource::<LightWorld>()
        .init_resource::<ActiveZ>()
        .add_message::<LightUpdate>()
        .add_systems(Update, (track_optics, update_sources).chain());
    app
}

fn lamp(range: f32) -> Optics {
    Optics {
        light: Some(LightSource {
            range,
            power: 1.0,
            color: [255, 255, 255],
            height: 1.0,
            angle: 360,
            dir: Dir::North,
            offset: [0, 0],
        }),
        ..default()
    }
}

fn wall() -> Optics {
    Optics {
        opaque: true,
        ..default()
    }
}

const REGION: Region = Region {
    min: IVec2::new(1, 1),
    size: UVec2::new(12, 5),
    z: 1,
};

/// Red channel of tg corner `(cx, cy)` in the baked region.
fn corner(app: &App, cx: i32, cy: i32) -> u8 {
    let (corners, _, _) = bake(app.world().resource::<LightWorld>(), REGION);
    let i = (cx - (REGION.min.x - 1)) as usize;
    let j = (cy - (REGION.min.y - 1)) as usize;
    corners[(j * (REGION.size.x as usize + 1) + i) * 4]
}

/// Two rooms on row y = 3 split by a door at x = 6, walls on rows 2 and 4.
fn station(app: &mut App) -> Entity {
    for x in 1..=12 {
        app.world_mut().spawn((TilePos::new(x, 2, 1), wall()));
        app.world_mut().spawn((TilePos::new(x, 4, 1), wall()));
    }
    app.world_mut().spawn((TilePos::new(3, 3, 1), lamp(6.0)));
    app.world_mut().spawn((TilePos::new(6, 3, 1), wall())).id()
}

#[test]
fn door_opening_and_closing_relights_the_next_room() {
    let mut app = app();
    let door = station(&mut app);
    app.update();
    let behind = (8, 3);
    assert!(corner(&app, 3, 3) > 0, "the lamp's own tile is lit");
    assert_eq!(corner(&app, behind.0, behind.1), 0, "closed door blocks");

    let updates = app.world().resource::<LightingStats>().source_updates;
    app.world_mut().get_mut::<Optics>(door).unwrap().opaque = false;
    app.update();
    assert!(
        corner(&app, behind.0, behind.1) > 0,
        "open door lets light through"
    );
    assert_eq!(
        app.world().resource::<LightingStats>().source_updates,
        updates + 1,
        "only the lamp in reach was recomputed"
    );

    app.world_mut().despawn(door);
    app.world_mut().spawn((TilePos::new(6, 3, 1), wall()));
    app.update();
    assert_eq!(
        corner(&app, behind.0, behind.1),
        0,
        "a new wall blocks again"
    );
}

#[test]
fn far_opacity_changes_do_not_recompute() {
    let mut app = app();
    station(&mut app);
    app.update();
    let updates = app.world().resource::<LightingStats>().source_updates;
    app.world_mut().spawn((TilePos::new(30, 3, 1), wall()));
    app.update();
    assert_eq!(
        app.world().resource::<LightingStats>().source_updates,
        updates
    );
}

#[test]
fn removed_light_goes_dark_and_ambient_tiles_bake() {
    let mut app = app();
    let lamp = app
        .world_mut()
        .spawn((TilePos::new(3, 3, 1), lamp(4.0)))
        .id();
    app.world_mut().spawn((
        TilePos::new(10, 3, 1),
        Optics {
            tile: Some(TileLighting::Area {
                dynamic: false,
                ambient: [255, 0, 0, 128],
            }),
            ..default()
        },
    ));
    app.update();
    assert!(corner(&app, 3, 3) > 0);
    app.world_mut().entity_mut(lamp).remove::<Optics>();
    app.update();
    assert_eq!(corner(&app, 3, 3), 0);

    let (_, tiles, _) = bake(app.world().resource::<LightWorld>(), REGION);
    let at = |x: i32, y: i32| {
        let i = ((y - REGION.min.y) * REGION.size.x as i32 + (x - REGION.min.x)) as usize * 4;
        [tiles[i], tiles[i + 1], tiles[i + 2], tiles[i + 3]]
    };
    assert_eq!(
        at(10, 3),
        [128, 0, 0, 0],
        "ambient premultiplied, not lit by sources"
    );
    assert_eq!(
        at(9, 3),
        [0, 0, 0, 255],
        "default: lit by sources, no ambient"
    );
}

fn bits(tiles: &[(i32, i32)]) -> [u8; 32] {
    let mut bits = [0; 32];
    for &(x, y) in tiles {
        let (_, i) = light_block(TilePos::new(x, y, 1));
        bits[i / 8] |= 1 << (i % 8);
    }
    bits
}

/// The same station as `station`, from light block updates as the server streams them.
#[test]
fn block_updates_light_occlude_and_forget() {
    let mut app = app();
    let origin = TilePos::new(1, 1, 1);
    let mut walls: Vec<(i32, i32)> = (1..=12).flat_map(|x| [(x, 2), (x, 4)]).collect();
    walls.push((6, 3));
    let lamp = lamp(6.0).light.unwrap();
    app.world_mut().write_message(LightUpdate::Opacity {
        origin,
        bits: bits(&walls),
    });
    app.world_mut().write_message(LightUpdate::Lights {
        pos: TilePos::new(3, 3, 1),
        lights: vec![lamp],
    });
    app.update();
    assert!(corner(&app, 3, 3) > 0, "the lamp's own tile is lit");
    assert_eq!(corner(&app, 8, 3), 0, "the closed door blocks");
    assert!(
        app.world()
            .resource::<LightWorld>()
            .is_opaque(TilePos::new(6, 3, 1))
    );

    // The door opens: one bit flips, only the lamp in reach is recomputed.
    let updates = app.world().resource::<LightingStats>().source_updates;
    walls.pop();
    app.world_mut().write_message(LightUpdate::Opacity {
        origin,
        bits: bits(&walls),
    });
    app.update();
    assert!(corner(&app, 8, 3) > 0, "the open door lets light through");
    assert_eq!(
        app.world().resource::<LightingStats>().source_updates,
        updates + 1
    );

    // An outage: the lamp's light is replaced by a dimmer one, then the tile has none.
    let mut dim = lamp;
    dim.range = 2.0;
    app.world_mut().write_message(LightUpdate::Lights {
        pos: TilePos::new(3, 3, 1),
        lights: vec![dim],
    });
    app.update();
    assert_eq!(corner(&app, 8, 3), 0, "a range-2 light does not reach");
    assert_eq!(app.world().resource::<LightWorld>().sources.len(), 1);

    // The block leaves the interest: lights and opacity are gone.
    app.world_mut()
        .write_message(LightUpdate::Forget { origin });
    app.update();
    let world = app.world().resource::<LightWorld>();
    assert!(world.sources.is_empty() && world.tile_lights.is_empty());
    assert!(!world.is_opaque(TilePos::new(6, 2, 1)));
    assert_eq!(corner(&app, 3, 3), 0);
}

/// Not a pass/fail check: the cost of a full rebuild with ~100 sources on screen, the worst
/// case (everything changed at once). `cargo test -p ss13_render lighting_cost -- --nocapture`.
#[test]
fn lighting_cost() {
    let mut app = app();
    // A 40x30 station: walls every 5 tiles with doorways, 100 lamps of range 4-8.
    for x in 1..=40 {
        for y in 1..=30 {
            if (x % 5 == 0 && y % 7 != 3) || (y % 6 == 0 && x % 9 != 4) {
                app.world_mut().spawn((TilePos::new(x, y, 1), wall()));
            }
        }
    }
    for i in 0..100 {
        let (x, y) = (1 + (i * 7) % 40, 1 + (i * 11) % 30);
        app.world_mut()
            .spawn((TilePos::new(x, y, 1), lamp(4.0 + (i % 5) as f32)));
    }
    let start = Instant::now();
    app.update();
    let sources = start.elapsed();
    let region = Region {
        min: IVec2::new(1, 1),
        size: UVec2::new(40, 30),
        z: 1,
    };
    let start = Instant::now();
    let (_, _, in_region) = bake(app.world().resource::<LightWorld>(), region);
    let baked = start.elapsed();
    assert_eq!(in_region, 100);
    println!("100 sources from scratch: {sources:?}; bake 40x30: {baked:?}");
}
