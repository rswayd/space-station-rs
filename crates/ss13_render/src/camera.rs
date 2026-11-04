//! World camera: integer zoom, pixel-grid snapping, follows `CameraTarget`, draws into the
//! window area that docked UI leaves free (BYOND's map control next to the chat).

use bevy::camera::{ScalingMode, Viewport};
use bevy::prelude::*;
use ss13_shared::{TILE_SIZE, TilePos};

use crate::ActiveZ;
use crate::order::Z_RANGE;

/// The camera the world is drawn with. `zoom` is screen pixels per icon pixel.
#[derive(Component, Debug)]
pub struct WorldCamera {
    pub zoom: u32,
}

/// Marker: the camera centres on this entity and draws its z-level.
#[derive(Component, Debug, Default)]
pub struct CameraTarget;

/// Pixels the view is pushed off its target (BYOND `client.pixel_x`/`pixel_y`, animated by
/// server camera effects such as gun recoil).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct CameraOffset(pub Vec2);

/// Window space reserved by docked UI, in logical pixels: the map is drawn between the insets and
/// the camera centres its target there. Written by whoever docks UI, read every frame.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct MapInsets {
    pub left: f32,
    pub right: f32,
}

/// The world camera's viewport for a render target of `target` physical pixels; `None` for an
/// empty (minimised) target. At least one pixel column always stays for the map.
pub fn map_viewport(target: UVec2, scale_factor: f32, insets: MapInsets) -> Option<Viewport> {
    if target.x == 0 || target.y == 0 {
        return None;
    }
    let physical = |logical: f32| (logical.max(0.0) * scale_factor).round() as u32;
    let left = physical(insets.left).min(target.x - 1);
    let right = physical(insets.right).min(target.x - 1 - left);
    Some(Viewport {
        physical_position: UVec2::new(left, 0),
        physical_size: UVec2::new(target.x - left - right, target.y),
        ..default()
    })
}

/// Keeps the world camera's viewport on the free map area across resizes and inset changes.
/// `Camera::viewport_to_world` takes window positions, so cursor picking needs no offset.
pub fn apply_map_viewport(
    insets: Res<MapInsets>,
    mut cameras: Query<&mut Camera, With<WorldCamera>>,
) {
    for mut camera in &mut cameras {
        let (Some(target), Some(scale)) = (
            camera.physical_target_size(),
            camera.target_scaling_factor(),
        ) else {
            continue;
        };
        let new = map_viewport(target, scale, *insets);
        let same = match (&camera.viewport, &new) {
            (Some(a), Some(b)) => {
                a.physical_position == b.physical_position && a.physical_size == b.physical_size
            }
            (None, None) => true,
            _ => false,
        };
        if !same {
            camera.viewport = new;
        }
    }
}

pub fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        WorldCamera { zoom: 2 },
        // Sprites use z in [0, Z_RANGE), the level below and space under it; see `sprites`.
        Projection::Orthographic(OrthographicProjection {
            near: 0.0,
            far: 3.0 * Z_RANGE + 8.0,
            scaling_mode: ScalingMode::WindowSize,
            ..OrthographicProjection::default_2d()
        }),
        Transform::from_xyz(0.0, 0.0, Z_RANGE),
    ));
}

/// Camera x so that the viewport's left edge falls on a whole icon pixel: with an odd
/// viewport width the centre sits between two screen pixels.
pub fn snap_axis(center: f32, zoom: u32, viewport_px: u32) -> f32 {
    let zoom = zoom as f32;
    let half = viewport_px as f32 / 2.0;
    ((center * zoom - half).round() + half) / zoom
}

