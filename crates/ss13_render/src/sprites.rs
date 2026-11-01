//! `Appearance` + `Facing` + `TilePos` -> Bevy sprites. Driven by change detection: an unchanged
//! entity costs one change-tick check per frame, nothing is rebuilt.
//!
//! The sprite `Transform` sits exactly on the tile; BYOND pixel offsets go into each sprite's
//! `Anchor`, so tile movement and z ordering never have to know about them.

use bevy::camera::visibility::RenderLayers;
use bevy::ecs::entity::EntityHashSet;
use bevy::prelude::*;
use bevy::sprite::Anchor;
use ss13_shared::{
    Appearance, Dir, EMISSIVE_PLANE, Facing, IconStateId, Overlay, TILE_SIZE, TilePos,
};

use crate::ActiveZ;
use crate::anim::frame_at;
use crate::atlas::{IconAtlas, frame_count, frame_rect};
use crate::order::{DrawOrder, overlay_z};

/// The icon state a sprite entity (base or overlay) currently shows.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct IconSprite {
    pub state: IconStateId,
    pub dir: Dir,
}

/// Present only on sprites whose state has more than one frame.
#[derive(Component, Clone, Copy, Debug)]
pub struct Animated {
    /// `Time::elapsed_secs_f64` when the state was first shown.
    pub start: f64,
    pub frame: usize,
    /// Nothing to do before this time; `INFINITY` once a finite animation has ended.
    pub next_change: f64,
}

/// Overlay sprite entities of a base sprite, in `Appearance::overlays` order.
/// They are children (`ChildOf`) of the base, so they move, hide and despawn with it.
#[derive(Component, Default, Debug)]
pub struct Overlays(pub Vec<Entity>);

/// Render layer of emissive masks: only the lighting's emissive camera draws them
/// ([`crate::lighting`]).
pub const EMISSIVE_LAYER: usize = 1;

/// Local z of overlay `i` of a base sprite at `base_z`. An overlay on another plane than its
/// parent (above the lighting) is ordered as a sprite of that plane at the parent's layer.
fn overlay_local_z(
    order: &DrawOrder,
    appearance: &Appearance,
    overlay: &Overlay,
    i: usize,
    base_z: f32,
    tiebreak: u32,
) -> f32 {
    match overlay.plane {
        Some(plane) if plane != appearance.plane && plane != EMISSIVE_PLANE => {
            order.z(plane, appearance.layer.0, tiebreak) - base_z + overlay_z(i)
        }
        _ => overlay_z(i),
    }
}

/// Overlays on another plane than their parent (emissive masks aside).
fn crosses_planes(appearance: &Appearance) -> bool {
    appearance.overlays.iter().any(|o| {
        o.plane
            .is_some_and(|p| p != appearance.plane && p != EMISSIVE_PLANE)
    })
}

pub fn tile_translation(pos: TilePos) -> Vec2 {
    Vec2::new(pos.x as f32, pos.y as f32) * TILE_SIZE as f32
}

fn tint(color: Option<[u8; 4]>) -> Color {
    color.map_or(Color::WHITE, |[r, g, b, a]| Color::srgba_u8(r, g, b, a))
}

/// An overlay's color: its own, or the parent's; BYOND multiplies the parent's alpha into it
/// either way (without `RESET_ALPHA`), so a faded mob fades its colored overlays too.
fn overlay_color(own: Option<[u8; 4]>, parent: Option<[u8; 4]>) -> Option<[u8; 4]> {
    match (own, parent) {
        (Some([r, g, b, a]), Some([.., pa])) => {
            Some([r, g, b, (u16::from(a) * u16::from(pa) / 255) as u8])
        }
        (own, parent) => own.or(parent),
    }
}

fn pixels([x, y]: [i16; 2]) -> Vec2 {
    Vec2::new(f32::from(x), f32::from(y))
}

