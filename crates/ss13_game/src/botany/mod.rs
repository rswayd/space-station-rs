//! Hydroponics (server): tg `code/modules/hydroponics/` with the data `ss13-mapload convert`
//! writes to `plants.ron` ([`ss13_mapload::plants`]).
//!
//! - Seed packets (`/obj/item/seeds`) and grown items carry a [`Seed`]: a packet its own, a
//!   grown item the seed it came from (map-placed and vended ones: their type's at potency 50).
//! - Trays (`/obj/machinery/hydroponics`, soil plots) carry a [`Tray`] and a nutrient holder
//!   (`maxnutri`); map-placed ones start full of water with half their nutrients as E-Z-Nutrient.
//!   Every `SSmachines` fire (2 s) a tray whose `cycledelay` (20 s) passed runs tg's cycle
//!   ([`tray::cycle`]): age, nutrients and their reagent effects, light, water, toxins, pests,
//!   weeds, pollination of the neighbours, instability mutations, death, harvestability.
//! - Clicks on a tray (out of combat checks, as tg's `item_interaction`): a seed is planted, a
//!   reagent container pours (water fills the tray, the rest goes into the nutrients), food is
//!   composted, a cultivator pulls the weeds, a shovel digs the plant out (5 s); an empty hand
//!   harvests a ready plant ([`plants::harvest`]) or removes a dead one. A plant analyzer prints
//!   tg's analyzer fields for a tray, a seed or produce. Produce into a seed extractor gives
//!   `rand(1, 4) + 1` seeds with its stats. Secateurs cut grafts, grafts go onto plants
//!   ([`grafts`]).
//! - Grown items act on their genes' product traits ([`produce`]): slipping, squashing,
//!   glowing, teleporting, stinging, batteries, harvest-time chemistry; invasive plants spread
//!   to the trays around.
//! - `SS13_HYDRO_SPEED` (dev): every hydroponics timer runs this many times faster (tests use
//!   [`HydroSpeed`]), so a cycle keeps tg's ratios.
// shortcut: no plant bags, seed storage in the extractor (its tgui), biogenerator, sprays,
// gene shears or right click nutrient emptying; the analyzer prints to chat instead of its
// tgui; invasive spreading ignores windows between trays (tg `Adjacent`).

pub mod genes;
pub mod grafts;
pub mod plants;
pub mod produce;
pub mod seeds;
pub mod tray;

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;
use ss13_mapload::plants::{
    GROWN_INEDIBLE_ROOT, GROWN_ROOT, PLANTS_FILE, PlantData, SEED_ROOT, SeedDef, TRAY_ROOT,
    TraitDef, TrayDef,
};
use ss13_mapload::{DmItem, DmObj, IconIndex, MapLoadPlugin};
use ss13_reagents::{Holder, Owner, flags, is_under};
use ss13_shared::chat::Span;
use ss13_shared::protocol::ClickModifiers;
use ss13_shared::{Appearance, EMISSIVE_PLANE, Optics, TilePos};
use ss13_world::SpatialIndex;

pub use seeds::Seed;
pub use tray::{PlantStatus, Tray};

use crate::chem::{Chem, Reagents};
use crate::combat::CombatRng;

/// `SSmachines` `wait`: trays check their cycle this often.
pub const MACHINES_WAIT: f64 = 2.0;
/// The equipment icon of the status lights.
const EQUIPMENT_DMI: &str = "icons/obj/service/hydroponics/equipment.dmi";
const EZ_NUTRIENT: &str = "/datum/reagent/plantnutriment/eznutriment";

/// `plants.ron` with lookups by type.
#[derive(Debug, Default)]
pub struct Plants {
    pub data: PlantData,
    seeds: HashMap<String, usize>,
    traits: HashMap<String, usize>,
    trays: HashMap<String, usize>,
    grown: HashMap<String, usize>,
}

