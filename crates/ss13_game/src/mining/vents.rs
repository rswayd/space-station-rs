//! Ore vents (server), after tg `/obj/structure/ore_vent`
//! (`code/game/objects/structures/lavaland/vent_mining/ore_vent.dm`), `SSore_generation`
//! (`code/controllers/subsystem/ore_generation.dm`) and the wave behaviour of
//! `/datum/component/spawner`.
//!
//! - Round start (`SSore_generation/Initialize`): the mining level's vents are sorted by their
//!   distance to the mining epicenter landmark; the closest third are small, the next medium,
//!   the rest large (boulder size 5/10/15, 5/10/15 waves). Each then takes 4 distinct minerals
//!   from the lavaland pool (`ore_vent_minerals_lavaland` expanded by weight, each pick taken
//!   out of it) with weights `rand(1, 4)`. Without a landmark a vent rolls its size and draws
//!   from the pool by weight. The starter vent is tapped from the start with iron and glass.
//! - A mining scanner on a vent (`scan_and_confirm`): the first scan (4 s) discovers it, its
//!   minerals join its description, it gets a GPS signal and the scanner's ID account 100 mining
//!   points. The next scan asks to start the wave defense (`tgui_alert`); a yes clears the rock
//!   around it (`pre_wave_defense`), lands a NODE drone on it (300 + 100 per size step health)
//!   and starts the waves: 7.5 s after the last wave is dead the next comes, two kinds of the
//!   vent's defenders, up to `boulder_size / 5` of each, on a free tile 4 away (`turf_peel`);
//!   with none free they come out of the vent itself, which blows a light blast
//!   (`anti_cheese`).
//! - The drone dead or gone ends it: the vent closes up and can be tried again; the drone moved
//!   off the vent too. Surviving every wave taps the vent: everyone within 7 tiles with an ID
//!   gets `100 * (size + 2) - 100` mining points, the drone flies away.
//! - Every 60 s (`SSore_generation/fire`) each tapped vent with fewer than 10 boulders on it makes
//!   one (`produce_boulder`): 7% an artifact boulder, 3 rolls of its minerals by weight, each
//!   `50 * max(round(size / log(rand(1 + i, 4 + i))), 1)` units; the boulders lying on tapped
//!   vents are what the BRM pulls ([`OreGeneration::available`]).
// shortcut: no boss or gulag vents, no golem/goldgrub boulder pulling (`TRAIT_BOULDER_BREAKER`),
// no Candela network boost (`boosted`), no lava platforms while clearing, no mining experience
// for the wave's miners, no `heads_up` on spawned mobs, the clearing and the drone's flight are
// instant (no `sleep`, `animate`, `Shake`, smoke particles or mineral overlays), the spawn tile
// ignores line of sight; add each with its mechanic.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmMineral, DmName, DmObj};
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, Span};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind};
use ss13_world::SpatialIndex;

use super::boulders::{Boulder, new_boulder};
use super::rocks::MiningJob;
use crate::combat::CombatRng;
use crate::construction::{Job, path_of, turf_at};
use crate::health::{DEFAULT_MESSAGE_RANGE, Health, Stat};
use crate::inventory::Inventory;
use crate::sound::{PlaySound, playsound};
use crate::tools::{Actor, DoAfter, do_after, now};

pub const ORE_VENT: &str = "/obj/structure/ore_vent";
const STARTER: &str = "/obj/structure/ore_vent/starter_resources";
const MINING_CENTER: &str = "/obj/effect/landmark/mining_center";
const NODE_DRONE: &str = crate::mobs::lavaland::minebot::NODE_DRONE;
const BOULDER: &str = "/obj/item/boulder";
const ARTIFACTS: [&str; 3] = [
    "/obj/item/boulder/artifact",
    "/obj/item/boulder/artifact/bluespace",
    "/obj/item/boulder/artifact/diamond",
];
const RUBBLE: &str = "/obj/effect/decal/cleanable/rubble";
/// `/obj/effect/temp_visual/dust_cloud`: the spawner's windup `effect`.
const DUST_CLOUD: crate::effects::temp_visual::Visual = crate::effects::temp_visual::Visual {
    offset: [-4, -4],
    ..crate::effects::temp_visual::Visual::new("icons/effects/effects.dmi", "light_dust_cloud", 1.0)
};
const ASTEROID: &str = "/turf/open/misc/asteroid";

/// `BOULDER_SIZE_*`.
pub const SIZE_SMALL: u32 = 5;
pub const SIZE_MEDIUM: u32 = 10;
pub const SIZE_LARGE: u32 = 15;
/// `ARTIFACT_ROLL_CHANCE`.
const ARTIFACT_CHANCE: f32 = 7.0;
/// `MINERAL_TYPE_OPTIONS_RANDOM`.
const MINERALS_PER_VENT: usize = 4;
/// `MINERALS_PER_BOULDER`.
const MINERALS_PER_BOULDER: u32 = 3;
/// `MAX_MINERAL_PICK_ATTEMPTS`.
const MAX_PICK_ATTEMPTS: u32 = 10;
/// `HALF_SHEET_MATERIAL_AMOUNT`.
const HALF_SHEET: f64 = 50.0;
/// `MAX_BOULDERS_PER_VENT`.
const MAX_BOULDERS_PER_VENT: usize = 10;
/// `SSore_generation` `wait`.
const FIRE_SECS: f64 = 60.0;
/// `MINER_POINT_MULTIPLIER`.
pub const MINER_POINT_MULTIPLIER: i64 = 100;
/// The scan's `do_after`, deciseconds.
const SCAN_DS: f32 = 40.0;
/// `manual_vent_cooldown`.
const MANUAL_COOLDOWN: f64 = 10.0;
/// The spawner component's `spawn_time` and `spawn_windup`.
const WAVE_SPAWN_TIME: f64 = 7.5;
const SPAWN_WINDUP: f64 = 0.9;
/// `spawn_distance`, `spawn_distance_exclude`.
const SPAWN_DISTANCE: i32 = 4;
const SPAWN_EXCLUDE: i32 = 3;
/// `max_spawned`, `max_spawn_types_per_attempt`.
const MAX_SPAWNED: usize = 10;
const SPAWN_TYPES_PER_WAVE: usize = 2;
/// `range(7, src)` of the payout.
const PAYOUT_RANGE: u32 = 7;
/// `pre_escape`'s flick, then `escape`'s flight.
const ESCAPE_FLICK: f64 = 1.9;
const ESCAPE_FLIGHT: f64 = 2.0;
/// `pre_wave_defense`: rings cleared around the vent.
const CLEAR_RINGS: i32 = 5;

