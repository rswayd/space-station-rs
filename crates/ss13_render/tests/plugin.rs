//! `RenderPlugin` in a headless app: sprites follow `Appearance`/`Facing`/`TilePos` changes.

use std::time::Duration;

use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy::time::TimeUpdateStrategy;
use ss13_render::atlas::frame_rect;
use ss13_render::synthetic::{BADGE, BIG, BLINK, FLOOR, HAT, MOB, WALKER};
use ss13_render::{Animated, IconAtlas, Overlays, RenderPlugin};
use ss13_shared::{Appearance, Dir, Facing, IconStateId, Layer, Overlay, TilePos};

fn app(name: &str) -> App {
    let root = std::env::temp_dir().join(format!("ss13_render_it_{name}_{}", std::process::id()));
    ss13_render::synthetic::write(&root).unwrap();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Image>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            100,
        )))
        .add_plugins(RenderPlugin { asset_root: root });
    app.update();
    app
}

fn look(state: IconStateId, layer: f32) -> Appearance {
    Appearance {
        icon_state: state,
        layer: Layer(layer),
        plane: 0,
        color: None,
        pixel_offset: [0, 0],
        rotation: 0,
        mouse_opacity: Default::default(),
        overlays: vec![],
    }
}

/// Follows the parent's facing and color, no offset.
fn overlay(state: IconStateId) -> Overlay {
    Overlay::new(state)
}

fn overlay_sprites(app: &App, e: Entity) -> Vec<Entity> {
    app.world().get::<Overlays>(e).unwrap().0.clone()
}

fn rect(app: &App, state: IconStateId, dir: Dir, frame: usize) -> Option<Rect> {
    let atlas = app.world().resource::<IconAtlas>();
    frame_rect(atlas.manifest.get(state).unwrap(), dir, frame)
}

#[test]
fn sprite_follows_appearance_position_and_z_level() {
    let mut app = app("follow");
    let mut a = look(MOB, 4.0);
    a.overlays = vec![overlay(HAT), overlay(BADGE)];
    a.color = Some([255, 0, 0, 255]);
    let e = app
        .world_mut()
        .spawn((TilePos::new(3, 4, 1), Facing(Dir::East), a))
        .id();
    app.update();

    let world = app.world();
    let sprite = world.get::<Sprite>(e).unwrap();
    assert_eq!(sprite.rect, rect(&app, MOB, Dir::East, 0));
    assert_eq!(sprite.color, Color::srgba_u8(255, 0, 0, 255));
    assert_eq!(
        world.get::<Transform>(e).unwrap().translation.xy(),
        Vec2::new(96.0, 128.0)
    );
    let overlays = world.get::<Overlays>(e).unwrap().0.clone();
    assert_eq!(overlays.len(), 2);
    let hat = world.get::<Sprite>(overlays[0]).unwrap();
    assert_eq!(hat.rect, rect(&app, HAT, Dir::East, 0));
    assert_eq!(world.get::<ChildOf>(overlays[1]).unwrap().parent(), e);

    // Diagonal on a 4-dir state, one overlay dropped.
    let mut a = world.get::<Appearance>(e).unwrap().clone();
    a.overlays.truncate(1);
    app.world_mut()
        .entity_mut(e)
        .insert((a, Facing(Dir::NorthWest)));
    app.update();
    let world = app.world();
    assert_eq!(
        world.get::<Sprite>(e).unwrap().rect,
        rect(&app, MOB, Dir::West, 0)
    );
    assert_eq!(world.get::<Overlays>(e).unwrap().0, vec![overlays[0]]);
    assert!(world.get_entity(overlays[1]).is_err());

    // No `Facing` draws south.
    app.world_mut().entity_mut(e).remove::<Facing>();
    app.update();
    assert_eq!(
        app.world().get::<Sprite>(e).unwrap().rect,
        rect(&app, MOB, Dir::South, 0)
    );

    app.world_mut().entity_mut(e).insert(TilePos::new(5, 5, 2));
    app.update();
    let world = app.world();
    assert_eq!(world.get::<Visibility>(e), Some(&Visibility::Hidden));
    assert_eq!(
        world.get::<Transform>(e).unwrap().translation.xy(),
        Vec2::new(160.0, 160.0)
    );

    app.world_mut().entity_mut(e).remove::<Appearance>();
    app.update();
    assert!(app.world().get::<Sprite>(e).is_none());
    assert!(app.world().get_entity(overlays[0]).is_err());
}

