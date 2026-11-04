//! tg `recoil_camera` (`code/modules/mob/mob_helpers.dm`): a shot pushes the shooter's view
//! back along the firing line ([`ss13_shared::protocol::CameraEffect::Recoil`]); the client
//! animates it (`crate::client`).

use bevy::prelude::*;
use ss13_net::camera::server::SendCameraEffect;
use ss13_shared::protocol::CameraEffect;

/// tg `recoil_camera(mob, duration, backtime_duration, strength, angle)`: `strength` tiles
/// towards `angle` (degrees clockwise from north) over `duration` deciseconds, back over
/// `backtime`. Nothing for a mob without a client or under a decisecond.
pub fn recoil_camera(
    world: &mut World,
    mob: Entity,
    duration: f32,
    backtime: f32,
    strength: f32,
    angle: f32,
) {
    if duration < 1.0 {
        return;
    }
    let Some(client) = world.get::<crate::server::PlayerMob>(mob).map(|p| p.client) else {
        return;
    };
    world.write_message(SendCameraEffect {
        client,
        effect: recoil_effect(duration, backtime, strength, angle),
    });
}

/// The pixels and times of a recoil kick (`mpx = sin(angle) * strength * world.icon_size`).
pub fn recoil_effect(duration: f32, backtime: f32, strength: f32, angle: f32) -> CameraEffect {
    let px = strength * ss13_shared::TILE_SIZE as f32;
    let (sin, cos) = angle.to_radians().sin_cos();
    CameraEffect::Recoil {
        offset: [(sin * px).round() as i16, (cos * px).round() as i16],
        out_ds: duration,
        back_ds: backtime,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recoil_pushes_against_the_shot() {
        // Shot east (90 degrees): the view goes west (270), one tile for recoil 1.
        let CameraEffect::Recoil {
            offset,
            out_ds,
            back_ds,
        } = recoil_effect(2.0, 2.5, 1.0, 270.0)
        else {
            panic!("a recoil");
        };
        assert_eq!(offset, [-32, 0]);
        assert_eq!((out_ds, back_ds), (2.0, 2.5));
    }
}