/// `GLOB.ore_vent_minerals_lavaland`.
pub const LAVALAND_MINERALS: [(&str, u32); 8] = [
    ("/datum/material/iron", 14),
    ("/datum/material/glass", 13),
    ("/datum/material/plasma", 10),
    ("/datum/material/silver", 7),
    ("/datum/material/titanium", 6),
    ("/datum/material/gold", 5),
    ("/datum/material/uranium", 4),
    ("/datum/material/plastic", 1),
];

/// `/obj/structure/ore_vent` `defending_mobs` (lavaland).
const LAVALAND_DEFENDERS: [&str; 6] = [
    "/mob/living/basic/mining/goliath",
    "/mob/living/basic/mining/legion/spawner_made",
    "/mob/living/basic/mining/watcher",
    "/mob/living/basic/mining/lobstrosity/lava",
    "/mob/living/basic/mining/brimdemon",
    "/mob/living/basic/mining/bileworm",
];

/// `/obj/structure/ore_vent/random/icebox` and `icebox/lower` `defending_mobs`.
const ICEBOX_DEFENDERS: [&str; 4] = [
    "/mob/living/basic/mining/lobstrosity",
    "/mob/living/basic/mining/legion/snow/spawner_made",
    "/mob/living/basic/mining/wolf",
    "/mob/living/basic/mining/polarbear",
];
const ICEBOX_LOWER_DEFENDERS: [&str; 6] = [
    "/mob/living/basic/mining/ice_whelp",
    "/mob/living/basic/mining/lobstrosity",
    "/mob/living/basic/mining/legion/snow/spawner_made",
    "/mob/living/basic/mining/ice_demon",
    "/mob/living/basic/mining/wolf",
    "/mob/living/basic/mining/polarbear",
];

/// A vent's live state.
#[derive(Component, Clone, Debug)]
pub struct OreVent {
    pub tapped: bool,
    pub discovered: bool,
    /// `unique_vent`: outside the mineral distribution (the starter vent).
    pub unique: bool,
    /// `mineral_breakdown`: material type and weight.
    pub minerals: Vec<(String, u32)>,
    pub boulder_size: u32,
    /// `wave_timer`: the waves of its defense.
    pub waves: u32,
    /// `ore_string`.
    pub ore_string: String,
    /// `boulder_icon_state`.
    pub boulder_string: &'static str,
    pub artifact_chance: f32,
    pub defenders: &'static [&'static str],
    /// `node`: the NODE drone of a defense under way.
    pub node: Option<Entity>,
    /// `wave_cooldown` running: a defense under way.
    pub defending: bool,
    /// `manual_vent_cooldown` ends.
    pub manual_ready: f64,
}

impl OreVent {
    fn of(path: &str, boulder_string: &'static str) -> Self {
        let defenders: &'static [&'static str] =
            if is_under(path, "/obj/structure/ore_vent/random/icebox/lower") {
                &ICEBOX_LOWER_DEFENDERS
            } else if is_under(path, "/obj/structure/ore_vent/random/icebox") {
                &ICEBOX_DEFENDERS
            } else {
                &LAVALAND_DEFENDERS
            };
        let starter = is_under(path, STARTER);
        Self {
            tapped: starter,
            discovered: starter,
            unique: starter,
            minerals: if starter {
                vec![
                    ("/datum/material/iron".into(), 1),
                    ("/datum/material/glass".into(), 1),
                ]
            } else {
                Vec::new()
            },
            boulder_size: SIZE_SMALL,
            waves: 5,
            ore_string: String::new(),
            boulder_string,
            artifact_chance: ARTIFACT_CHANCE,
            defenders,
            node: None,
            defending: false,
            manual_ready: 0.0,
        }
    }

    /// `vent_size_setup` with a size: `boulder_size`, `wave_timer`.
    fn set_size(&mut self, size: VentSize) {
        (self.boulder_size, self.waves) = match size {
            VentSize::Small => (SIZE_SMALL, 5),
            VentSize::Medium => (SIZE_MEDIUM, 10),
            VentSize::Large => (SIZE_LARGE, 15),
        };
    }
}

/// `SMALL_VENT_TYPE`, `MEDIUM_VENT_TYPE`, `LARGE_VENT_TYPE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VentSize {
    Small,
    Medium,
    Large,
}

impl VentSize {
    fn name(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Medium => "medium",
            Self::Large => "large",
        }
    }
}

/// `ore_vent_options` of `/obj/structure/ore_vent`: large 3, medium 5, small 7.
const SIZE_WEIGHTS: [(VentSize, u32); 3] = [
    (VentSize::Large, 3),
    (VentSize::Medium, 5),
    (VentSize::Small, 7),
];