#[test]
fn draw_order_survives_new_layers() {
    let mut app = app("order");
    let floor = app
        .world_mut()
        .spawn((TilePos::new(0, 0, 1), look(FLOOR, 2.0)))
        .id();
    let mob = app
        .world_mut()
        .spawn((TilePos::new(0, 0, 1), look(MOB, 4.0)))
        .id();
    app.update();
    // A layer between the two arrives later and shifts ranks.
    let mid = app
        .world_mut()
        .spawn((TilePos::new(0, 0, 1), look(BLINK, 3.0)))
        .id();
    let mut on_top_plane = look(FLOOR, 1.0);
    on_top_plane.plane = 1;
    let top = app
        .world_mut()
        .spawn((TilePos::new(0, 0, 1), on_top_plane))
        .id();
    app.update();
    let z = |e| app.world().get::<Transform>(e).unwrap().translation.z;
    assert!(z(floor) < z(mid), "{} {}", z(floor), z(mid));
    assert!(z(mid) < z(mob));
    assert!(z(mob) < z(top));
}

#[test]
fn animation_advances_by_delays() {
    let mut app = app("anim");
    // BLINK: 4 frames of 3 ds each.
    let e = app
        .world_mut()
        .spawn((TilePos::new(0, 0, 1), look(BLINK, 3.0)))
        .id();
    app.update();
    // Sample mid-frame (t = 0.05, 0.15, ...) so float rounding at frame edges cannot flip results.
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        50,
    )));
    app.update();
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        100,
    )));
    let mut seen = vec![];
    for _ in 0..13 {
        let r = app.world().get::<Sprite>(e).unwrap().rect;
        let frame = (0..4)
            .find(|&f| rect(&app, BLINK, Dir::South, f) == r)
            .unwrap();
        seen.push(frame);
        app.update();
    }
    // 100 ms per update: each frame shows for 3 updates, then wraps.
    assert_eq!(seen, vec![0, 0, 0, 1, 1, 1, 2, 2, 2, 3, 3, 3, 0]);
}

#[test]
fn turning_updates_rects_and_keeps_the_animation_clock() {
    let mut app = app("turn");
    // WALKER: 4 dirs x 2 frames of 4 ds.
    let mut a = look(WALKER, 4.0);
    a.overlays = vec![
        overlay(HAT),
        Overlay {
            dir: Some(Dir::North),
            ..overlay(HAT)
        },
    ];
    let e = app
        .world_mut()
        .spawn((TilePos::new(0, 0, 1), Facing(Dir::South), a))
        .id();
    app.update();
    let start = app.world().get::<Animated>(e).unwrap().start;
    // 0.5 s in: frame 1 (0.4..0.8 s).
    for _ in 0..5 {
        app.update();
    }
    app.world_mut().entity_mut(e).insert(Facing(Dir::East));
    app.update();

    let world = app.world();
    let anim = world.get::<Animated>(e).unwrap();
    assert_eq!(anim.start, start);
    assert_eq!(anim.frame, 1);
    assert_eq!(
        world.get::<Sprite>(e).unwrap().rect,
        rect(&app, WALKER, Dir::East, 1)
    );
    let [follows, fixed] = overlay_sprites(&app, e)[..] else {
        panic!("two overlays");
    };
    assert_eq!(
        world.get::<Sprite>(follows).unwrap().rect,
        rect(&app, HAT, Dir::East, 0)
    );
    assert_eq!(
        world.get::<Sprite>(fixed).unwrap().rect,
        rect(&app, HAT, Dir::North, 0)
    );
}

#[test]
fn pixel_offsets_and_overlay_color() {
    let mut app = app("offset");
    let mut a = look(MOB, 4.0);
    a.pixel_offset = [3, -5];
    a.color = Some([10, 20, 30, 128]);
    a.overlays = vec![
        overlay(BADGE),
        Overlay {
            color: Some([255, 255, 255, 64]),
            pixel_offset: [1, 2],
            ..overlay(BADGE)
        },
    ];
    let e = app
        .world_mut()
        .spawn((TilePos::new(2, 1, 1), Facing(Dir::South), a))
        .id();
    let mut big = look(BIG, 3.0);
    big.pixel_offset = [-16, 8];
    let b = app.world_mut().spawn((TilePos::new(0, 0, 1), big)).id();
    app.update();

    let world = app.world();
    // The transform stays on the tile; the offset lives in the anchor (in sprite sizes).
    assert_eq!(
        world.get::<Transform>(e).unwrap().translation.xy(),
        Vec2::new(64.0, 32.0)
    );
    let anchor = |e| world.get::<Anchor>(e).unwrap().0;
    let bottom_left = Anchor::BOTTOM_LEFT.0;
    assert_eq!(anchor(e), bottom_left - Vec2::new(3.0, -5.0) / 32.0);
    assert_eq!(anchor(b), bottom_left - Vec2::new(-16.0, 8.0) / 64.0);

    let [inherits, own] = overlay_sprites(&app, e)[..] else {
        panic!("two overlays");
    };
    assert_eq!(anchor(inherits), anchor(e));
    assert_eq!(anchor(own), bottom_left - Vec2::new(4.0, -3.0) / 32.0);
    let color = |e| world.get::<Sprite>(e).unwrap().color;
    assert_eq!(color(inherits), Color::srgba_u8(10, 20, 30, 128));
    // BYOND multiplies the parent's alpha (128) into an overlay with its own color: 64 -> 32.
    assert_eq!(color(own), Color::srgba_u8(255, 255, 255, 32));
}