impl Plants {
    pub fn new(data: PlantData) -> Self {
        let index = |paths: Vec<&String>| {
            paths
                .into_iter()
                .enumerate()
                .map(|(i, p)| (p.clone(), i))
                .collect()
        };
        Self {
            seeds: index(data.seeds.iter().map(|s| &s.path).collect()),
            traits: index(data.traits.iter().map(|t| &t.path).collect()),
            trays: index(data.trays.iter().map(|t| &t.path).collect()),
            grown: index(data.grown.iter().map(|g| &g.path).collect()),
            data,
        }
    }

    pub fn seed_def(&self, path: &str) -> Option<&SeedDef> {
        self.seeds.get(path).map(|&i| &self.data.seeds[i])
    }

    pub fn trait_def(&self, path: &str) -> Option<&TraitDef> {
        self.traits.get(path).map(|&i| &self.data.traits[i])
    }

    pub fn tray_def(&self, path: &str) -> Option<&TrayDef> {
        self.trays.get(path).map(|&i| &self.data.trays[i])
    }

    /// A grown item type's `seed` and `bite_consumption_mod`.
    pub fn grown(&self, path: &str) -> Option<(&str, f32)> {
        self.grown.get(path).map(|&i| {
            let g = &self.data.grown[i];
            (g.seed.as_str(), g.bite_consumption_mod)
        })
    }

    /// A grown item type's data.
    pub fn grown_def(&self, path: &str) -> Option<&ss13_mapload::plants::GrownDef> {
        self.grown.get(path).map(|&i| &self.data.grown[i])
    }

    /// `tox_damage` of a uranium reagent.
    pub fn tox_damage(&self, reagent: &str) -> f32 {
        self.data
            .tox_damage
            .iter()
            .find(|(p, _)| p == reagent)
            .map_or(0.0, |(_, t)| *t)
    }
}

/// The loaded plant data.
#[derive(Resource, Clone, Default)]
pub struct Botany(pub Arc<Plants>);

/// Hydroponics timers run this many times faster (dev and tests; 1 = tg).
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct HydroSpeed(pub f32);

impl Default for HydroSpeed {
    fn default() -> Self {
        Self(1.0)
    }
}

/// A timed hydroponics action ([`crate::construction::Job::Botany`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BotanyJob {
    /// A shovel digging the plant out of a tray.
    Dig,
    /// Squeezing a plant with liquid contents in hand (BandaStation).
    Squash,
}

pub struct BotanyServerPlugin;

impl Plugin for BotanyServerPlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<HydroSpeed>() {
            let speed = std::env::var("SS13_HYDRO_SPEED")
                .ok()
                .and_then(|v| v.parse::<f32>().ok())
                .filter(|v| *v > 0.0)
                .unwrap_or(1.0);
            app.insert_resource(HydroSpeed(speed));
        }
        app.add_observer(add_seed)
            .add_systems(Startup, setup_trays)
            .add_systems(Update, process_trays);
        produce::build(app);
    }

    /// After every plugin's `build`: the map plugin's assets dir is known by now.
    fn finish(&self, app: &mut App) {
        if app.world().contains_resource::<Botany>() {
            return; // inserted by a test
        }
        let path = app
            .world()
            .get_resource::<MapLoadPlugin>()
            .map(|m| m.assets.join(PLANTS_FILE));
        let data = match path.as_deref().map(PlantData::read) {
            Some(Ok(data)) => data,
            Some(Err(e)) => {
                warn!("{e:#} (rerun `ss13-mapload convert`); no hydroponics");
                PlantData::default()
            }
            None => PlantData::default(),
        };
        info!(
            "hydroponics: {} seeds, {} plant traits, {} tray types",
            data.seeds.len(),
            data.traits.len(),
            data.trays.len()
        );
        app.insert_resource(Botany(Arc::new(Plants::new(data))));
    }
}