/// `SSore_generation/Initialize`: the sizes of vents sorted closest first, a third each.
pub fn sizes_by_distance(count: usize) -> Vec<VentSize> {
    let third = (count / 3) as i64;
    let mut cutoff = third;
    let mut level = VentSize::Small;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        out.push(level);
        cutoff -= 1;
        if cutoff > 0 || level == VentSize::Large {
            continue;
        }
        cutoff = third;
        level = match level {
            VentSize::Small => VentSize::Medium,
            _ => VentSize::Large,
        };
    }
    out
}

/// tg `expand_weights` of [`LAVALAND_MINERALS`] (their greatest common factor is 1).
pub fn mineral_pool() -> Vec<String> {
    LAVALAND_MINERALS
        .iter()
        .flat_map(|&(m, w)| std::iter::repeat_n(m.to_owned(), w as usize))
        .collect()
}

fn pick_weight<'a, T>(rng: &mut CombatRng, items: &'a [(T, u32)]) -> Option<&'a T> {
    let total: u32 = items.iter().map(|(_, w)| *w).sum();
    if total == 0 {
        return None;
    }
    let mut roll = rng.range(1, total);
    for (item, w) in items {
        if roll <= *w {
            return Some(item);
        }
        roll -= w;
    }
    None
}

/// `generate_mineral_breakdown(MINERAL_TYPE_OPTIONS_RANDOM, map_loading)`: with `pool` (the round
/// start distribution) each pick is taken out of it; without, the lavaland weights.
pub fn generate_minerals(
    rng: &mut CombatRng,
    mut pool: Option<&mut Vec<String>>,
) -> Vec<(String, u32)> {
    let mut picked: Vec<String> = Vec::new();
    for _ in 0..MINERALS_PER_VENT {
        let mut mineral = None;
        for _ in 0..MAX_PICK_ATTEMPTS {
            mineral = match pool.as_deref() {
                Some(pool) if !pool.is_empty() => {
                    Some(pool[rng.range(0, pool.len() as u32 - 1) as usize].clone())
                }
                Some(_) => None,
                None => {
                    let weights: Vec<(&str, u32)> = LAVALAND_MINERALS.to_vec();
                    pick_weight(rng, &weights).map(|m| (*m).to_owned())
                }
            };
            if mineral.as_ref().is_none_or(|m| !picked.contains(m)) {
                break;
            }
        }
        let Some(mineral) = mineral else {
            break;
        };
        if let Some(pool) = pool.as_deref_mut()
            && let Some(i) = pool.iter().position(|m| *m == mineral)
        {
            pool.remove(i);
        }
        if !picked.contains(&mineral) {
            picked.push(mineral);
        }
    }
    picked
        .into_iter()
        .map(|m| (m, rng.range(1, MINERALS_PER_VENT as u32)))
        .collect()
}

/// `ore_quantity_function(ore_floor)`: material units of the `ore_floor`th roll of a boulder.
pub fn ore_quantity(rng: &mut CombatRng, boulder_size: u32, ore_floor: u32) -> f64 {
    let roll = f64::from(rng.range(1 + ore_floor, 4 + ore_floor));
    // DM `log(x)` is the natural logarithm; `round(x)` with one argument floors.
    let steps = (f64::from(boulder_size) / roll.ln()).floor().max(1.0);
    HALF_SHEET * steps
}

/// `SSore_generation`: the boulders the BRMs pull, refreshed every [`FIRE_SECS`].
#[derive(Resource, Default)]
pub struct OreGeneration {
    pub available: Vec<Entity>,
    next_fire: f64,
}

/// A wave defense under way on its vent (`/datum/component/spawner`, `SPAWN_BY_WAVE_BEHAVIOR`).
#[derive(Component, Clone, Debug, Default)]
pub struct VentWave {
    pub completed: u32,
    pub max: u32,
    /// `spawn_delay` ends.
    pub next_spawn: f64,
    /// `spawned_things`.
    pub spawned: Vec<Entity>,
    /// The windup of a spawn: due time, tile, mob type.
    pending: Vec<(f64, TilePos, &'static str)>,
}

/// A NODE drone leaving (`pre_escape` then `escape`): the message at the first time, gone at
/// the second.
#[derive(Component, Clone, Copy, Debug)]
pub struct NodeEscaping {
    pub message_at: f64,
    pub gone_at: f64,
    pub success: bool,
    told: bool,
}

/// The `tgui_alert` of a second scan.
#[derive(Component, Clone, Copy, Debug)]
struct ExcavationPrompt {
    vent: Entity,
}

pub(super) struct VentsPlugin;

impl Plugin for VentsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OreGeneration>()
            .add_systems(ss13_world::MapInit, setup_vents)
            .add_systems(
                Update,
                (
                    ore_generation_fire,
                    waves.run_if(any_with_component::<VentWave>),
                    node_escapes.run_if(any_with_component::<NodeEscaping>),
                    excavation_answers
                        .in_set(ss13_tgui::server::UiSystems::Handle)
                        .after(ss13_tgui::prompt::PromptSystems::Answer),
                ),
            );
    }
}

fn name_of_material(world: &World, material: &str) -> String {
    world
        .get_resource::<crate::science::techweb::ResearchDb>()
        .and_then(|db| {
            let i = *db.material_index.get(material)?;
            Some(crate::science::lathe::plain(&db.materials[i].name).to_owned())
        })
        .unwrap_or_else(|| material.rsplit('/').next().unwrap_or(material).to_owned())
}

