//! Bluespace shelter capsules (server), after tg `/obj/item/survivalcapsule`
//! (`code/modules/mining/equipment/survival_pod.dm`) and `/datum/map_template/shelter`
//! (`code/modules/mining/shelters.dm`).
//!
//! - Used in hand the capsule shakes ("Stand back!") and 5 s later expands where it lies (or
//!   where its carrier stands), centred on that tile.
//! - `check_deploy`: no tile of the footprint in a shuttle or holodeck area, on a closed turf
//!   other than a mineral wall, holding a dense anchored object or a stone tile; the footprint
//!   stays off the map edge. A refused capsule can be used again.
//! - The living in the footprint are paralyzed (3 s), knocked down (6 s) and thrown out of it,
//!   the closer the further (`yote_nearby`).
//! - The template replaces the footprint's turfs (each placed on top of the turf it replaces)
//!   and areas and brings its contents; what stood there stays.
//! - The fishing capsules (`/obj/item/survivalcapsule/fishing`) choose their template from a
//!   radial menu: fishing (../fishing), not handled here.
// The template loads onto a scratch level (`ss13_mapload::load_template`) and its turfs,
// contents and areas move onto the footprint before the map init pass runs, so everything
// initializes in place.
// shortcut: each deploy leaves its emptied scratch level behind (template-sized, a few KiB);
// load straight onto the target level if the map loader learns to paste onto an existing one.
// shortcut: activations away from the mining level go to the server log, not to admins
// (`trigger_admin_alert`); upgrade with an admin message hook in ss13_game.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_atmos::{Atmos, TurfAir};
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{Areas, DmItem, DmObj, MapLoadPlugin, Turf};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_world::{Density, SpatialIndex};

use crate::construction::{Baseturfs, baseturfs_of, path_of, turf_at};
use crate::examine::Line;
use crate::health::Health;
use crate::obj_damage::Anchored;
use crate::sound::{PlaySound, playsound};
use crate::status_effects::Incap;
use crate::tools::tell;

pub const CAPSULE: &str = "/obj/item/survivalcapsule";
const FISHING: &str = "/obj/item/survivalcapsule/fishing";
const PHASE_IN: &str = "sound/effects/phasein.ogg";
/// `addtimer(expand, 5 SECONDS)`.
pub const EXPAND_DELAY: f64 = 5.0;

/// A `/datum/map_template/shelter`.
pub struct Shelter {
    pub name: &'static str,
    pub description: &'static str,
    pub mappath: &'static str,
}

const ALPHA: Shelter = Shelter {
    name: "Shelter Alpha",
    description: "A cosy self-contained pressurized shelter, with built-in navigation, \
        entertainment, medical facilities and a sleeping area! Order now, and we'll throw in a \
        TINY FAN, absolutely free!",
    mappath: "_maps/templates/shelter_1.dmm",
};
const BETA: Shelter = Shelter {
    name: "Shelter Beta",
    description: "An extremely luxurious shelter, containing all the amenities of home, \
        including carpeted floors, hot and cold running water, a gourmet three course meal, \
        cooking facilities, and a deluxe companion to keep you from getting lonely during an \
        ash storm.",
    mappath: "_maps/templates/shelter_2.dmm",
};
const CHARLIE: Shelter = Shelter {
    name: "Shelter Charlie",
    description: "A luxury elite bar which holds an entire bar along with two vending \
        machines, tables, and a restroom that also has a sink. This isn't a survival capsule \
        and so you can expect that this won't save you if you're bleeding out to death.",
    mappath: "_maps/templates/shelter_3.dmm",
};
const TOILET: Shelter = Shelter {
    name: "Emergency Relief Shelter",
    description: "A stripped-down emergency shelter focused on providing only the most \
        essential amenities to unfortunate employees who find themselves in need far from \
        home.",
    mappath: "_maps/templates/shelter_t.dmm",
};