/// `/obj/item/seeds/Initialize` and `/obj/item/food/grown/Initialize` of a map-placed or
/// spawned item: its type's seed (a grown item's at potency 50). Spawners that pass a seed
/// (harvests, the extractor) insert theirs over it.
fn add_seed(
    add: On<Add<DmItem>>,
    items: Query<&DmItem>,
    botany: Option<Res<Botany>>,
    rng: Option<ResMut<CombatRng>>,
    mut commands: Commands,
) {
    let (Ok(item), Some(botany)) = (items.get(add.entity), botany) else {
        return;
    };
    let plants = &botany.0;
    let seed = if is_under(&item.path, SEED_ROOT) {
        plants.seed_def(&item.path).map(|d| {
            let mut seed = Seed::new(d, plants);
            if is_under(&item.path, seeds::STRANGE_SEED)
                && let Some(mut rng) = rng
            {
                seed.randomize_strange(plants, &mut *rng);
            }
            seed
        })
    } else if is_under(&item.path, GROWN_ROOT) || is_under(&item.path, GROWN_INEDIBLE_ROOT) {
        plants
            .grown(&item.path)
            .and_then(|(seed, _)| plants.seed_def(seed))
            .map(|d| {
                let mut seed = Seed::new(d, plants);
                seed.adjust_potency(50.0 - seed.potency);
                seed
            })
    } else {
        None
    };
    if let Some(seed) = seed {
        commands.entity(add.entity).insert_if_new(seed);
    }
}

/// The trays on the map (`Initialize(mapload)`): full of water, half their nutrients (at least
/// 10) as E-Z-Nutrient.
fn setup_trays(world: &mut World) {
    let mut query = world.query::<(Entity, &DmObj)>();
    let trays: Vec<Entity> = query
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, TRAY_ROOT))
        .map(|(e, _)| e)
        .collect();
    for e in trays {
        setup_tray(world, e, true);
    }
}