/// tg `english_list`.
fn english_list(items: &[String]) -> String {
    match items {
        [] => "nothing".to_owned(),
        [one] => one.clone(),
        [a, b] => format!("{a} and {b}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

/// `generate_description(user)`.
fn generate_description(world: &mut World, vent: Entity, discoverer: Option<String>) {
    let Some(minerals) = world.get::<OreVent>(vent).map(|v| v.minerals.clone()) else {
        return;
    };
    let names: Vec<String> = minerals
        .iter()
        .map(|(m, _)| name_of_material(world, m))
        .collect();
    let mut text = format!("{}.", english_list(&names));
    if let Some(user) = discoverer {
        text.push_str(&format!("\nThis vent was first discovered by {user}."));
    }
    if let Some(mut v) = world.get_mut::<OreVent>(vent) {
        v.ore_string = text;
    }
}

fn set_name(world: &mut World, vent: Entity, name: String) {
    if let Some(mut n) = world.get_mut::<DmName>(vent) {
        n.name = name;
    }
}

/// The mining epicenter on the mining level (`GLOB.mining_center`), read from its map.
fn mining_center(world: &World) -> Option<TilePos> {
    let path = world.get_resource::<ss13_mapload::MiningLevel>()?.0.clone();
    let first_z = ss13_mapload::mining_z(world)?;
    let map = ss13_assets::dmm::DmmMap::from_file(&path).ok()?;
    let marks: Vec<bool> = map
        .prefabs
        .iter()
        .map(|p| p.objects.iter().any(|o| o.path == MINING_CENTER))
        .collect();
    let (sx, sy, sz) = map.size;
    for mz in 1..=sz {
        for y in 1..=sy {
            for x in 1..=sx {
                let Some(i) = map.index(x, y, mz) else {
                    continue;
                };
                if marks.get(i as usize).copied().unwrap_or(false) {
                    return Some(TilePos::new(x as i32, y as i32, first_z + (mz - 1) as u16));
                }
            }
        }
    }
    None
}

/// `Initialize` of the mapped vents, then `SSore_generation/Initialize`.
fn setup_vents(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &DmObj, &TilePos), With<ss13_world::Mapped>>();
    let vents: Vec<(Entity, String, TilePos)> = query
        .iter(world)
        .filter(|(_, o, _)| is_under(&o.path, ORE_VENT))
        .map(|(e, o, p)| (e, o.path.clone(), *p))
        .collect();
    if vents.is_empty() {
        return;
    }
    let center = mining_center(world);
    let mut sorted: Vec<(Entity, i32)> = Vec::new();
    for (vent, path, pos) in &vents {
        let boulder_string = {
            let mut rng = world.resource_mut::<CombatRng>();
            ["boulder", "rock", "stone"][rng.range(0, 2) as usize]
        };
        let state = OreVent::of(path, boulder_string);
        let (tapped, unique) = (state.tapped, state.unique);
        world.entity_mut(*vent).insert(state);
        if tapped {
            become_tapped(world, *vent);
        }
        if unique {
            generate_description(world, *vent, None);
            continue;
        }
        match center.filter(|c| c.z == pos.z) {
            Some(c) => sorted.push((*vent, (pos.x - c.x).abs().max((pos.y - c.y).abs()))),
            None => {
                // No epicenter on its level: a random size and minerals by weight.
                let (size, minerals) = {
                    let mut rng = world.resource_mut::<CombatRng>();
                    let size = *pick_weight(&mut rng, &SIZE_WEIGHTS).unwrap_or(&VentSize::Small);
                    (size, generate_minerals(&mut rng, None))
                };
                apply_size(world, *vent, size);
                if let Some(mut v) = world.get_mut::<OreVent>(*vent) {
                    v.minerals = minerals;
                }
            }
        }
    }
    // `sortTim(cmp_numeric_asc)` is stable: equal distances keep map order.
    sorted.sort_by_key(|&(_, d)| d);
    let sizes = sizes_by_distance(sorted.len());
    let mut pool = mineral_pool();
    for (&(vent, _), size) in sorted.iter().zip(sizes) {
        apply_size(world, vent, size);
    }
    for &(vent, _) in &sorted {
        let minerals = generate_minerals(&mut world.resource_mut::<CombatRng>(), Some(&mut pool));
        if let Some(mut v) = world.get_mut::<OreVent>(vent) {
            v.minerals = minerals;
        }
    }
    info!(
        "ore generation: {} vents ({} by distance), {} minerals left in the pool",
        vents.len(),
        sorted.len(),
        pool.len()
    );
}

fn apply_size(world: &mut World, vent: Entity, size: VentSize) {
    if let Some(mut v) = world.get_mut::<OreVent>(vent) {
        v.set_size(size);
    }
    set_name(world, vent, format!("{} ore vent", size.name()));
}

/// `become_tapped`.
fn become_tapped(world: &mut World, vent: Entity) {
    if let Some(mut v) = world.get_mut::<OreVent>(vent) {
        v.tapped = true;
    }
    crate::obj_damage::set_icon_state(world, vent, "ore_vent_active");
}

/// `SSore_generation/fire`: the boulders lying on tapped vents become available, and each
/// tapped vent with room makes one more.
fn ore_generation_fire(world: &mut World) {
    let t = now(world);
    {
        let mut generation = world.resource_mut::<OreGeneration>();
        if t < generation.next_fire {
            return;
        }
        generation.next_fire = t + FIRE_SECS;
        generation.available.clear();
    }
    let mut query = world.query::<(Entity, &OreVent, &TilePos)>();
    let tapped: Vec<(Entity, TilePos)> = query
        .iter(world)
        .filter(|(_, v, _)| v.tapped)
        .map(|(e, _, p)| (e, *p))
        .collect();
    let mut available = Vec::new();
    for (vent, pos) in tapped {
        let here: Vec<Entity> = world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .filter(|&e| world.get::<Boulder>(e).is_some())
            .collect();
        let count = here.len();
        available.extend(here);
        if count >= MAX_BOULDERS_PER_VENT {
            continue;
        }
        available.extend(produce_boulder(world, vent, false));
    }
    world.resource_mut::<OreGeneration>().available = available;
}

/// `produce_boulder(apply_cooldown)`: a new boulder on the vent's tile with its minerals.
pub fn produce_boulder(world: &mut World, vent: Entity, apply_cooldown: bool) -> Option<Entity> {
    let state = world.get::<OreVent>(vent)?.clone();
    let &pos = world.get::<TilePos>(vent)?;
    let t = now(world);
    if apply_cooldown && t < state.manual_ready {
        return None;
    }
    let (path, rolls) = {
        let mut rng = world.resource_mut::<CombatRng>();
        let path = if rng.chance(state.artifact_chance) {
            ARTIFACTS[rng.range(0, ARTIFACTS.len() as u32 - 1) as usize]
        } else {
            BOULDER
        };
        let mut rolls: Vec<(String, f64)> = Vec::new();
        for i in 1..=MINERALS_PER_BOULDER {
            let Some(material) = pick_weight(&mut rng, &state.minerals).cloned() else {
                break;
            };
            rolls.push((material, ore_quantity(&mut rng, state.boulder_size, i)));
        }
        (path, rolls)
    };
    let boulder = new_boulder(world, path, pos)?;
    let durability = world
        .resource_mut::<CombatRng>()
        .range(2, state.boulder_size.max(2));
    if let Some(mut b) = world.get_mut::<Boulder>(boulder) {
        for (material, units) in rolls {
            b.add_material(&material, units);
        }
        b.size = state.boulder_size;
        b.durability = durability;
        b.boulder_string = state.boulder_string;
    }
    super::boulders::update_icon(world, boulder);
    if apply_cooldown && let Some(mut v) = world.get_mut::<OreVent>(vent) {
        v.manual_ready = t + MANUAL_COOLDOWN;
    }
    Some(boulder)
}

fn is_scanner(world: &World, item: Entity) -> bool {
    path_of(world, item).is_some_and(|p| {
        super::rocks::MINING_SCANNERS
            .iter()
            .any(|s| is_under(&p, s))
    })
}

/// tg `item_interaction`: a mining scanner on a vent scans it. True when handled.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<OreVent>(target).is_none() || !is_scanner(world, held) {
        return false;
    }
    if world.get::<OreVent>(target).is_some_and(|v| v.tapped) {
        crate::balloon::balloon_alert_to_viewers(world, target, "vent tapped!", None, &[]);
        return true;
    }
    scan_and_confirm(
        world,
        Actor {
            client,
            mob,
            item: Some(held),
        },
        target,
    );
    true
}