/// The capsule type's `template_id`; `None`: not a capsule handled here.
pub fn shelter(path: &str) -> Option<&'static Shelter> {
    if !is_under(path, CAPSULE) || is_under(path, FISHING) {
        return None;
    }
    Some(if is_under(path, "/obj/item/survivalcapsule/luxuryelite") {
        &CHARLIE
    } else if is_under(path, "/obj/item/survivalcapsule/luxury") {
        &BETA
    } else if is_under(path, "/obj/item/survivalcapsule/bathroom") {
        &TOILET
    } else {
        &ALPHA
    })
}

/// Where the shelter templates come from; insert before the plugin builds to override (tests).
/// `None`: the BYOND checkout of [`MapLoadPlugin`].
#[derive(Resource, Clone, Debug, Default)]
pub struct ShelterSource(pub Option<PathBuf>);

/// `used`: the capsule is shaking and expands at `at`.
#[derive(Component, Clone, Copy, Debug)]
pub struct CapsuleArmed {
    pub at: f64,
    pub user: Entity,
}

pub(super) struct CapsulePlugin;

impl Plugin for CapsulePlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<ShelterSource>() {
            let source = app
                .world()
                .get_resource::<MapLoadPlugin>()
                .map(|m| m.source.clone());
            app.insert_resource(ShelterSource(source));
        }
        app.add_systems(Update, expand_due);
    }
}

fn now(world: &World) -> f64 {
    world
        .get_resource::<Time>()
        .map_or(0.0, |t| t.elapsed_secs_f64())
}

fn capsule_shelter(world: &World, item: Entity) -> Option<&'static Shelter> {
    world.get::<DmItem>(item).and_then(|i| shelter(&i.path))
}

/// `visible_message` from `pos`.
fn visible(world: &mut World, pos: TilePos, span: Span, text: &str) {
    let clients = crate::obj_damage::clients_in_range(world, pos, 7, &[]);
    for client in clients {
        tell(world, client, span, text);
    }
}

/// `interact`: arms the capsule. True when `item` is a capsule.
pub fn attack_self(world: &mut World, mob: Entity, item: Entity) -> bool {
    if capsule_shelter(world, item).is_none() {
        return false;
    }
    if world.get::<CapsuleArmed>(item).is_some() {
        return true;
    }
    let name = crate::examine::name_of(world, item);
    if let Some(pos) = crate::explosions::turf_of(world, item) {
        visible(
            world,
            pos,
            Span::Warning,
            &format!("{name} begins to shake. Stand back!"),
        );
    }
    let at = now(world) + EXPAND_DELAY;
    world
        .entity_mut(item)
        .insert(CapsuleArmed { at, user: mob });
    true
}

/// "This capsule has the [template.name] stored." and the description.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<Line>) {
    if let Some(s) = capsule_shelter(world, target) {
        lines.push((
            Span::Notice,
            format!("This capsule has the {} stored.", s.name),
        ));
        lines.push((Span::Notice, s.description.to_owned()));
    }
}

fn expand_due(world: &mut World) {
    let t = now(world);
    let due: Vec<(Entity, Entity)> = world
        .query::<(Entity, &CapsuleArmed)>()
        .iter(world)
        .filter(|(_, a)| a.at <= t)
        .map(|(e, a)| (e, a.user))
        .collect();
    for (capsule, user) in due {
        world.entity_mut(capsule).remove::<CapsuleArmed>();
        expand(world, capsule, user);
    }
}

/// `SHELTER_DEPLOY_*` refusals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeployError {
    BadArea,
    BadTurfs,
    AnchoredObjects,
    BannedObjects,
    OutsideMap,
}

/// The footprint's bottom-left tile (`deploy_location - round(size / 2)`), or why it leaves
/// the map (`shelter_origin <= 1 || origin + size > max`).
pub fn origin(
    deploy: TilePos,
    (width, height): (u32, u32),
    (max_x, max_y): (u32, u32),
) -> Result<TilePos, DeployError> {
    let (w, h) = (width as i32, height as i32);
    let ox = deploy.x - w / 2;
    let oy = deploy.y - h / 2;
    if ox <= 1 || ox + w > max_x as i32 || oy <= 1 || oy + h > max_y as i32 {
        return Err(DeployError::OutsideMap);
    }
    Ok(TilePos {
        x: ox,
        y: oy,
        z: deploy.z,
    })
}