/// A tray's state; one built from a frame (`crate::machine_frame`) starts dry and empty.
pub(crate) fn setup_tray(world: &mut World, e: Entity, mapload: bool) {
    let Some(botany) = world.get_resource::<Botany>() else {
        return;
    };
    let Some(def) = world
        .get::<DmObj>(e)
        .and_then(|o| botany.0.tray_def(&o.path))
        .cloned()
    else {
        return;
    };
    let chem = world.get_resource::<Chem>().map(|c| c.0.clone());
    let mut tray = Tray::new(&def);
    let mut holder = Holder::new(
        tray.maxnutri,
        flags::INJECTABLE,
        Owner {
            path: def.path.clone(),
            living: false,
        },
    );
    if mapload {
        tray.waterlevel = tray.maxwater;
        if let Some(chem) = &chem
            && let Some(id) = chem.id(EZ_NUTRIENT)
        {
            holder.add_reagent_list(chem, &[(id, (tray.maxnutri / 2.0).max(10.0))], None, true);
        }
    }
    world.entity_mut(e).insert((tray, Reagents(holder)));
    refresh_tray(world, e);
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

/// `SSmachines` firing every [`MACHINES_WAIT`]: trays whose cycle is due run it.
// Cost: one pass over the trays (a few dozen on a station) every 2 s; the light of a tray is
// summed from the sources within 8 tiles once per cycle (20 s).
fn process_trays(world: &mut World, mut next: Local<f64>) {
    let t = now(world);
    if t < *next {
        return;
    }
    let speed = f64::from(world.get_resource::<HydroSpeed>().map_or(1.0, |s| s.0));
    *next = t + MACHINES_WAIT / speed;
    let delay = tray::HYDROTRAY_CYCLE_DELAY / speed;
    let mut query = world.query::<(Entity, &Tray)>();
    let due: Vec<Entity> = query
        .iter(world)
        .filter(|(_, tray)| t > tray.lastcycle + delay)
        .map(|(e, _)| e)
        .collect();
    for e in due {
        run_cycle(world, e, t);
    }
}

fn run_cycle(world: &mut World, e: Entity, t: f64) {
    let (Some(botany), Some(chem)) = (
        world.get_resource::<Botany>().cloned(),
        world.get_resource::<Chem>().map(|c| c.0.clone()),
    ) else {
        return;
    };
    let Some(pos) = world.get::<TilePos>(e).copied() else {
        return;
    };
    let (Some(mut state), Some(mut nutrients)) = (
        world.get::<Tray>(e).cloned(),
        world.get::<Reagents>(e).map(|r| r.0.clone()),
    ) else {
        return;
    };
    let env = tray::CycleEnv {
        chem: &chem,
        plants: &botany.0,
        lumcount: lumcount(world, pos),
    };
    let out = world.resource_scope(|_, mut rng: Mut<CombatRng>| {
        tray::cycle(&mut state, &mut nutrients, &env, &mut *rng)
    });
    state.lastcycle = t;
    if out.pollinate {
        pollinate(world, &mut state, pos);
    }
    world.entity_mut(e).insert(state);
    // `COMSIG_SEED_ON_GROW`.
    if world
        .get::<Tray>(e)
        .and_then(|s| s.myseed.as_ref())
        .is_some_and(|s| s.has_gene(produce::INVASIVE))
    {
        try_spread(world, e, pos, t);
    }
    if let Some(mut r) = world.get_mut::<Reagents>(e)
        && r.0 != nutrients
    {
        r.0 = nutrients;
    }
    for message in &out.messages {
        crate::chem::visible(world, e, "", message, Span::Warning);
    }
    refresh_tray(world, e);
}

/// `/datum/plant_gene/trait/invasive/try_spread`: `5 × (11 - production)`% of the cycles, each
/// tray around has a 15% chance to be overtaken by a copy (not one of its own living kind).
// shortcut: windows between trays do not stop it (tg `Adjacent`).
fn try_spread(world: &mut World, origin: Entity, pos: TilePos, t: f64) {
    let Some(source) = world.get::<Tray>(origin).and_then(|s| s.myseed.clone()) else {
        return;
    };
    let plants = world.resource::<Botany>().0.clone();
    if roll_prob(world, 100.0 - 5.0 * (11.0 - source.production)) {
        return;
    }
    let neighbours: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(pos, 1)
        .filter(|&n| n != origin && world.get::<TilePos>(n) != Some(&pos))
        .filter(|&n| world.get::<Tray>(n).is_some())
        .collect();
    for n in neighbours {
        if !roll_prob(world, 15.0) {
            continue;
        }
        let Some(mut target) = world.get::<Tray>(n).cloned() else {
            continue;
        };
        if let Some(theirs) = &target.myseed {
            if theirs.path == source.path && target.plant_status != PlantStatus::Dead {
                continue; // it should not destroy its own kind
            }
            let text = format!(
                "The {} is overtaken by {}!",
                theirs.plantname, source.plantname
            );
            crate::chem::visible(world, n, "", &text, Span::Warning);
        }
        target.set_seed(Some(source.copy(&plants)));
        target.set_plant_health(source.endurance, false);
        target.lastcycle = t;
        target.weedlevel = 0.0;
        target.pestlevel = 0.0;
        world.entity_mut(n).insert(target);
        let text = format!("The {} spreads!", source.plantname);
        crate::chem::visible(world, n, "", &text, Span::Warning);
        refresh_tray(world, n);
    }
}

fn roll_prob(world: &mut World, percent: f32) -> bool {
    use seeds::Dice;
    world.resource_mut::<CombatRng>().prob(percent)
}

/// `pollinate(range = 1)`: every tray around with a living plant gets averaged towards this one.
// shortcut: windows between trays do not block it (tg `Adjacent`).
fn pollinate(world: &mut World, state: &mut Tray, pos: TilePos) {
    state.being_pollinated = false;
    if state.myseed.is_none() || state.plant_status == PlantStatus::Dead {
        return;
    }
    let neighbours: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(pos, 1)
        .filter(|&n| world.get::<TilePos>(n) != Some(&pos))
        .filter(|&n| {
            world
                .get::<Tray>(n)
                .is_some_and(|t| t.myseed.is_some() && t.plant_status != PlantStatus::Dead)
        })
        .collect();
    for n in neighbours {
        let Some(mut other) = world.get::<Tray>(n).cloned() else {
            continue;
        };
        let (Some(source), Some(target)) = (state.myseed.as_mut(), other.myseed.as_mut()) else {
            continue;
        };
        world.resource_scope(|_, mut rng: Mut<CombatRng>| {
            tray::pollinate_pair(source, target, &mut *rng);
        });
        state.being_pollinated = true;
        world.entity_mut(n).insert(other);
    }
}

/// `/turf/proc/get_static_lumcount()` of `pos`: the mean of the four corners' red, green and
/// blue light from the sources that see the tile. `None`: the tile has no lighting object
/// (space, areas without static lighting), which tg never counts as dark.
// shortcut: a source sees the tile when the straight line between them has no opaque tile
// (tg's `view()` per corner); dynamic overlay lights count as static ones.
pub fn lumcount(world: &World, pos: TilePos) -> Option<f32> {
    let index = world.resource::<SpatialIndex>();
    // A turf without `Optics` is lit by sources only (`Optics::tile == None`).
    let turf = index
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| world.get::<ss13_mapload::Turf>(e).is_some())?;
    if matches!(
        world.get::<Optics>(turf).and_then(|o| o.tile),
        Some(ss13_shared::TileLighting::Space)
            | Some(ss13_shared::TileLighting::Area { dynamic: false, .. })
    ) {
        return None;
    }
    let opaque = |p: TilePos| {
        index
            .on_tile(p)
            .iter()
            .any(|&e| world.get::<Optics>(e).is_some_and(|o| o.opaque))
    };
    let mut total = 0.0;
    for e in index.in_radius(pos, 8) {
        let (Some(light), Some(&src)) = (
            world.get::<Optics>(e).and_then(|o| o.light.as_ref()),
            world.get::<TilePos>(e),
        ) else {
            continue;
        };
        if !line_clear(src, pos, &opaque) {
            continue;
        }
        let (ox, oy) = (
            f32::from(light.offset[0]) / 32.0,
            f32::from(light.offset[1]) / 32.0,
        );
        let colour = light.color.iter().map(|&c| f32::from(c)).sum::<f32>() / (3.0 * 255.0);
        for (cx, cy) in [(0.5, 0.5), (0.5, -0.5), (-0.5, -0.5), (-0.5, 0.5)] {
            let x = (pos.x - src.x) as f32 + cx - ox;
            let y = (pos.y - src.y) as f32 + cy - oy;
            total += light.power * falloff(light, x, y) * colour / 4.0;
        }
    }
    Some(total)
}