/// `scan_and_confirm`.
fn scan_and_confirm(world: &mut World, actor: Actor, vent: Entity) {
    let Some(state) = world.get::<OreVent>(vent).cloned() else {
        return;
    };
    if state.tapped {
        crate::balloon::balloon_alert_to_viewers(world, vent, "vent tapped!", None, &[]);
        return;
    }
    if state.defending || state.node.is_some() {
        crate::balloon::balloon_alert_to_viewers(world, vent, "protect the node drone!", None, &[]);
        return;
    }
    if !state.discovered {
        if world.get::<DoAfter>(actor.mob).is_some() {
            crate::balloon::balloon_alert(world, actor.mob, vent, "already scanning!");
            return;
        }
        crate::balloon::balloon_alert(world, actor.mob, vent, "scanning...");
        if let Some(&pos) = world.get::<TilePos>(vent) {
            playsound(
                world,
                PlaySound::new(pos, "sound/items/timer.ogg", 30.0, true),
            );
        }
        do_after(
            world,
            actor,
            vent,
            SCAN_DS,
            0,
            Job::Mining(MiningJob::ScanVent),
        );
        return;
    }
    let prompt = ss13_tgui::prompt::ask_world(
        world,
        Ask::new(
            actor.client,
            actor.mob,
            "Begin defending ore vent?",
            "Are you ready to excavate this ore vent?",
            PromptKind::Alert {
                buttons: vec!["Yes".into(), "No".into()],
                autofocus: true,
            },
        ),
    );
    world.entity_mut(prompt).insert(ExcavationPrompt { vent });
}

/// The end of the first scan: discovered, described, a GPS signal, 100 points.
pub fn complete_scan(world: &mut World, mob: Entity, action: &DoAfter) {
    let vent = action.target;
    let fresh = world
        .get::<OreVent>(vent)
        .is_some_and(|v| !v.discovered && !v.tapped);
    if !fresh {
        return;
    }
    if let Some(mut v) = world.get_mut::<OreVent>(vent) {
        v.discovered = true;
    }
    crate::balloon::balloon_alert(world, mob, vent, "vent scanned!");
    let discoverer = crate::security::name_of(world, mob);
    generate_description(world, vent, Some(discoverer));
    let name = world
        .get::<DmName>(vent)
        .map(|n| n.name.clone())
        .unwrap_or_default();
    world
        .entity_mut(vent)
        .insert(crate::devices::gps::Gps::fixed(&name));
    award_points(
        world,
        mob,
        MINER_POINT_MULTIPLIER,
        &format!(
            "You've been awarded {MINER_POINT_MULTIPLIER} mining points for discovery of an ore vent."
        ),
    );
}