fn footprint(origin: TilePos, (width, height): (u32, u32)) -> impl Iterator<Item = TilePos> {
    (0..height as i32).flat_map(move |dy| {
        (0..width as i32).map(move |dx| TilePos {
            x: origin.x + dx,
            y: origin.y + dy,
            z: origin.z,
        })
    })
}

fn anchored(world: &World, e: Entity) -> bool {
    world
        .get::<Anchored>(e)
        .map(|a| a.0)
        .or_else(|| world.get::<DmObj>(e).map(|o| o.anchored))
        .unwrap_or(false)
}

/// `check_deploy` of the tiles: areas, turfs, objects.
fn check_tiles(world: &World, tiles: &[TilePos]) -> Result<(), DeployError> {
    let areas = world.resource::<Areas>();
    let index = world.resource::<SpatialIndex>();
    for &pos in tiles {
        let area = areas
            .area_at(pos)
            .and_then(|a| world.get::<ss13_mapload::DmPower>(a))
            .map(|p| p.path.as_str());
        if area
            .is_some_and(|a| is_under(a, "/area/shuttle") || is_under(a, "/area/station/holodeck"))
        {
            return Err(DeployError::BadArea);
        }
        let turf = turf_at(world, pos).and_then(|t| path_of(world, t));
        if turf
            .is_some_and(|t| is_under(&t, "/turf/closed") && !is_under(&t, "/turf/closed/mineral"))
        {
            return Err(DeployError::BadTurfs);
        }
        for &e in index.on_tile(pos) {
            if world.get::<Turf>(e).is_some() || world.get::<Health>(e).is_some() {
                continue;
            }
            if world.get::<Density>(e).is_some() && anchored(world, e) {
                return Err(DeployError::AnchoredObjects);
            }
            if path_of(world, e).is_some_and(|p| is_under(&p, "/obj/structure/stone_tile")) {
                return Err(DeployError::BannedObjects);
            }
        }
    }
    Ok(())
}

/// The shelter's size: its map's `(width, height)` read from the `.dmm` without loading it.
fn template_size(path: &Path) -> Option<(u32, u32)> {
    let text = ss13_data::read_to_string(path).ok()?;
    let map = ss13_assets::dmm::DmmMap::parse(&text).ok()?;
    Some((map.size.0, map.size.1))
}

/// `expand`.
fn expand(world: &mut World, capsule: Entity, user: Entity) {
    let Some(shelter) = capsule_shelter(world, capsule) else {
        return;
    };
    let Some(deploy) = crate::explosions::turf_of(world, capsule) else {
        return;
    };
    let Some(path) = world
        .resource::<ShelterSource>()
        .0
        .as_ref()
        .map(|s| s.join(shelter.mappath))
    else {
        warn!("shelter template {}: no source", shelter.mappath);
        return;
    };
    let Some(size) = template_size(&path) else {
        warn!("shelter template {} not found", path.display());
        return;
    };
    let name = crate::examine::name_of(world, capsule);
    let max = world
        .resource::<SpatialIndex>()
        .level_size(deploy.z)
        .unwrap_or((0, 0));
    let checked = origin(deploy, size, max).and_then(|o| {
        let tiles: Vec<TilePos> = footprint(o, size).collect();
        check_tiles(world, &tiles).map(|()| (o, tiles))
    });
    let (origin, tiles) = match checked {
        Ok(ok) => ok,
        Err(DeployError::BadArea) => {
            visible(
                world,
                deploy,
                Span::Warning,
                &format!("{name} will not function in this area."),
            );
            return;
        }
        Err(_) => {
            visible(
                world,
                deploy,
                Span::Warning,
                &format!(
                    "{name} doesn't have room to deploy! You need to clear a {}x{} area!",
                    size.0, size.1
                ),
            );
            return;
        }
    };
    yote_nearby(world, deploy, size);
    let scratch = match ss13_mapload::load_template(world, &path) {
        Ok(z) => z,
        Err(e) => {
            warn!("shelter template {}: {e:#}", path.display());
            ss13_world::run_map_init(world);
            return;
        }
    };
    paste(world, scratch, origin, size);
    ss13_world::run_map_init(world);
    if ss13_mapload::mining_z(world) != Some(deploy.z) {
        let who = crate::examine::name_of(world, user);
        info!(
            "{who} activated a bluespace capsule away from the mining level at {:?}",
            deploy
        );
    }
    playsound(world, PlaySound::new(deploy, PHASE_IN, 100.0, true));
    if let Some(mut atmos) = world.get_resource_mut::<Atmos>() {
        for &pos in &tiles {
            atmos.blocker_changed(pos, false);
        }
    }
    let (min, maxp) = (
        (origin.x, origin.y),
        (origin.x + size.0 as i32 - 1, origin.y + size.1 as i32 - 1),
    );
    ss13_mapload::light::relight_rect(world, origin.z, min, maxp);
    world.despawn(capsule);
}

