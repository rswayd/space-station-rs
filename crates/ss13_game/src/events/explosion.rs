//! `ex_act` of turfs, structures and mobs (`EX_ACT`: the contents first, then the turf) and a
//! stand-in for `explosion()`, for meteors.
//!
//! - Walls (`/turf/closed/wall/ex_act`): devastating scrapes the wall away, heavy dismantles it
//!   (half the time devastated: sheets only), light dismantles it with `prob(hardness)`.
//! - Floors (`/turf/open/floor/ex_act`): devastating scrapes two layers off, heavy rolls 1-3
//!   (two layers, two layers, or one layer at 80%; iron sheets at 33%), light is cosmetic.
//! - Structures with integrity (`/obj/ex_act`): `INFINITY`, `rand(100, 250)` or `rand(10, 90)`
//!   brute.
//! - Humans (`/mob/living/carbon/human/ex_act`): brute and burn split over the bodyparts,
//!   unconsciousness and a knockdown.

use bevy::prelude::*;
use ss13_mapload::construct::DmTurfBuild;
use ss13_shared::TilePos;
use ss13_world::SpatialIndex;

use super::controller::EventRng;
use crate::construction::{path_of, scrape_away, spawn_obj, spawn_stack, turf_at};
use crate::health::{DamageType, Health, Stat, apply_damage_zone};
use crate::obj_damage::{Integrity, is_under, take_damage};
use crate::zone::BodyZone;

/// tg `EXPLODE_LIGHT` 1, `EXPLODE_HEAVY` 2, `EXPLODE_DEVASTATE` 3.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Light = 1,
    Heavy = 2,
    Devastate = 3,
}

const WALL: &str = "/turf/closed/wall";
const R_WALL: &str = "/turf/closed/wall/r_wall";
const FLOOR: &str = "/turf/open/floor";
const IRON: &str = "/obj/item/stack/sheet/iron";

/// `EX_ACT(turf, severity)`: everything on the tile, then the turf.
pub fn ex_act_tile(world: &mut World, pos: TilePos, severity: Severity) {
    let turf = turf_at(world, pos);
    let things: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| Some(e) != turf)
        .collect();
    for thing in things {
        ex_act_movable(world, thing, severity);
    }
    ex_act_turf(world, pos, severity);
}

/// `ex_act` of one movable: a living mob or a structure with integrity.
// shortcut: items and machines without integrity are untouched; add with item integrity.
pub fn ex_act_movable(world: &mut World, thing: Entity, severity: Severity) {
    if world.get_entity(thing).is_err() {
        return;
    }
    if world.get::<Health>(thing).is_some() {
        ex_act_mob(world, thing, severity);
        return;
    }
    if world.get::<Integrity>(thing).is_none() {
        return;
    }
    let mut rng = world.resource_mut::<EventRng>();
    let damage = match severity {
        Severity::Devastate => f32::INFINITY,
        Severity::Heavy => rng.0.range(100, 250) as f32,
        Severity::Light => rng.0.range(10, 90) as f32,
    };
    take_damage(world, thing, damage, DamageType::Brute);
}

/// `/mob/living/carbon/human/ex_act`, without bomb armor.
// shortcut: no gibbing (a devastating hit deals its 300 brute and burn), wounds, dismemberment,
// thrown bodies, clothes damage or deafness; add with those systems.
pub fn ex_act_mob(world: &mut World, mob: Entity, severity: Severity) {
    let (brute, burn, knockdown, unconscious) = match severity {
        Severity::Devastate => (300.0, 300.0, 20.0, 3.0),
        Severity::Heavy => (60.0, 60.0, 20.0, 2.0),
        Severity::Light => (30.0, 0.0, 16.0, 0.0),
    };
    // shortcut: no unconsciousness status yet: `Unconscious()` paralyzes as long.
    if unconscious > 0.0 {
        crate::status_effects::apply(
            world,
            mob,
            crate::status_effects::Incap::Paralyze,
            unconscious,
        );
    }
    let parts = BodyZone::ALL.len() as f32;
    // `round(loss / bodyparts, DAMAGE_PRECISION)` to each bodypart.
    let share = |loss: f32| (loss / parts * 10.0).round() / 10.0;
    if let Ok(mut entity) = world.get_entity_mut(mob)
        && let Ok((mut health, mut stat)) = entity.get_components_mut::<(&mut Health, &mut Stat)>()
    {
        for zone in BodyZone::ALL {
            for (kind, loss) in [(DamageType::Brute, brute), (DamageType::Burn, burn)] {
                if loss > 0.0 {
                    apply_damage_zone(&mut health, &mut stat, zone, kind, share(loss));
                }
            }
        }
    }
    crate::status_effects::knockdown(world, mob, knockdown, 0.0);
}