/// Mining points onto the account of `mob`'s ID, with the card's message.
fn award_points(world: &mut World, mob: Entity, points: i64, text: &str) {
    let Some(card) = world.get::<Inventory>(mob).and_then(|i| i.id_card(world)) else {
        return;
    };
    let Some(account) = crate::cargo::economy::account_of(world, card) else {
        return;
    };
    let paid = world
        .get_resource_mut::<crate::cargo::economy::Economy>()
        .and_then(|mut e| e.account_mut(account).map(|a| a.mining_points += points))
        .is_some();
    if paid {
        crate::cargo::economy::bank_card_talk(world, account, text);
    }
}

/// "Yes" to the excavation: `can_interact` again, then `pre_wave_defense` and
/// `start_wave_defense`.
fn excavation_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.actor, a.answer.clone()))
        .collect();
    for (prompt, actor, answer) in answers {
        let Some(ExcavationPrompt { vent }) = world.get::<ExcavationPrompt>(prompt).copied() else {
            continue;
        };
        if answer != Some(PromptAnswer::Button(0)) {
            continue;
        }
        let ready = world
            .get::<OreVent>(vent)
            .is_some_and(|v| v.discovered && !v.tapped && !v.defending && v.node.is_none());
        let conscious = world.get::<Stat>(actor) == Some(&Stat::Conscious);
        let near = match (world.get::<TilePos>(actor), world.get::<TilePos>(vent)) {
            (Some(a), Some(v)) => a.chebyshev(*v).is_some_and(|d| d <= 1),
            _ => false,
        };
        if !ready || !conscious || !near {
            continue;
        }
        start_defense(world, vent, Some(actor));
    }
}

/// `pre_wave_defense` (a NODE drone lands, the rock around is cleared) and
/// `start_wave_defense`.
pub fn start_defense(world: &mut World, vent: Entity, user: Option<Entity>) {
    let Some(&pos) = world.get::<TilePos>(vent) else {
        return;
    };
    let Some(state) = world.get::<OreVent>(vent).cloned() else {
        return;
    };
    let node = crate::mobs::spawn::spawn_mob(world, NODE_DRONE, pos);
    if let Some(node) = node
        && let Some(mut health) = world.get_mut::<Health>(node)
    {
        // `arrive`: `300 + (boulder_size / BOULDER_SIZE_SMALL) * 100`.
        health.max = 300.0 + (state.boulder_size / SIZE_SMALL) as f32 * 100.0;
    }
    clear_surroundings(world, pos, user);
    let t = now(world);
    if let Some(mut v) = world.get_mut::<OreVent>(vent) {
        v.node = node;
        v.defending = true;
    }
    world.entity_mut(vent).insert(VentWave {
        completed: 0,
        max: state.waves,
        // `initial_spawn_delay`: the first wave waits a `spawn_time`.
        next_spawn: t + WAVE_SPAWN_TIME,
        spawned: Vec::new(),
        pending: Vec::new(),
    });
}

/// The rock-clearing loop of `pre_wave_defense`: rings 1 to 5 around the vent; mineral walls are
/// drilled (skipped more often farther out), asteroid floors sometimes get rubble.
fn clear_surroundings(world: &mut World, center: TilePos, user: Option<Entity>) {
    for ring in 1..=CLEAR_RINGS {
        for dy in -ring..=ring {
            for dx in -ring..=ring {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let at = TilePos::new(center.x + dx, center.y + dy, center.z);
                let Some(turf) = turf_at(world, at) else {
                    continue;
                };
                if world.get::<DmMineral>(turf).is_some() {
                    let skip = (ring * 15 - 25) as f32;
                    if world.resource_mut::<CombatRng>().chance(skip) {
                        continue;
                    }
                    super::rocks::gets_drilled(world, at, user);
                    if world.resource_mut::<CombatRng>().chance(15.0) {
                        crate::construction::spawn_obj(world, RUBBLE, at);
                    }
                    continue;
                }
                let asteroid = path_of(world, turf).is_some_and(|p| is_under(&p, ASTEROID));
                if asteroid && world.resource_mut::<CombatRng>().chance(10.0) {
                    crate::construction::spawn_obj(world, RUBBLE, at);
                }
            }
        }
    }
}

fn alive(world: &World, e: Entity) -> bool {
    world.get_entity(e).is_ok() && world.get::<Stat>(e) != Some(&Stat::Dead)
}