/// `template.load(centered = TRUE)`: the scratch level's turfs, contents and areas onto the
/// footprint at `origin`.
fn paste(world: &mut World, scratch: u16, origin: TilePos, (width, height): (u32, u32)) {
    for dy in 0..height as i32 {
        for dx in 0..width as i32 {
            let from = TilePos {
                x: 1 + dx,
                y: 1 + dy,
                z: scratch,
            };
            let to = TilePos {
                x: origin.x + dx,
                y: origin.y + dy,
                z: origin.z,
            };
            let area = world.resource::<Areas>().area_at(from);
            if let Some(area) = area {
                world.resource_mut::<Areas>().set_area(to, area);
            }
            let here: Vec<Entity> = world.resource::<SpatialIndex>().on_tile(from).to_vec();
            let new_turf = here
                .iter()
                .copied()
                .find(|&e| world.get::<Turf>(e).is_some());
            if let Some(turf) = new_turf {
                // `PlaceOnTop`: an open turf it covers joins the new turf's stack.
                let mut stack = Vec::new();
                if let Some(old) = turf_at(world, to) {
                    stack = baseturfs_of(world, old);
                    if let Some(p) = path_of(world, old)
                        && is_under(&p, "/turf/open")
                    {
                        stack.push(p);
                    }
                    world.despawn(old);
                }
                world.entity_mut(turf).insert((Baseturfs(stack), to));
                let air = world.get::<TurfAir>(turf).cloned();
                if let (Some(air), Some(mut atmos)) = (air, world.get_resource_mut::<Atmos>()) {
                    atmos.change_turf(to, &air);
                }
            }
            for e in here {
                // Atmos visuals belong to their tile, not to the template.
                let visual = world.get::<ss13_atmos::GasOverlay>(e).is_some()
                    || world.get::<ss13_atmos::HotspotEffect>(e).is_some();
                if Some(e) != new_turf && !visual {
                    world.entity_mut(e).insert(to);
                }
            }
            crate::construction::tile_changed(world, to);
        }
    }
}