pub fn follow_target(
    mut active_z: ResMut<ActiveZ>,
    target: Query<(&TilePos, Option<&Transform>), (With<CameraTarget>, Without<WorldCamera>)>,
    mut cameras: Query<(&WorldCamera, &Camera, &mut Projection, &mut Transform)>,
    offset: Res<CameraOffset>,
) {
    let target = target.iter().next();
    if let Some((pos, _)) = target {
        active_z.set_if_neq(ActiveZ(pos.z));
    }
    for (world, camera, mut projection, mut transform) in &mut cameras {
        let zoom = world.zoom.max(1);
        let scale = camera.target_scaling_factor().unwrap_or(1.0) / zoom as f32;
        if let Projection::Orthographic(ortho) = projection.as_ref()
            && ortho.scale != scale
            && let Projection::Orthographic(ortho) = projection.as_mut()
        {
            ortho.scale = scale;
        }
        let Some((pos, target_transform)) = target else {
            continue;
        };
        // Follow the drawn position (it may glide between tiles), centred on the tile.
        let corner = target_transform.map_or_else(
            || crate::sprites::tile_translation(*pos),
            |t| t.translation.xy(),
        );
        let center = corner + Vec2::splat(TILE_SIZE as f32 / 2.0) + offset.0;
        let size = camera.physical_viewport_size().unwrap_or(UVec2::ONE);
        let snapped = Vec2::new(
            snap_axis(center.x, zoom, size.x),
            snap_axis(center.y, zoom, size.y),
        );
        if transform.translation.xy() != snapped {
            transform.translation.x = snapped.x;
            transform.translation.y = snapped.y;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(v: Viewport) -> (u32, u32, u32, u32) {
        (
            v.physical_position.x,
            v.physical_position.y,
            v.physical_size.x,
            v.physical_size.y,
        )
    }

    #[test]
    fn viewport_leaves_room_for_docked_panel() {
        let panel = MapInsets {
            left: 0.0,
            right: 480.0,
        };
        let none = MapInsets::default();
        for (target, scale, insets, want) in [
            // Default window: tg's 19x15 view at zoom 2 next to the panel.
            ((1696, 960), 1.0, panel, (0, 0, 1216, 960)),
            ((1696 * 2, 960 * 2), 2.0, panel, (0, 0, 2432, 1920)),
            ((1697, 961), 1.0, panel, (0, 0, 1217, 961)),
            ((3395, 1921), 2.0, panel, (0, 0, 2435, 1921)),
            ((1216, 960), 1.0, none, (0, 0, 1216, 960)),
            ((2433, 1921), 2.0, none, (0, 0, 2433, 1921)),
            (
                (800, 600),
                1.5,
                MapInsets {
                    left: 10.0,
                    right: 100.3,
                },
                (15, 0, 635, 600),
            ),
            // Narrower than the panel: one column survives.
            ((300, 200), 1.0, panel, (0, 0, 1, 200)),
        ] {
            let got = map_viewport(UVec2::from(target), scale, insets).map(rect);
            assert_eq!(got, Some(want), "{target:?} x{scale} {insets:?}");
        }
        assert!(map_viewport(UVec2::new(0, 960), 1.0, panel).is_none());
    }

    /// Through Bevy's own projection: the map area's centre (window coordinates, as the cursor
    /// reports them) maps to the target, so it is centred next to the panel and picking needs no
    /// offset.
    #[test]
    fn target_centred_in_map_area() {
        let zoom = 2;
        let center = Vec2::new(1234.0, 567.0);
        for (logical, scale, right) in [
            ((1696.0, 960.0), 1.0, 480.0),
            ((1696.5, 960.5), 2.0, 480.0),
            ((1001.0, 777.0), 1.0, 480.0),
            ((1216.0, 960.0), 2.0, 0.0),
        ] {
            let target = (Vec2::from(logical) * scale).as_uvec2();
            let insets = MapInsets { left: 0.0, right };
            let mut camera = Camera {
                viewport: map_viewport(target, scale, insets),
                ..default()
            };
            camera.computed.target_info = Some(bevy::camera::RenderTargetInfo {
                physical_size: target,
                scale_factor: scale,
            });
            let size = camera.physical_viewport_size().unwrap();
            let mut projection = OrthographicProjection {
                near: 0.0,
                far: 3.0 * Z_RANGE + 8.0,
                scaling_mode: ScalingMode::WindowSize,
                scale: scale / zoom as f32,
                ..OrthographicProjection::default_2d()
            };
            bevy::camera::CameraProjection::update(&mut projection, size.x as f32, size.y as f32);
            camera.computed.clip_from_view =
                bevy::camera::CameraProjection::get_clip_from_view(&projection);
            let cam = Vec2::new(
                snap_axis(center.x, zoom, size.x),
                snap_axis(center.y, zoom, size.y),
            );
            let transform = GlobalTransform::from_translation(cam.extend(Z_RANGE));

            let area = camera.logical_viewport_rect().unwrap();
            assert_eq!(area.min.x, 0.0);
            assert!((area.max.x - (target.x as f32 / scale - right)).abs() <= 0.5 / scale);
            let world = camera
                .viewport_to_world_2d(&transform, area.center())
                .unwrap();
            assert!(
                (world - center).abs().max_element() <= 0.5 / zoom as f32 + 1e-3,
                "{logical:?} x{scale}: {world} vs {center}"
            );
            let on_screen = camera
                .world_to_viewport(&transform, center.extend(0.0))
                .unwrap();
            assert!(area.contains(on_screen));
        }
    }

    #[test]
    fn viewport_edge_lands_on_pixel_grid() {
        for (center, zoom, px) in [(100.3, 2, 801), (16.0, 3, 1280), (-7.7, 1, 33)] {
            let x = snap_axis(center, zoom, px);
            let left_screen_px = x * zoom as f32 - px as f32 / 2.0;
            assert_eq!(left_screen_px, left_screen_px.round());
            assert!((x - center).abs() <= 0.5 / zoom as f32 + 1e-4);
        }
    }
}