/// The wave component's `process` and the vent's conclusion handlers.
fn waves(world: &mut World) {
    let t = now(world);
    let mut query = world.query_filtered::<Entity, With<VentWave>>();
    let vents: Vec<Entity> = query.iter(world).collect();
    for vent in vents {
        let (Some(&pos), Some(state)) = (
            world.get::<TilePos>(vent),
            world.get::<OreVent>(vent).cloned(),
        ) else {
            world.entity_mut(vent).remove::<VentWave>();
            continue;
        };
        // `linked_mob`: the drone dead or gone stops the spawner; moved, the vent lets go.
        let node_ok = state.node.is_some_and(|n| alive(world, n));
        if !node_ok {
            conclude(world, vent, Outcome::Lost(LOST_NODE));
            continue;
        }
        if state
            .node
            .and_then(|n| world.get::<TilePos>(n).copied())
            .is_some_and(|p| p != pos)
        {
            conclude(world, vent, Outcome::Lost(LOST_DETACHED));
            continue;
        }
        let mut wave = world.get::<VentWave>(vent).cloned().unwrap_or_default();
        wave.spawned.retain(|&m| alive(world, m));
        let due: Vec<(TilePos, &'static str)> = wave
            .pending
            .iter()
            .filter(|(at, ..)| *at <= t)
            .map(|&(_, p, m)| (p, m))
            .collect();
        wave.pending.retain(|(at, ..)| *at > t);
        for (at, path) in due {
            if let Some(mob) = crate::mobs::spawn::spawn_mob(world, path, at) {
                wave.spawned.push(mob);
                crate::security::show(
                    world,
                    pos,
                    DEFAULT_MESSAGE_RANGE,
                    ChatChannel::Combat,
                    (
                        Span::Danger,
                        format!(
                            "A creature emerges to assault {}.",
                            crate::security::name_of(world, vent)
                        ),
                    ),
                    &[],
                );
            }
        }
        if t >= wave.next_spawn && wave.spawned.is_empty() && wave.pending.is_empty() {
            if wave.completed >= wave.max {
                world.entity_mut(vent).insert(wave);
                conclude(world, vent, Outcome::Won);
                continue;
            }
            wave.completed += 1;
            wave.next_spawn = t + WAVE_SPAWN_TIME;
            let text = format!("wave {}/{}", wave.completed, wave.max);
            crate::balloon::balloon_alert_to_viewers(world, vent, &text, None, &[]);
            spawn_wave(world, pos, &state, &mut wave, t);
        }
        world.entity_mut(vent).insert(wave);
    }
}

/// `try_spawn_mob` of a wave: two kinds of defenders, up to `boulder_size / 5` of each, after
/// the dust cloud's windup.
fn spawn_wave(world: &mut World, pos: TilePos, state: &OreVent, wave: &mut VentWave, t: f64) {
    let per_attempt = (state.boulder_size / SIZE_SMALL) as usize;
    let mut kinds: Vec<&'static str> = state.defenders.to_vec();
    for _ in 0..SPAWN_TYPES_PER_WAVE {
        if kinds.is_empty() {
            break;
        }
        let kind = {
            let mut rng = world.resource_mut::<CombatRng>();
            kinds.remove(rng.range(0, kinds.len() as u32 - 1) as usize)
        };
        let cap = per_attempt.min(MAX_SPAWNED.saturating_sub(wave.spawned.len()));
        let count = if cap > 1 {
            world.resource_mut::<CombatRng>().range(1, cap as u32) as usize
        } else {
            1
        };
        for _ in 0..count {
            let at = spawn_tile(world, pos);
            if at == pos {
                // `COMSIG_SPAWNER_SPAWNED_DEFAULT` -> `anti_cheese`.
                let mut ex = crate::explosions::Explosion::new(0, 1, 3);
                ex.flame = Some(0.0);
                ex.flash = Some(0.0);
                ex.cause = "ore vent".into();
                crate::explosions::explosion(world, pos, ex);
            }
            crate::effects::temp_visual::spawn(world, at, DUST_CLOUD);
            wave.pending.push((t + SPAWN_WINDUP, at, kind));
        }
    }
    playsound(
        world,
        PlaySound::new(pos, "sound/effects/node_drilling.ogg", 40.0, true),
    );
}

/// `select_turf`: a free tile exactly 4 away (`turf_peel(4, 3)`), else any within 4.
fn spawn_tile(world: &mut World, center: TilePos) -> TilePos {
    let index = world.resource::<SpatialIndex>();
    let mut ring = Vec::new();
    let mut all = Vec::new();
    for dy in -SPAWN_DISTANCE..=SPAWN_DISTANCE {
        for dx in -SPAWN_DISTANCE..=SPAWN_DISTANCE {
            let at = TilePos::new(center.x + dx, center.y + dy, center.z);
            if !index.in_bounds(at) {
                continue;
            }
            all.push(at);
            if dx.abs().max(dy.abs()) > SPAWN_EXCLUDE && index.dense_on_tile(at).is_empty() {
                ring.push(at);
            }
        }
    }
    let pool = if ring.is_empty() { all } else { ring };
    if pool.is_empty() {
        return center;
    }
    let i = world
        .resource_mut::<CombatRng>()
        .range(0, pool.len() as u32 - 1) as usize;
    pool[i]
}

const LOST_NODE: &str =
    "creaks and groans as the mining attempt fails, and the vent closes back up.";
const LOST_DETACHED: &str = "detaches";

enum Outcome {
    Won,
    Lost(&'static str),
}

/// `handle_wave_conclusion`: `initiate_wave_win` or `initiate_wave_loss`.
fn conclude(world: &mut World, vent: Entity, outcome: Outcome) {
    world.entity_mut(vent).remove::<VentWave>();
    let Some(&pos) = world.get::<TilePos>(vent) else {
        return;
    };
    let node = world.get::<OreVent>(vent).and_then(|v| v.node);
    let vent_name = crate::security::name_of(world, vent);
    match outcome {
        Outcome::Lost(reason) => {
            let text = if reason == LOST_DETACHED {
                let node_name =
                    node.map_or_else(String::new, |n| crate::security::name_of(world, n));
                format!(
                    "The {node_name} detaches from the {vent_name}, and the vent closes back up!"
                )
            } else {
                format!("{} {reason}", ss13_shared::capitalize(&vent_name))
            };
            crate::security::show(
                world,
                pos,
                DEFAULT_MESSAGE_RANGE,
                ChatChannel::Info,
                (Span::Danger, text),
                &[],
            );
            playsound(
                world,
                PlaySound::new(pos, "sound/effects/rock/rock_break.ogg", 50.0, true),
            );
            reset_drone(world, vent, false);
            if let Some(mut v) = world.get_mut::<OreVent>(vent) {
                v.defending = false;
            }
        }
        Outcome::Won => {
            become_tapped(world, vent);
            info!("ore vent {vent_name} at {pos:?} was tapped");
            crate::balloon::balloon_alert_to_viewers(world, vent, "vent tapped!", None, &[]);
            playsound(
                world,
                PlaySound::new(pos, "sound/effects/node_victory.ogg", 40.0, true),
            );
            world.entity_mut(vent).remove::<crate::devices::gps::Gps>();
            reset_drone(world, vent, true);
            if let Some(mut v) = world.get_mut::<OreVent>(vent) {
                v.defending = false;
            }
            pay_miners(world, vent, pos);
        }
    }
}

/// `initiate_wave_win`'s payout to every living mob within 7 tiles with an ID.
fn pay_miners(world: &mut World, vent: Entity, pos: TilePos) {
    let size = world
        .get::<OreVent>(vent)
        .map_or(SIZE_SMALL, |v| v.boulder_size);
    let reward = MINER_POINT_MULTIPLIER * (i64::from(size) + 2) - MINER_POINT_MULTIPLIER;
    let miners: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(pos, PAYOUT_RANGE)
        .filter(|&e| world.get::<Health>(e).is_some() && world.get::<Inventory>(e).is_some())
        .collect();
    let text = format!("You have been awarded {reward} mining points for your efforts.");
    for miner in miners {
        award_points(world, miner, reward, &text);
    }
}

/// `reset_drone`: the drone flies off.
fn reset_drone(world: &mut World, vent: Entity, success: bool) {
    let node = world
        .get_mut::<OreVent>(vent)
        .and_then(|mut v| v.node.take());
    let Some(node) = node else {
        return;
    };
    if world.get_entity(node).is_err() || world.get::<NodeEscaping>(node).is_some() {
        return;
    }
    let t = now(world);
    world.entity_mut(node).insert(NodeEscaping {
        message_at: t + ESCAPE_FLICK,
        gone_at: t + ESCAPE_FLICK + ESCAPE_FLIGHT,
        success,
        told: false,
    });
}

/// `escape`: the message, then the drone is gone.
fn node_escapes(world: &mut World) {
    let t = now(world);
    let mut query = world.query::<(Entity, &NodeEscaping)>();
    let leaving: Vec<(Entity, NodeEscaping)> = query.iter(world).map(|(e, n)| (e, *n)).collect();
    for (node, escape) in leaving {
        if !escape.told && t >= escape.message_at {
            if let Some(&pos) = world.get::<TilePos>(node) {
                let (span, text) = if escape.success {
                    (
                        Span::Notice,
                        "The drone flies away to safety as the vent is secured.",
                    )
                } else {
                    (
                        Span::Danger,
                        "The drone flies away after failing to open the vent!",
                    )
                };
                crate::security::show(
                    world,
                    pos,
                    DEFAULT_MESSAGE_RANGE,
                    ChatChannel::Info,
                    (span, text.to_owned()),
                    &[],
                );
            }
            if let Some(mut e) = world.get_mut::<NodeEscaping>(node) {
                e.told = true;
            }
        }
        if t >= escape.gone_at {
            world.despawn(node);
        }
    }
}

/// `examine` of a vent.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(v) = world.get::<OreVent>(target) else {
        return;
    };
    if v.discovered {
        let size = match v.boulder_size {
            SIZE_LARGE => "large",
            SIZE_MEDIUM => "medium",
            _ => "small",
        };
        lines.push((
            Span::Notice,
            format!(
                "This vent produces {size} boulders containing {}",
                v.ore_string
            ),
        ));
    } else {
        lines.push((
            Span::Notice,
            "This vent can be scanned with a Mining Scanner.".into(),
        ));
    }
    if v.artifact_chance > 0.0 {
        lines.push((
            Span::Notice,
            "This vent has a low chance to produce an artifact boulder. These may contain rare \
             minerals or strange artifacts."
                .into(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_go_up_a_third_at_a_time() {
        use VentSize::*;
        assert_eq!(
            sizes_by_distance(6),
            vec![Small, Small, Medium, Medium, Large, Large]
        );
        let fifteen = sizes_by_distance(15);
        assert_eq!(fifteen.iter().filter(|s| **s == Small).count(), 5);
        assert_eq!(fifteen.iter().filter(|s| **s == Large).count(), 5);
        // Fewer than three: each vent a size up, capped at large.
        assert_eq!(sizes_by_distance(2), vec![Small, Medium]);
        assert_eq!(sizes_by_distance(4), vec![Small, Medium, Large, Large]);
    }

    #[test]
    fn round_start_minerals_come_out_of_the_pool() {
        let mut rng = CombatRng(7);
        let mut pool = mineral_pool();
        assert_eq!(pool.len(), 60);
        for _ in 0..15 {
            let minerals = generate_minerals(&mut rng, Some(&mut pool));
            assert!(!minerals.is_empty() && minerals.len() <= MINERALS_PER_VENT);
            let mut names: Vec<&String> = minerals.iter().map(|(m, _)| m).collect();
            names.sort();
            names.dedup();
            assert_eq!(names.len(), minerals.len(), "distinct minerals");
            assert!(minerals.iter().all(|(_, w)| (1..=4).contains(w)));
        }
        assert!(pool.len() <= 60 - 15, "{}", pool.len());
    }

    #[test]
    fn the_first_roll_is_the_largest() {
        let mut rng = CombatRng(3);
        for _ in 0..50 {
            let first = ore_quantity(&mut rng, SIZE_SMALL, 1);
            // `5 / ln(2..5)`: 7 down to 3 half sheets.
            assert!((150.0..=350.0).contains(&first), "{first}");
            // `5 / ln(4..7)`: 3 or 2 half sheets.
            let third = ore_quantity(&mut rng, SIZE_SMALL, 3);
            assert!((100.0..=150.0).contains(&third), "{third}");
        }
    }
}