/// `yote_nearby`: the living in `range(loc, "[width]x[height]")` are stunned and thrown out of
/// the footprint, the closer the further.
fn yote_nearby(world: &mut World, deploy: TilePos, size: (u32, u32)) {
    let (hw, hh) = (size.0 as i32 / 2, size.1 as i32 / 2);
    let living: Vec<(Entity, TilePos)> = world
        .resource::<SpatialIndex>()
        .in_rect(
            deploy.z,
            (deploy.x - hw, deploy.y - hh),
            (deploy.x + hw, deploy.y + hh),
        )
        .filter(|&e| world.get::<Health>(e).is_some())
        .filter_map(|e| world.get::<TilePos>(e).map(|&p| (e, p)))
        .collect();
    for (mob, pos) in living {
        let (dx, dy) = ((pos.x - deploy.x).signum(), (pos.y - deploy.y).signum());
        let (dx, dy) = if (dx, dy) == (0, 0) {
            let dirs = [
                (0, 1),
                (1, 1),
                (1, 0),
                (1, -1),
                (0, -1),
                (-1, -1),
                (-1, 0),
                (-1, 1),
            ];
            let i = world.resource_mut::<crate::combat::CombatRng>().range(0, 7) as usize;
            dirs[i]
        } else {
            (dx, dy)
        };
        let dist = throw_distance(size, (dx, dy), (pos.x - deploy.x, pos.y - deploy.y));
        crate::status_effects::apply(world, mob, Incap::Paralyze, 3.0);
        crate::status_effects::knockdown(world, mob, 6.0, 0.0);
        let target = TilePos {
            x: pos.x + dx * dist as i32,
            y: pos.y + dy * dist as i32,
            z: pos.z,
        };
        crate::throw::throw_at(world, mob, target, dist, 3.0);
    }
}

/// `yote_nearby`'s `throw_dist`: just enough to leave a `size` footprint from `offset`, going
/// `dir`.
pub fn throw_distance(size: (u32, u32), dir: (i32, i32), offset: (i32, i32)) -> u32 {
    let bx = size.0.div_ceil(2) as f64;
    let by = size.1.div_ceil(2) as f64;
    let (x, y) = (offset.0.abs() as f64, offset.1.abs() as f64);
    let d = if dir.0 != 0 && dir.1 != 0 {
        ((bx * bx + by * by).sqrt() - (x * x + y * y).sqrt()).ceil()
    } else if dir.1 != 0 {
        by - y + 1.0
    } else {
        bx - x + 1.0
    };
    d.max(0.0) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capsule_types_pick_their_shelter() {
        assert_eq!(shelter(CAPSULE).unwrap().name, "Shelter Alpha");
        assert_eq!(
            shelter("/obj/item/survivalcapsule/luxury").unwrap().name,
            "Shelter Beta"
        );
        assert_eq!(
            shelter("/obj/item/survivalcapsule/luxuryelite")
                .unwrap()
                .name,
            "Shelter Charlie"
        );
        assert_eq!(
            shelter("/obj/item/survivalcapsule/bathroom")
                .unwrap()
                .mappath,
            "_maps/templates/shelter_t.dmm"
        );
        assert!(shelter("/obj/item/survivalcapsule/fishing").is_none());
        assert!(shelter("/obj/item/pickaxe").is_none());
    }

    #[test]
    fn footprint_is_centred_and_stays_off_the_edge() {
        let deploy = TilePos { x: 10, y: 10, z: 2 };
        let o = origin(deploy, (5, 5), (255, 255)).unwrap();
        assert_eq!((o.x, o.y), (8, 8));
        assert_eq!(footprint(o, (5, 5)).count(), 25);
        assert!(footprint(o, (5, 5)).any(|p| p == deploy));
        // `origin <= 1`.
        let edge = TilePos { x: 3, y: 10, z: 2 };
        assert_eq!(
            origin(edge, (5, 5), (255, 255)),
            Err(DeployError::OutsideMap)
        );
        let top = TilePos {
            x: 10,
            y: 254,
            z: 2,
        };
        assert_eq!(
            origin(top, (5, 5), (255, 255)),
            Err(DeployError::OutsideMap)
        );
    }

    #[test]
    fn closer_mobs_fly_further() {
        // Beside the centre of a 5x5 shelter: 3 - 1 + 1.
        assert_eq!(throw_distance((5, 5), (1, 0), (1, 0)), 3);
        assert_eq!(throw_distance((5, 5), (0, -1), (0, -2)), 2);
        // Diagonal: ceil(sqrt(18) - sqrt(2)) = 3.
        assert_eq!(throw_distance((5, 5), (1, 1), (1, 1)), 3);
    }
}