/// BYOND `transform` turn (clockwise degrees) as a rotation of the sprite entity, and the base
/// sprite's offset in the entity's (rotated) frame: the turn is around the tile's center instead
/// of the entity origin (the tile's bottom-left corner, where every sprite is anchored), and
/// `pixel_offset` stays a screen-space shift, as in BYOND.
// shortcut: turns around the 32x32 tile center; BYOND turns around the icon's center, which
// differs for larger icons. Use the base state's size once big sprites get rotated.
fn turn(degrees: u16, pixel_offset: Vec2) -> (Quat, Vec2) {
    if degrees.is_multiple_of(360) {
        return (Quat::IDENTITY, pixel_offset);
    }
    let rotation = Quat::from_rotation_z(-f32::from(degrees).to_radians());
    let center = Vec2::splat(TILE_SIZE as f32 / 2.0);
    let local = rotation.inverse() * (center + pixel_offset).extend(0.0);
    (rotation, local.xy() - center)
}

/// Bottom-left anchor moved by `offset` pixels; `Anchor` is in units of the sprite size.
fn offset_anchor(offset: Vec2, size: Vec2) -> Anchor {
    Anchor(Anchor::BOTTOM_LEFT.0 - offset / size)
}

/// Builds the sprite for `state`, keeping the animation clock when the state did not change
/// (so turning or retinting does not restart an animation).
fn make_sprite(
    atlas: &mut IconAtlas,
    icon: IconSprite,
    color: Color,
    offset: Vec2,
    prev: Option<(&IconSprite, Option<&Animated>)>,
    now: f64,
) -> (Sprite, Anchor, Option<Animated>) {
    let Some((info, image)) = atlas.state(icon.state) else {
        // Missing state: draw a visible magenta tile instead of silently nothing.
        let size = Vec2::splat(TILE_SIZE as f32);
        let sprite = Sprite::from_color(Color::srgb(1.0, 0.0, 1.0), size);
        return (sprite, offset_anchor(offset, size), None);
    };
    let anim = (frame_count(info) > 1).then(|| {
        let start = match prev {
            Some((p, Some(a))) if p.state == icon.state => a.start,
            _ => now,
        };
        let (frame, next) = frame_at(info, (now - start) as f32);
        Animated {
            start,
            frame,
            next_change: next.map_or(f64::INFINITY, |n| now + f64::from(n)),
        }
    });
    let sprite = Sprite {
        image,
        rect: frame_rect(info, icon.dir, anim.map_or(0, |a| a.frame)),
        color,
        ..default()
    };
    let size = Vec2::new(info.width as f32, info.height as f32);
    (sprite, offset_anchor(offset, size), anim)
}

fn insert_sprite(
    entity: &mut EntityCommands,
    icon: IconSprite,
    (sprite, anchor, anim): (Sprite, Anchor, Option<Animated>),
) {
    entity.insert((sprite, anchor, icon));
    match anim {
        Some(a) => entity.insert(a),
        None => entity.remove::<Animated>(),
    };
}

type AppearanceItem<'a> = (
    Entity,
    Ref<'a, Appearance>,
    Ref<'a, TilePos>,
    Option<Ref<'a, Facing>>,
    Option<&'a IconSprite>,
    Option<&'a Animated>,
    Option<&'a mut Transform>,
    Option<&'a Overlays>,
);

