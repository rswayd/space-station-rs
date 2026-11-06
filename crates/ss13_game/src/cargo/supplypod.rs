//! Supply pods (server), after tg `/obj/structure/closet/supplypod` and `/obj/effect/pod_landingzone`
//! (`code/modules/cargo/supplypod.dm`) as BandaStation delivers them: every delivery is the
//! teleporter pod (`supplypod/teleporter`, `modular_bandastation/aesthetics/supply_pods`), a
//! portal without explosion, smoke or damage.
//!
//! - [`launch`]: the pod lands on the tile after `POD_TRANSIT` + `POD_FALLING` (3.4 s), opens
//!   after `POD_OPENING` (3 s; the cargo appears on its tile: an order's crate via
//!   `supply::generate`, or loose items) and leaves after `POD_LEAVING` (3 s; `bluespace`: it is
//!   deleted).
// shortcut: the landing zone marker, the falling animation, sounds and pod styles are not shown
// (the pod is the portal sprite when the assets have it, else the prototype's look); no reverse
// pods, admin pod launcher or mobs in pods.

use bevy::prelude::*;
use ss13_mapload::IconIndex;
use ss13_mapload::cargo::SUPPLY_POD;
use ss13_shared::TilePos;

use super::supply::SupplyOrder;

/// `delays`, seconds.
pub const TRANSIT_SECS: f64 = 3.0 + 0.4;
pub const OPENING_SECS: f64 = 3.0;
pub const LEAVING_SECS: f64 = 3.0;
/// The teleporter pod's look (`set_style`).
const PORTAL: (&str, &str) = ("icons/obj/anomaly.dmi", "portal");

/// What a pod brings.
#[derive(Clone, Debug, PartialEq)]
pub enum PodCargo {
    /// A supply order: its crate (`supply_order/generate`).
    Order(SupplyOrder),
    /// Items by type (the order consoles' express orders).
    Items(Vec<String>),
}

#[derive(Clone, Debug)]
struct Pod {
    pos: TilePos,
    land_at: f64,
    pod: Option<Entity>,
    cargo: Option<PodCargo>,
}

/// The pods on their way.
#[derive(Resource, Default)]
pub struct SupplyPods(Vec<Pod>);

impl SupplyPods {
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

pub struct SupplyPodPlugin;

impl Plugin for SupplyPodPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SupplyPods>()
            .add_systems(Update, tick.run_if(|p: Res<SupplyPods>| !p.is_empty()));
    }
}

/// `new /obj/effect/pod_landingzone(turf, pod, order)`.
pub fn launch(world: &mut World, pos: TilePos, cargo: PodCargo) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    world.resource_mut::<SupplyPods>().0.push(Pod {
        pos,
        land_at: now + TRANSIT_SECS,
        pod: None,
        cargo: Some(cargo),
    });
}

fn tick(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let pods = std::mem::take(&mut world.resource_mut::<SupplyPods>().0);
    let mut kept = Vec::new();
    for mut pod in pods {
        let since = now - pod.land_at;
        if since < 0.0 {
            kept.push(pod);
            continue;
        }
        if pod.pod.is_none() && pod.cargo.is_some() {
            pod.pod = land(world, pod.pos);
        }
        if since >= OPENING_SECS
            && let Some(cargo) = pod.cargo.take()
        {
            open(world, pod.pos, cargo);
        }
        if since >= OPENING_SECS + LEAVING_SECS {
            if let Some(e) = pod.pod.take()
                && world.get_entity(e).is_ok()
            {
                world.despawn(e);
                crate::construction::tile_changed(world, pod.pos);
            }
            continue;
        }
        kept.push(pod);
    }
    // Pods launched while these were handled are already in the resource.
    world.resource_mut::<SupplyPods>().0.extend(kept);
}

/// `pre_open`: the pod on its tile.
fn land(world: &mut World, pos: TilePos) -> Option<Entity> {
    let pod = crate::construction::spawn_obj(world, SUPPLY_POD, pos)?;
    let portal = world.get_resource::<IconIndex>().and_then(|i| {
        i.0.get(&(PORTAL.0.to_owned(), PORTAL.1.to_owned()))
            .copied()
    });
    if let Some(state) = portal
        && let Some(mut look) = world.get_mut::<ss13_shared::Appearance>(pod)
    {
        look.icon_state = state;
        look.pixel_offset = [0, 0];
    }
    Some(pod)
}

/// `open_pod`: the cargo out onto the tile.
fn open(world: &mut World, pos: TilePos, cargo: PodCargo) {
    match cargo {
        PodCargo::Order(order) => {
            if super::supply::generate(world, &order, pos).is_none() {
                warn!("cargo: express order #{} had no crate to deliver", order.id);
            }
        }
        PodCargo::Items(paths) => {
            for path in paths {
                crate::construction::spawn_obj(world, &path, pos);
            }
        }
    }
    crate::construction::tile_changed(world, pos);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pod_delays_are_tgs() {
        assert!((TRANSIT_SECS - 3.4).abs() < 1e-9);
        assert_eq!(OPENING_SECS + LEAVING_SECS, 6.0);
    }
}