/// tg `falloff_at_coord` (the same as `ss13_render::lighting::math::falloff`, which the server
/// does not link).
fn falloff(light: &ss13_shared::LightSource, x: f32, y: f32) -> f32 {
    let range = light.range.max(1.0);
    let d = (x * x + y * y + light.height).max(0.0).sqrt();
    let multiplier = 1.0 - (d / range).clamp(0.0, 1.0);
    let angle = f32::from(light.angle);
    if angle >= 360.0 || angle <= 0.0 {
        return multiplier;
    }
    let dir_angle = match light.dir {
        ss13_shared::Dir::North => 0.0,
        ss13_shared::Dir::NorthEast => 45.0,
        ss13_shared::Dir::East => 90.0,
        ss13_shared::Dir::SouthEast => 135.0,
        ss13_shared::Dir::South => 180.0,
        ss13_shared::Dir::SouthWest => 225.0,
        ss13_shared::Dir::West => 270.0,
        ss13_shared::Dir::NorthWest => 315.0,
    };
    let delta_angle = if y == 0.0 {
        if x >= 0.0 { 90.0 } else { 270.0 }
    } else {
        let a = (x / y).atan().to_degrees();
        if y < 0.0 {
            a + 180.0
        } else if x < 0.0 {
            a + 360.0
        } else {
            a
        }
    };
    let mut delta: f32 = (dir_angle - delta_angle).abs();
    if delta > 180.0 {
        delta = 360.0 - delta;
    }
    (multiplier * (1.0 - (delta - angle / 2.0).max(0.0) / 30.0)).max(0.0)
}