/// Rebuilds the sprite and overlays of every entity whose `Appearance` or `Facing` changed (or
/// lost its `Facing`), or that just got a `TilePos`. Recomputes every z only when a new
/// `(plane, layer)` pair shifted the ranks.
// shortcut: a turn rebuilds all sprites of the entity (a handful of inserts); add a rect-only
// path if profiling shows turning crowds as a hot spot.
pub fn sync_appearance(
    mut commands: Commands,
    mut atlas: ResMut<IconAtlas>,
    mut order: ResMut<DrawOrder>,
    time: Res<Time>,
    changed: Query<&Appearance, (With<TilePos>, Or<(Changed<Appearance>, Added<TilePos>)>)>,
    turned: Query<(), (With<Appearance>, With<TilePos>, Changed<Facing>)>,
    mut lost_facing: RemovedComponents<Facing>,
    mut entities: Query<AppearanceItem>,
    overlay_state: Query<(&IconSprite, Option<&Animated>)>,
    mut overlay_transforms: Query<&mut Transform, Without<Appearance>>,
) {
    let mut rekey = false;
    for a in &changed {
        rekey |= order.insert(a.plane, a.layer.0);
        for o in &a.overlays {
            if let Some(plane) = o.plane.filter(|&p| p != a.plane && p != EMISSIVE_PLANE) {
                rekey |= order.insert(plane, a.layer.0);
            }
        }
    }
    let lost_facing: EntityHashSet = lost_facing.read().collect();
    if changed.is_empty() && turned.is_empty() && lost_facing.is_empty() {
        return;
    }
    let now = time.elapsed_secs_f64();
    for (entity, appearance, pos, facing, icon, anim, transform, overlays) in &mut entities {
        let dirty = appearance.is_changed()
            || pos.is_added()
            || facing.as_ref().is_some_and(Ref::is_changed)
            || lost_facing.contains(&entity);
        if !dirty && !rekey {
            continue;
        }
        let tiebreak = entity.index_u32();
        let z = order.z(appearance.plane, appearance.layer.0, tiebreak);
        let (rotation, offset) = turn(appearance.rotation, pixels(appearance.pixel_offset));
        match transform {
            Some(mut t) => {
                t.translation.z = z;
                t.rotation = rotation;
            }
            None => {
                let xy = tile_translation(*pos);
                commands
                    .entity(entity)
                    .insert(Transform::from_xyz(xy.x, xy.y, z).with_rotation(rotation));
            }
        }
        if !dirty {
            // Rekeyed: overlays on other planes follow their plane's new rank.
            if crosses_planes(&appearance) {
                let children = overlays.map_or(&[][..], |o| &o.0);
                for (i, (o, &child)) in appearance.overlays.iter().zip(children).enumerate() {
                    if let Ok(mut t) = overlay_transforms.get_mut(child) {
                        t.translation.z = overlay_local_z(&order, &appearance, o, i, z, tiebreak);
                    }
                }
            }
            continue;
        }

        let dir = facing.map_or(Dir::South, |f| f.0);
        // Overlays are children: they inherit the rotation, their offsets are in its frame.
        let base = IconSprite {
            state: appearance.icon_state,
            dir,
        };
        let built = make_sprite(
            &mut atlas,
            base,
            tint(appearance.color),
            offset,
            icon.zip(Some(anim)),
            now,
        );
        insert_sprite(&mut commands.entity(entity), base, built);

        let old: &[Entity] = overlays.map_or(&[], |o| &o.0);
        let mut kept = Vec::with_capacity(appearance.overlays.len());
        for (i, overlay) in appearance.overlays.iter().enumerate() {
            let icon = IconSprite {
                state: overlay.icon_state,
                dir: overlay.dir.unwrap_or(dir),
            };
            let prev = old.get(i).and_then(|&e| overlay_state.get(e).ok());
            let built = make_sprite(
                &mut atlas,
                icon,
                tint(overlay_color(overlay.color, appearance.color)),
                offset + pixels(overlay.pixel_offset),
                prev,
                now,
            );
            let local_z = overlay_local_z(&order, &appearance, overlay, i, z, tiebreak);
            let child = match old.get(i) {
                Some(&e) => {
                    if let Ok(mut t) = overlay_transforms.get_mut(e) {
                        t.translation.z = local_z;
                    }
                    e
                }
                None => commands
                    .spawn((Transform::from_xyz(0.0, 0.0, local_z), ChildOf(entity)))
                    .id(),
            };
            let mut child_commands = commands.entity(child);
            insert_sprite(&mut child_commands, icon, built);
            child_commands.insert(if overlay.shown_for(dir) {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
            if overlay.plane == Some(EMISSIVE_PLANE) {
                child_commands.insert(RenderLayers::layer(EMISSIVE_LAYER));
            } else {
                child_commands.remove::<RenderLayers>();
            }
            kept.push(child);
        }
        for &extra in old.iter().skip(kept.len()) {
            commands.entity(extra).despawn();
        }
        if !kept.is_empty() || overlays.is_some() {
            commands.entity(entity).insert(Overlays(kept));
        }
    }
}

/// Stops drawing entities that lost `Appearance` or `TilePos` (e.g. picked up into a hand).
pub fn strip_removed(
    mut commands: Commands,
    mut lost_appearance: RemovedComponents<Appearance>,
    mut lost_pos: RemovedComponents<TilePos>,
    drawn: Query<Option<&Overlays>, With<IconSprite>>,
) {
    for entity in lost_appearance.read().chain(lost_pos.read()) {
        // Despawned entities fail the query; their overlays went with them.
        let Ok(overlays) = drawn.get(entity) else {
            continue;
        };
        for &o in overlays.map_or(&[][..], |o| &o.0) {
            commands.entity(o).despawn();
        }
        commands
            .entity(entity)
            .remove::<(Sprite, IconSprite, Animated, Overlays)>();
    }
}

/// Moves sprites to their tile and shows only the camera's z-level.
pub fn sync_tile_pos(
    active_z: Res<ActiveZ>,
    mut moved: Query<(
        Ref<TilePos>,
        Ref<IconSprite>,
        &mut Transform,
        &mut Visibility,
    )>,
) {
    let all = active_z.is_changed();
    for (pos, icon, mut transform, mut visibility) in &mut moved {
        if !all && !pos.is_changed() && !icon.is_added() {
            continue;
        }
        let xy = tile_translation(*pos);
        if transform.translation.xy() != xy {
            transform.translation.x = xy.x;
            transform.translation.y = xy.y;
        }
        let wanted = if pos.z == active_z.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        visibility.set_if_neq(wanted);
    }
}

/// Advances frames. Each sprite is touched only when its current frame runs out.
pub fn animate(
    atlas: Res<IconAtlas>,
    time: Res<Time>,
    mut sprites: Query<(&IconSprite, &mut Animated, &mut Sprite)>,
) {
    let now = time.elapsed_secs_f64();
    sprites
        .par_iter_mut()
        .for_each(|(icon, mut anim, mut sprite)| {
            if now < anim.next_change {
                return;
            }
            let Some(info) = atlas.manifest.get(icon.state) else {
                return;
            };
            let (frame, next) = frame_at(info, (now - anim.start) as f32);
            anim.next_change = next.map_or(f64::INFINITY, |n| now + f64::from(n));
            if frame != anim.frame {
                anim.frame = frame;
                sprite.rect = frame_rect(info, icon.dir, frame);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlays_take_the_parent_alpha() {
        let faded = Some([255, 255, 255, 40]);
        assert_eq!(overlay_color(None, faded), faded);
        assert_eq!(
            overlay_color(Some([10, 20, 30, 255]), faded),
            Some([10, 20, 30, 40])
        );
        assert_eq!(
            overlay_color(Some([10, 20, 30, 255]), None),
            Some([10, 20, 30, 255])
        );
        // A parent tint without alpha leaves the overlay's own color as it was.
        let tint = Some([100, 0, 0, 255]);
        assert_eq!(
            overlay_color(Some([1, 2, 3, 128]), tint),
            Some([1, 2, 3, 128])
        );
    }

    #[test]
    fn turned_sprites_turn_around_the_tile_center_with_a_screen_space_offset() {
        let center = Vec2::splat(TILE_SIZE as f32 / 2.0);
        for degrees in [0, 90, 180, 270] {
            let pixel = Vec2::new(0.0, -6.0);
            let (rotation, offset) = turn(degrees, pixel);
            // The sprite's center in world space, relative to the tile's corner.
            let drawn = rotation * (offset + center).extend(0.0);
            assert!((drawn.xy() - (center + pixel)).length() < 1e-4, "{degrees}");
        }
        let (rotation, _) = turn(90, Vec2::ZERO);
        // Clockwise, as BYOND `Turn`: the sprite's top (+y) points east (+x).
        let up = rotation * Vec3::Y;
        assert!((up - Vec3::X).length() < 1e-5);
    }
}