/// Runs frames until no page is loading (decoding happens on the task pool).
fn settle(app: &mut App) {
    for _ in 0..500 {
        app.update();
        if app.world().resource::<IconAtlas>().stats().loading == 0 {
            return;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    panic!("atlas pages still loading");
}

#[test]
fn pages_load_when_a_sprite_first_needs_them() {
    let mut app = app("on_demand");
    let stats = |app: &App| app.world().resource::<IconAtlas>().stats();
    assert_eq!((stats(&app).pages, stats(&app).loaded), (2, 0));

    // MOB lives on page 0, BIG on page 1.
    let e = app
        .world_mut()
        .spawn((TilePos::new(1, 1, 1), look(MOB, 3.0)))
        .id();
    app.update();
    // Requested this frame; a tiny page may already be decoded by the end of it.
    assert_eq!(stats(&app).loading + stats(&app).loaded, 1);
    let handle = app.world().get::<Sprite>(e).unwrap().image.clone();
    settle(&mut app);
    let s = stats(&app);
    assert_eq!((s.loaded, s.loading), (1, 0));
    let images = app.world().resource::<Assets<Image>>();
    let page = images
        .get(&handle)
        .expect("page 0 inserted under the sprite's handle");
    assert_eq!(
        s.resident_bytes,
        u64::from(page.width() * page.height() * 4)
    );
    // The decoded page keeps a one-bit alpha mask for mouse picking.
    assert_eq!(
        s.mask_bytes,
        u64::from(page.width() * page.height()).div_ceil(64) * 8
    );

    app.world_mut()
        .spawn((TilePos::new(2, 1, 1), look(BIG, 3.0)));
    app.update();
    settle(&mut app);
    assert_eq!(stats(&app).loaded, 2);

    // A second sprite on a resident page reuses its handle.
    let again = app
        .world_mut()
        .spawn((TilePos::new(3, 1, 1), look(FLOOR, 2.0)))
        .id();
    app.update();
    assert_eq!(app.world().get::<Sprite>(again).unwrap().image, handle);
    assert_eq!(stats(&app).loading, 0);
}

#[test]
fn overlay_planes_and_facings() {
    use bevy::camera::visibility::RenderLayers;
    use ss13_shared::{ABOVE_LIGHTING_PLANE, EMISSIVE_PLANE};

    let mut app = app("planes");
    let mut a = look(MOB, 4.0);
    a.overlays = vec![
        Overlay {
            plane: Some(EMISSIVE_PLANE),
            ..overlay(HAT)
        },
        Overlay {
            plane: Some(ABOVE_LIGHTING_PLANE),
            ..overlay(BADGE)
        },
        Overlay {
            only_dirs: Dir::East as u8,
            ..overlay(HAT)
        },
    ];
    let e = app
        .world_mut()
        .spawn((TilePos::new(1, 1, 1), Facing(Dir::South), a))
        .id();
    // Something on a plane between the parent's and the above-lighting overlay's.
    let between = app
        .world_mut()
        .spawn((TilePos::new(1, 1, 1), {
            let mut b = look(FLOOR, 1.0);
            b.plane = 5;
            b
        }))
        .id();
    app.update();

    let world = app.world();
    let [glow, unlit, east] = overlay_sprites(&app, e)[..] else {
        panic!("three overlays");
    };
    // The emissive mask is only on the emissive camera's layer.
    assert_eq!(
        world.get::<RenderLayers>(glow),
        Some(&RenderLayers::layer(ss13_render::sprites::EMISSIVE_LAYER))
    );
    assert!(world.get::<RenderLayers>(unlit).is_none());
    // The above-lighting overlay sorts above sprites of higher planes than its parent.
    let z = |e: Entity| {
        let parent = world.get::<ChildOf>(e).map(ChildOf::parent);
        let base = parent.map_or(0.0, |p| world.get::<Transform>(p).unwrap().translation.z);
        base + world.get::<Transform>(e).unwrap().translation.z
    };
    assert!(z(unlit) > z(between), "{} vs {}", z(unlit), z(between));
    assert!(z(glow) < z(between));
    // `only_dirs`: hidden facing south, shown facing east.
    assert_eq!(world.get::<Visibility>(east), Some(&Visibility::Hidden));
    app.world_mut().entity_mut(e).insert(Facing(Dir::East));
    app.update();
    assert_eq!(
        app.world().get::<Visibility>(east),
        Some(&Visibility::Inherited)
    );
}