/// No opaque tile strictly between `a` and `b` (Bresenham).
fn line_clear(a: TilePos, b: TilePos, opaque: &impl Fn(TilePos) -> bool) -> bool {
    let (dx, dy) = ((b.x - a.x).abs(), -(b.y - a.y).abs());
    let (sx, sy) = ((b.x - a.x).signum(), (b.y - a.y).signum());
    let (mut x, mut y, mut err) = (a.x, a.y, dx + dy);
    loop {
        if (x, y) == (b.x, b.y) {
            return true;
        }
        if (x, y) != (a.x, a.y) && opaque(TilePos::new(x, y, a.z)) {
            return false;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// tg `hydroponics/constructable/RefreshParts` (no soils here): `maxwater = bins * 50`,
/// `maxnutri = bins * 5 + STATIC_NUTRIENT_CAPACITY`, `rating` the servo tier, `nutridrain =
/// 1 / rating`.
pub(crate) fn refresh_parts(world: &mut World, e: Entity) {
    let constructable = world
        .get::<DmObj>(e)
        .is_some_and(|o| is_under(&o.path, ss13_mapload::plants::CONSTRUCTABLE_TRAY));
    if !constructable || world.get::<Tray>(e).is_none() {
        return;
    }
    let r = crate::stock_parts::ratings(world, e);
    let bins = r.sum(crate::stock_parts::PartKind::MatterBin) as f32;
    let servo = r.last(crate::stock_parts::PartKind::Servo);
    let Some(mut tray) = world.get_mut::<Tray>(e) else {
        return;
    };
    tray.maxwater = bins * 50.0;
    tray.maxnutri = bins * 5.0 + 10.0;
    if let Some(servo) = servo {
        tray.rating = servo as f32;
    }
    tray.nutridrain = 1.0 / tray.rating.max(1.0);
    let maxnutri = tray.maxnutri;
    if let Some(mut reagents) = world.get_mut::<Reagents>(e) {
        reagents.0.maximum_volume = maxnutri;
    }
}

/// tg `update_overlays()`: the plant (`get_tray_overlay`) and the status lights with their
/// emissive copies.
pub fn refresh_tray(world: &mut World, e: Entity) {
    let (Some(state), Some(botany)) = (
        world.get::<Tray>(e).cloned(),
        world.get_resource::<Botany>().cloned(),
    ) else {
        return;
    };
    let nutrients = world.get::<Reagents>(e).map_or(0.0, |r| r.0.total_volume);
    let Some(icons) = world.get_resource::<IconIndex>() else {
        return;
    };
    let mut overlays = Vec::new();
    if let Some(seed) = &state.myseed
        && let Some(def) = botany.0.seed_def(&seed.path)
    {
        let icon_state = tray::plant_icon_state(def, state.age, state.plant_status);
        if let Some(id) = icons.get(&def.growing_icon, &icon_state) {
            overlays.push(ss13_shared::Overlay {
                pixel_offset: [0, def.plant_icon_offset + state.plant_offset_y],
                ..ss13_shared::Overlay::new(id)
            });
        }
    }
    for light in tray::status_lights(&state, nutrients) {
        if let Some(id) = icons.get(EQUIPMENT_DMI, &light) {
            overlays.push(ss13_shared::Overlay::new(id));
            overlays.push(ss13_shared::Overlay {
                plane: Some(EMISSIVE_PLANE),
                ..ss13_shared::Overlay::new(id)
            });
        }
    }
    if let Some(mut appearance) = world.get_mut::<Appearance>(e)
        && appearance.overlays != overlays
    {
        appearance.overlays = overlays;
    }
}

/// tg `item_interaction` of hydroponics with `held` on `target`: trays, the seed extractor and
/// the plant analyzer. True when the click was theirs.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    modifiers: ClickModifiers,
) -> bool {
    if !world.contains_resource::<Botany>() {
        return false;
    }
    let held_path = world
        .get::<DmItem>(held)
        .map(|i| i.path.clone())
        .unwrap_or_default();
    if is_under(&held_path, plants::ANALYZER_ROOT) {
        return plants::analyze(world, client, target);
    }
    if produce::battery_interaction(world, client, mob, held, target) {
        return true;
    }
    if world.get::<Tray>(target).is_some() {
        return plants::tray_item_interaction(world, client, mob, held, target, modifiers);
    }
    if world
        .get::<DmObj>(target)
        .is_some_and(|o| is_under(&o.path, plants::EXTRACTOR_ROOT))
    {
        return plants::extractor_interaction(world, client, mob, held, target);
    }
    false
}

/// An empty-hand click on a tray (`attack_hand`): harvest, remove the dead plant, or examine
/// it. True when the click was a tray's.
pub fn interact(
    world: &mut World,
    client: Entity,
    mob: Entity,
    target: Entity,
    right: bool,
) -> bool {
    if right || world.get::<Tray>(target).is_none() {
        return false;
    }
    plants::attack_hand(world, client, mob, target);
    true
}

/// tg `attack_self` of a held plant (squeezing liquid contents). False for other items.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    world.contains_resource::<Botany>() && produce::attack_self(world, client, mob, item)
}

/// The end of a [`BotanyJob`].
pub fn complete(world: &mut World, mob: Entity, action: &crate::tools::DoAfter, job: BotanyJob) {
    match job {
        BotanyJob::Dig => plants::complete_dig(world, mob, action),
        BotanyJob::Squash => produce::complete_squash(world, mob, action),
    }
}

/// tg `/obj/machinery/hydroponics/examine`.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(Span, String)>) {
    produce::examine_lines(world, target, lines);
    let Some(state) = world.get::<Tray>(target) else {
        return;
    };
    let nutrients = world
        .get::<Reagents>(target)
        .map_or(0.0, |r| r.0.total_volume);
    match &state.myseed {
        Some(seed) => {
            lines.push((Span::Info, format!("It has {} planted.", seed.plantname)));
            match state.plant_status {
                PlantStatus::Dead => lines.push((Span::Warning, "It's dead!".to_owned())),
                PlantStatus::Harvestable => {
                    lines.push((Span::Info, "It's ready to harvest.".to_owned()));
                }
                _ if state.plant_health <= seed.endurance / 2.0 => {
                    lines.push((Span::Warning, "It looks unhealthy.".to_owned()));
                }
                _ => {}
            }
        }
        None => lines.push((Span::Info, "It's empty.".to_owned())),
    }
    let n = crate::chem::dm_num;
    lines.push((
        Span::Info,
        format!("Water: {}/{}.", n(state.waterlevel), n(state.maxwater)),
    ));
    lines.push((
        Span::Info,
        format!("Nutrient: {}/{}.", n(nutrients), n(state.maxnutri)),
    ));
    if state.weedlevel >= 5.0 {
        lines.push((Span::Warning, "It's filled with weeds!".to_owned()));
    }
    if state.pestlevel >= 5.0 {
        lines.push((Span::Warning, "It's filled with tiny worms!".to_owned()));
    }
}

