//! Food left on the floor goes bad (server): tg `/datum/component/germ_sensitive` and
//! `/datum/component/decomposition` (`datums/components/food/`), added by
//! `/obj/item/food/make_germ_sensitive`.
//!
//! - Food that decomposes ([`DmFood::decomposes`]) is exposed while it lies on an open floor
//!   with no table, rack, closet or kitchen/botany/medical machine on the tile
//!   (`typecache_elevated_structures`) and not in space. After 5 s of it (the five-second rule)
//!   germs reach it: the first time only marks a map-placed (or `decomp_req_handle`) food as
//!   handled, after that its decomposition timer runs while it stays exposed. Map-placed food is
//!   safe until it has moved.
//! - When the timer runs out (`decomp_secs`: 5 minutes raw, 7 gross, 10 otherwise), it becomes
//!   its `decomp_type` (moldy food) with its reagents, ants if it attracts them, and "gets
//!   overtaken by mold".
//! - Examined, it looks stale past a quarter of its time, gross past a half, barely edible past
//!   three quarters.
// shortcut: no floor diseases (`/datum/component/infective`), no stink particles; checked every
// second rather than on movement signals.

use bevy::prelude::*;
use ss13_mapload::{DmFood, DmObj, DmPower, DmTurfBuild, Turf};
use ss13_reagents::is_under;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_world::SpatialIndex;

use crate::chem::{Chem, Reagents, name_of, visible};

/// `GERM_EXPOSURE_DELAY`, seconds.
const GERM_DELAY: f32 = 5.0;
const ANTS: &str = "/obj/effect/decal/cleanable/ants";
/// `typecache_elevated_structures`.
const ELEVATED: [&str; 20] = [
    "/obj/machinery/conveyor",
    "/obj/structure/closet",
    "/obj/structure/rack",
    "/obj/structure/table",
    "/obj/machinery/smartfridge",
    "/obj/structure/bonfire",
    "/obj/machinery/grill",
    "/obj/machinery/griddle",
    "/obj/machinery/deepfryer",
    "/obj/machinery/processor",
    "/obj/machinery/microwave",
    "/obj/machinery/oven",
    "/obj/machinery/stove",
    "/obj/machinery/gibber",
    "/obj/machinery/icecream_vat",
    "/obj/machinery/biogenerator",
    "/obj/machinery/hydroponics",
    "/obj/machinery/stasis",
    "/obj/machinery/chem_master",
    "/obj/machinery/plumbing/pill_press",
];

/// A decomposing food's state.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Decay {
    /// `handled`: false until germs first reach a map-placed or `decomp_req_handle` food.
    pub handled: bool,
    /// Map-placed and not moved yet (`germ_sensitive` without `handle_movement`).
    pub protected_at: Option<TilePos>,
    /// Seconds left (`time_remaining`) and in all (`original_time`).
    pub remaining: f32,
    pub original: f32,
    /// Seconds exposed in this stay on the floor.
    pub exposed_for: f32,
    /// The decomposition timer runs (`decomp_timerid`).
    pub running: bool,
}

#[derive(Resource, Default)]
struct DecayClock(f32);

pub struct DecayPlugin;

impl Plugin for DecayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DecayClock>()
            .add_systems(Startup, protect_mapped)
            .add_systems(Update, decay_tick.run_if(any_with_component::<Decay>))
            .add_observer(add_decay);
    }
}

fn add_decay(add: On<Add<DmFood>>, foods: Query<&DmFood>, mut commands: Commands) {
    let Ok(food) = foods.get(add.entity) else {
        return;
    };
    if !food.decomposes {
        return;
    }
    commands.entity(add.entity).insert_if_new(Decay {
        handled: !food.decomp_req_handle,
        protected_at: None,
        remaining: food.decomp_secs,
        original: food.decomp_secs,
        exposed_for: 0.0,
        running: false,
    });
}

/// `Initialize(mapload)`: the map's food is protected until moved and not handled yet.
fn protect_mapped(mut foods: Query<(&mut Decay, Option<&TilePos>)>) {
    for (mut decay, pos) in &mut foods {
        decay.handled = false;
        decay.protected_at = Some(pos.copied().unwrap_or(TilePos::new(0, 0, 0)));
    }
}