/// tg `ex_act` of the turf of `pos`.
pub fn ex_act_turf(world: &mut World, pos: TilePos, severity: Severity) {
    let Some(turf) = turf_at(world, pos) else {
        return;
    };
    let Some(path) = path_of(world, turf) else {
        return;
    };
    if is_under(&path, WALL) {
        let hardness = if is_under(&path, R_WALL) { 10.0 } else { 40.0 };
        let mut rng = world.resource_mut::<EventRng>();
        match severity {
            Severity::Devastate => {
                scrape_away(world, pos, false);
            }
            Severity::Heavy => {
                let devastated = rng.0.prob(50.0);
                dismantle_wall(world, turf, pos, devastated);
            }
            Severity::Light => {
                if rng.0.prob(hardness) {
                    dismantle_wall(world, turf, pos, false);
                }
            }
        }
    } else if is_under(&path, FLOOR) {
        let mut rng = world.resource_mut::<EventRng>();
        match severity {
            Severity::Devastate => scrape(world, pos, 2),
            // shortcut: no lattice for case 1 over space (`attempt_lattice_replacement`), no
            // broken tile icons or the hotspot of case 3.
            Severity::Heavy => match rng.0.range(1, 3) {
                1 | 2 => {
                    let sheet = rng.0.prob(33.0);
                    scrape(world, pos, 2);
                    if sheet {
                        spawn_stack(world, IRON, pos, 1);
                    }
                }
                _ => {
                    let scrape_it = rng.0.prob(80.0);
                    let sheet = rng.0.prob(33.0);
                    if scrape_it {
                        scrape(world, pos, 1);
                    }
                    if sheet {
                        spawn_stack(world, IRON, pos, 1);
                    }
                }
            },
            // `break_tile()` and a hotspot: only the look changes.
            Severity::Light => {}
        }
    }
}

fn scrape(world: &mut World, pos: TilePos, layers: usize) {
    for _ in 0..layers {
        scrape_away(world, pos, true);
    }
}

/// tg `dismantle_wall(devastated, explode)`: `break_wall` (sheets and girder) or
/// `devastate_wall` (sheets, and iron for the girder), then `ScrapeAway`.
fn dismantle_wall(world: &mut World, wall: Entity, pos: TilePos, devastated: bool) {
    let Some(build) = world.get::<DmTurfBuild>(wall).cloned() else {
        return;
    };
    if scrape_away(world, pos, false).is_none() {
        return;
    }
    if !build.sheet_type.is_empty() {
        spawn_stack(world, &build.sheet_type, pos, build.sheet_amount);
    }
    if !build.girder_type.is_empty() {
        if devastated {
            spawn_stack(world, IRON, pos, 1);
        } else {
            spawn_obj(world, &build.girder_type, pos);
        }
    }
}

/// `explosion(epicenter, devastation_range, heavy_impact_range, light_impact_range,
/// flame_range)`: each turf by its distance from the epicenter (`CHEAP_HYPOTENUSE`).
// shortcut: no explosion blocking (`explosive_resistance`), thrown items, flashes, sounds or
// screen shake; replace with a real SSexplosions port.
pub fn explosion(world: &mut World, center: TilePos, dev: i32, heavy: i32, light: i32, flame: i32) {
    let range = dev.max(heavy).max(light).max(flame);
    let Some((w, h)) = world.resource::<SpatialIndex>().level_size(center.z) else {
        return;
    };
    for dy in -range..=range {
        for dx in -range..=range {
            let pos = TilePos::new(center.x + dx, center.y + dy, center.z);
            if pos.x < 1 || pos.y < 1 || pos.x > w as i32 || pos.y > h as i32 {
                continue;
            }
            let dist = f64::from(dx * dx + dy * dy).sqrt();
            let severity = if dist < f64::from(dev) {
                Some(Severity::Devastate)
            } else if dist < f64::from(heavy) {
                Some(Severity::Heavy)
            } else if dist < f64::from(light) {
                Some(Severity::Light)
            } else {
                None
            };
            if let Some(severity) = severity {
                ex_act_tile(world, pos, severity);
            }
            // `prob(40) && dist < flame_range && !isspaceturf && !density`: a hotspot.
            if dist < f64::from(flame) && world.resource_mut::<EventRng>().0.prob(40.0) {
                let open = turf_at(world, pos)
                    .and_then(|t| path_of(world, t))
                    .is_some_and(|p| {
                        is_under(&p, "/turf/open") && !is_under(&p, "/turf/open/space")
                    });
                if open && let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() {
                    // `new /obj/effect/hotspot(T)`: lights only where the air burns here.
                    atmos.hotspot_expose(pos, 1000.0, 2500.0, false);
                }
            }
        }
    }
}