/// Dice that return scripted values (`rand` the next number clamped into its range, else 0
/// clamped; `prob` the next flag, else false), for exact checks of tg's formulas.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct Script {
    pub rolls: Vec<i32>,
    pub probs: Vec<bool>,
}

#[cfg(test)]
impl seeds::Dice for Script {
    fn rand(&mut self, low: i32, high: i32) -> i32 {
        let v = if self.rolls.is_empty() {
            0
        } else {
            self.rolls.remove(0)
        };
        v.clamp(low.min(high), high.max(low))
    }

    fn prob(&mut self, _: f32) -> bool {
        !self.probs.is_empty() && self.probs.remove(0)
    }
}

#[cfg(test)]
pub(crate) fn test_plants() -> Plants {
    use ss13_mapload::plants::GrownDef;
    let tr = |path: &str, mutability: u32, ids: u32, flags: u32| TraitDef {
        path: path.into(),
        name: path.rsplit('/').next().unwrap_or_default().into(),
        rate: 0.05,
        mutability_flags: mutability,
        trait_ids: ids,
        trait_flags: flags,
        ..Default::default()
    };
    let tomato = SeedDef {
        path: "/obj/item/seeds/tomato".into(),
        name: "tomato seed pack".into(),
        plantname: "Tomato Plants".into(),
        product: "/obj/item/food/grown/tomato".into(),
        growing_icon: "icons/obj/service/hydroponics/growing_fruits.dmi".into(),
        icon_grow: "tomato-grow".into(),
        icon_dead: "tomato-dead".into(),
        icon_harvest: "tomato-harvest".into(),
        plant_icon_offset: 8,
        lifespan: 25.0,
        endurance: 15.0,
        maturation: 8.0,
        production: 6.0,
        yield_: 3.0,
        potency: 10.0,
        growthstages: 6,
        instability: 25.0,
        weed_rate: 1.0,
        weed_chance: 5.0,
        seed_flags: 1,
        genes: vec![genes::REPEATED_HARVEST.into()],
        reagents: vec![
            ("/datum/reagent/consumable/nutriment/vitamin".into(), 0.04),
            ("/datum/reagent/consumable/nutriment".into(), 0.1),
        ],
        mutatelist: vec!["/obj/item/seeds/tomato/blue".into()],
        ..Default::default()
    };
    let blue = SeedDef {
        path: "/obj/item/seeds/tomato/blue".into(),
        plantname: "Blue-Tomato Plants".into(),
        product: "/obj/item/food/grown/tomato/blue".into(),
        icon_grow: "bluetomato-grow".into(),
        mutatelist: vec![],
        ..tomato.clone()
    };
    let starthistle = SeedDef {
        path: "/obj/item/seeds/starthistle".into(),
        plantname: "starthistle".into(),
        product: String::new(),
        yield_: 2.0,
        genes: vec![genes::WEED_HARDY.into()],
        reagents: vec![],
        mutatelist: vec![],
        ..tomato.clone()
    };
    Plants::new(PlantData {
        seeds: vec![tomato, blue, starthistle],
        traits: vec![
            tr("/datum/plant_gene/trait/carnivory", 7, 0, 0),
            tr(genes::FUNGAL, 4, 2, 0),
            tr(genes::WEED_HARDY, 4, 2, 0),
            tr(genes::MAXCHEM, 7, 0, genes::TRAIT_HALVES_YIELD),
            tr(genes::REPEATED_HARVEST, 7, 0, 0),
        ],
        trays: vec![TrayDef {
            path: TRAY_ROOT.into(),
            maxwater: 100.0,
            maxnutri: 20.0,
            rating: 1.0,
            tray_flags: 1,
            status_lights: true,
            ..Default::default()
        }],
        grown: vec![GrownDef {
            path: "/obj/item/food/grown/tomato".into(),
            seed: "/obj/item/seeds/tomato".into(),
            bite_consumption_mod: 1.0,
            ..Default::default()
        }],
        random_reagents: vec!["/datum/reagent/consumable/sugar".into()],
        tox_damage: vec![],
    })
}
