//! Camera Failure (`code/modules/events/camerafailure.dm`): random cameras of the station
//! network go dark (tg `toggle_cam(null, 0)`): the first for sure, then each next at
//! `round(100 / iterations)` percent, `iterations` growing 2.5 times per camera turned off.
// shortcut: a camera turned off stays off: cameras cannot be toggled back by hand yet
// (`crate::silicon::cameras`).

use bevy::prelude::*;

use super::controller::{EventRng, EventState, RoundEvent};
use crate::silicon::cameras::Camera;

pub const NAME: &str = "Camera Failure";
/// `CAMERANET_NETWORK_SS13`.
const NETWORK_SS13: &str = "ss13";

pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(CameraFailure)
}

struct CameraFailure;

impl RoundEvent for CameraFailure {
    fn start(&mut self, world: &mut World, _: &mut EventState) {
        fail_cameras(world);
    }
}

/// `/datum/round_event/camera_failure/start()`: the cameras it turned off.
pub fn fail_cameras(world: &mut World) -> Vec<Entity> {
    let mut q = world.query_filtered::<Entity, With<Camera>>();
    let mut cameras: Vec<Entity> = q.iter(world).collect();
    cameras.sort();
    let mut off = Vec::new();
    let mut iterations = 1.0f64;
    loop {
        let chance = (100.0 / iterations).round() as f32;
        if !world.resource_mut::<EventRng>().0.prob(chance) || cameras.is_empty() {
            break;
        }
        let i = world
            .resource_mut::<EventRng>()
            .0
            .range(0, cameras.len() as i64 - 1) as usize;
        let camera = cameras.swap_remove(i);
        let Some(cam) = world.get::<Camera>(camera) else {
            continue;
        };
        // tg `continue`s here without counting the iteration.
        if !cam.network.iter().any(|n| n == NETWORK_SS13) {
            continue;
        }
        if cam.enabled {
            crate::emp::machines::set_camera(world, camera, false);
            off.push(camera);
        }
        iterations *= 2.5;
    }
    off
}