/// tg `germ_sensitive/handle_movement`: on an open floor with nothing elevated on it.
pub fn exposed(world: &World, food: Entity) -> bool {
    if world.get::<crate::inventory::CarriedBy>(food).is_some()
        || world.get::<crate::storage::StoredIn>(food).is_some()
    {
        return false;
    }
    let Some(&pos) = world.get::<TilePos>(food) else {
        return false;
    };
    let index = world.resource::<SpatialIndex>();
    let mut open = false;
    for &e in index.on_tile(pos) {
        if world.get::<Turf>(e).is_some() {
            open = world.get::<DmTurfBuild>(e).is_some_and(|t| {
                is_under(&t.path, "/turf/open")
                    && !is_under(&t.path, "/turf/open/space")
                    && !is_under(&t.path, "/turf/open/lava")
                    && !is_under(&t.path, "/turf/open/misc/asteroid")
                    && !is_under(&t.path, "/turf/open/openspace")
            });
            continue;
        }
        let path = world
            .get::<DmObj>(e)
            .map(|o| o.path.as_str())
            .or_else(|| world.get::<DmPower>(e).map(|p| p.path.as_str()));
        if path.is_some_and(|p| ELEVATED.iter().any(|r| is_under(p, r))) {
            return false;
        }
    }
    open
}

/// Every second: exposure, germs, the timers.
// Cost: one pass over the decomposing food (a few hundred on MetaStation) per second, a tile
// lookup each.
fn decay_tick(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs();
    {
        let mut clock = world.resource_mut::<DecayClock>();
        clock.0 += dt;
        if clock.0 < 1.0 {
            return;
        }
    }
    let step = std::mem::take(&mut world.resource_mut::<DecayClock>().0);
    let mut q = world.query_filtered::<Entity, With<Decay>>();
    let foods: Vec<Entity> = q.iter(world).collect();
    let mut rotten = Vec::new();
    for food in foods {
        let Some(mut d) = world.get::<Decay>(food).cloned() else {
            continue;
        };
        let before = d.clone();
        if let Some(at) = d.protected_at {
            let moved = world.get::<TilePos>(food) != Some(&at)
                || world.get::<crate::inventory::CarriedBy>(food).is_some();
            if moved {
                d.protected_at = None;
            }
        }
        if d.protected_at.is_none() && exposed(world, food) {
            if d.exposed_for < GERM_DELAY {
                d.exposed_for += step;
                // `expose_to_germs` -> `start_timer`: the first one only handles it.
                if d.exposed_for >= GERM_DELAY {
                    if d.handled {
                        d.running = true;
                    } else {
                        d.handled = true;
                    }
                }
            }
            if d.running {
                d.remaining -= step;
                if d.remaining <= 0.0 {
                    rotten.push(food);
                }
            }
        } else {
            d.exposed_for = 0.0;
            d.running = false;
        }
        if d != before {
            world.entity_mut(food).insert(d);
        }
    }
    for food in rotten {
        decompose(world, food);
    }
}

/// `decompose`.
pub fn decompose(world: &mut World, food: Entity) {
    let Some(dm) = world.get::<DmFood>(food).cloned() else {
        return;
    };
    let Some(&pos) = world.get::<TilePos>(food) else {
        return;
    };
    if dm.ant_attracting {
        crate::construction::spawn_obj(world, ANTS, pos);
    }
    if !dm.decomp_type.is_empty()
        && let Some(result) = crate::construction::spawn_obj(world, &dm.decomp_type, pos)
        && let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone())
        && let Some(mut from) = world.get::<Reagents>(food).map(|r| r.0.clone())
        && let Some(mut into) = world.get::<Reagents>(result).map(|r| r.0.clone())
    {
        into.clear();
        let all = from.total_volume;
        from.trans_to(&chem, &mut into, all, 1.0, None, false, false);
        world.entity_mut(result).insert(Reagents(into));
    }
    let name = name_of(world, food);
    let ants = if dm.ant_attracting { " and ants" } else { "" };
    visible(
        world,
        food,
        "",
        &format!("{name} gets overtaken by mold{ants}! Gross!"),
        Span::Warning,
    );
    world.despawn(food);
}

/// `decomposition/examine`.
pub fn examine_lines(world: &World, food: Entity, lines: &mut Vec<(Span, String)>) {
    let Some(d) = world.get::<Decay>(food) else {
        return;
    };
    let name = name_of(world, food);
    let left = d.remaining / d.original;
    if (0.5..0.75).contains(&left) {
        lines.push((Span::Notice, format!("{name} looks kinda stale.")));
    } else if (0.25..0.5).contains(&left) {
        lines.push((
            Span::Notice,
            format!("{name} is starting to look pretty gross."),
        ));
    } else if left < 0.25 {
        lines.push((Span::Danger, format!("{name} barely looks edible.")));
    }
}
