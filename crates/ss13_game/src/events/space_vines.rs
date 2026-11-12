//! Space Vines (`code/modules/events/space_vines/`): kudzu sprouts on a hallway floor and
//! spreads over the station, picking up mutations on the way; it is cut down with sharp or
//! burning items, burnt by hot air and stopped by the cold.
//!
//! - [`Vine`]: one `/obj/structure/spacevine` (spawned here with its own appearance, no
//!   prototype), [`VineController`]: tg `/datum/spacevine_controller` on its own entity,
//!   processed every 2 s (`SSobj`) by [`process_vines`].
//! - [`Mutation`]: the 20 `/datum/spacevine_mutation` subtypes with their severity and hue.
//! - Clicks: an item on a vine hits it ([`attacked_by`], ×4 for burn and sharp items), an empty
//!   hand frees the mob tangled in it; resisting frees one too ([`resist_vines`]).
//! - Crossing a vine runs its `on_cross` mutations ([`vine_contacts`]).
//!
//! Cost: a process walks every vine of the controller once (air checks) and grows at most
//! `spread_cap` (≤ 60) of them, every 2 s; a step onto a tile looks up its vines.
// shortcut: no venus human traps (the Flowering bud is placed, the bud itself does not grow;
// `docs/parity/mobs/`), no Conductive shocks (vines are not power nodes), no slippery items
// from Slippery vines, no `isvineimmune` factions (no plant-faction mobs here), no eating by
// aliens, no chemicals on vines (`on_chem` does nothing in tg either), no spread animation.

use std::collections::VecDeque;

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_atmos::{Atmos, GasId};
use ss13_mapload::{DmClothing, DmItem, DmName, DmWounding, IconIndex, Opaque};
use ss13_net::server::{IntentReceived, Networked};
use ss13_shared::appearance::{Layer, MouseOpacity};
use ss13_shared::chat::Span;
use ss13_shared::protocol::ClientIntent;
use ss13_shared::{Appearance, Dir, IconStateId, LightSource, Optics, TilePos};
use ss13_world::{Density, Moved, SpatialIndex};

use super::controller::{EventRng, EventState, RoundEvent, pick_weight};
use crate::construction::{is_space, path_of, turf_at};
use crate::explosions::{Explosion, Severity, explosion};
use crate::health::{DamageType, Health, Stat};
use crate::inventory::Inventory;
use crate::medical::surgery::Buckled;
use crate::obj_damage::is_under;
use crate::sound::{PlaySound, playsound};
use crate::tools::Actor;

pub const NAME: &str = "Space Vines";
pub const VINE: &str = "/obj/structure/spacevine";
pub const KUDZU_SEEDS: &str = "/obj/item/seeds/kudzu";
pub const FLOWER_BUD: &str = "/obj/structure/alien/resin/flower_bud";
const ICON: &str = "icons/mob/spacevines.dmi";
const HALLWAY: &str = "/area/station/hallway";

/// `SSobj` wait: the controller's `seconds_per_tick`.
pub const PROCESS_SECS: f64 = 2.0;
/// `max_integrity`.
const MAX_INTEGRITY: f32 = 50.0;
/// `SPACEVINE_LAYER`, `GAME_PLANE`; Timid: `ABOVE_OPEN_TURF_LAYER` on `FLOOR_PLANE`.
const VINE_LAYER: f32 = 4.4;
const GAME_PLANE: i16 = -4;
const ABOVE_OPEN_TURF_LAYER: f32 = 10015.0;
const FLOOR_PLANE: i16 = -6;
/// `FIRE_MINIMUM_TEMPERATURE_TO_SPREAD`, `VINE_FREEZING_POINT`, `T20C`.
const FIRE_MIN_TEMP: f64 = 150.0 + 273.15;
const VINE_FREEZING_POINT: f64 = 100.0;
const T20C: f64 = 293.15;
/// `TEMP_STABILISATION_MUTATION_MAXIMUM_ENERGY`.
const TEMP_STABILISATION_MAX_ENERGY: f64 = 40_000.0;
/// `GAS_MUTATION_REMOVAL_MULTIPLIER`.
const GAS_REMOVAL: f64 = 3.0;
/// `MUTATIVENESS_SCALE_FACTOR`, `MAX_SEVERITY_*`, `MAX_POSSIBLE_PRODUCTIVITY_VALUE`,
/// `SPREAD_CAP_*`, `SPREAD_MULTIPLIER_MAX`, `IDEAL_MAX_SEVERITY`.
const MUTATIVENESS_SCALE: f64 = 0.2;
const MAX_SEVERITY_LINEAR: f64 = 0.15;
const MAX_SEVERITY_CONSTANT: f64 = 10.0;
const MAX_SEVERITY_EVENT_BONUS: i32 = 10;
const MAX_PRODUCTIVITY: f64 = 10.0;
const SPREAD_CAP_LINEAR: f64 = 4.0;
const SPREAD_CAP_CONSTANT: f64 = 20.0;
const SPREAD_MULTIPLIER_MAX: f64 = 50.0;
const IDEAL_MAX_SEVERITY: i64 = 20;
/// Every body part (`HEAD|CHEST|GROIN|LEGS|FEET|ARMS|HANDS`), `HANDS`, `THICKMATERIAL`.
const FULL_BODY: u32 = (1 << 11) - 1;
const HANDS: u32 = (1 << 9) | (1 << 10);
const THICKMATERIAL: u32 = 1 << 4;

const SLASH: &str = "sound/items/weapons/slash.ogg";
const TAP: &str = "sound/items/weapons/tap.ogg";
const WELDER: &str = "sound/items/tools/welder.ogg";
const WHIP: &str = "sound/items/weapons/whip.ogg";
const PIERCE: &str = "sound/items/weapons/pierce.ogg";
const HIT_KICK: &str = "sound/effects/hit_kick.ogg";

/// A `/datum/spacevine_mutation` subtype.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mutation {
    Light,
    Toxicity,
    Explosive,
    FireProof,
    ColdProof,
    TempStabilisation,
    VineEating,
    AggressiveSpread,
    Transparency,
    OxyEater,
    NitroEater,
    Co2Eater,
    PlasmaEater,
    Thorns,
    Hardened,
    Timid,
    Flowering,
    Slippery,
    Conductive,
    Radioactive,
}

impl Mutation {
    pub const ALL: [Self; 20] = [
        Self::Light,
        Self::Toxicity,
        Self::Explosive,
        Self::FireProof,
        Self::ColdProof,
        Self::TempStabilisation,
        Self::VineEating,
        Self::AggressiveSpread,
        Self::Transparency,
        Self::OxyEater,
        Self::NitroEater,
        Self::Co2Eater,
        Self::PlasmaEater,
        Self::Thorns,
        Self::Hardened,
        Self::Timid,
        Self::Flowering,
        Self::Slippery,
        Self::Conductive,
        Self::Radioactive,
    ];

    pub fn bit(self) -> u32 {
        1 << (self as u32)
    }

    /// `severity` (`SEVERITY_TRIVIAL` 1, `MINOR` 2, `AVERAGE` 4, `ABOVE_AVERAGE` 7, `MAJOR` 10).
    pub fn severity(self) -> i32 {
        match self {
            Self::Light | Self::Transparency => 1,
            Self::TempStabilisation
            | Self::VineEating
            | Self::Co2Eater
            | Self::Timid
            | Self::Conductive => 2,
            Self::Toxicity
            | Self::ColdProof
            | Self::OxyEater
            | Self::NitroEater
            | Self::PlasmaEater
            | Self::Thorns
            | Self::Slippery => 4,
            Self::FireProof | Self::Hardened => 7,
            Self::Explosive | Self::AggressiveSpread | Self::Flowering | Self::Radioactive => 10,
        }
    }

    /// `hue` (Transparency has none).
    pub fn hue(self) -> Option<[u8; 3]> {
        let hex: u32 = match self {
            Self::Light => 0xB2EA70,
            Self::Toxicity => 0x9B3675,
            Self::Explosive => 0xD83A56,
            Self::FireProof => 0xFF616D,
            Self::ColdProof => 0x0BD5D9,
            Self::TempStabilisation => 0xB09856,
            Self::VineEating => 0xF4A442,
            Self::AggressiveSpread => 0x316B2F,
            Self::Transparency => return None,
            Self::OxyEater => 0x28B5B5,
            Self::NitroEater => 0xFF7B54,
            Self::Co2Eater => 0x798777,
            Self::PlasmaEater => 0x9074B6,
            Self::Thorns => 0x9ECCA4,
            Self::Hardened => 0x997700,
            Self::Timid => 0xA4A9AC,
            Self::Flowering => 0x66DE93,
            Self::Slippery => 0xA5980C,
            Self::Conductive => 0xAD5C00,
            Self::Radioactive => 0x09B82F,
        };
        Some([(hex >> 16) as u8, (hex >> 8) as u8, hex as u8])
    }

    /// `name`, shown on examine.
    pub fn name(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Toxicity => "Toxic",
            Self::Explosive => "Explosive",
            Self::FireProof => "Fire proof",
            Self::ColdProof => "Cold proof",
            Self::TempStabilisation => "Temperature stabilisation",
            Self::VineEating => "Vine eating",
            Self::AggressiveSpread => "Aggressive spreading",
            Self::Transparency => "transparent",
            Self::OxyEater => "Oxygen consuming",
            Self::NitroEater => "Nitrogen consuming",
            Self::Co2Eater => "CO2 consuming",
            Self::PlasmaEater => "Plasma consuming",
            Self::Thorns => "Thorny",
            Self::Hardened => "Hardened",
            Self::Timid => "Timid",
            Self::Flowering => "Flowering",
            Self::Slippery => "Slippery",
            Self::Conductive => "Conductive",
            Self::Radioactive => "Radioactive",
        }
    }

    /// The gas a `gas_eater` consumes.
    fn eats(self) -> Option<GasId> {
        match self {
            Self::OxyEater => Some(GasId::O2),
            Self::NitroEater => Some(GasId::N2),
            Self::Co2Eater => Some(GasId::CO2),
            Self::PlasmaEater => Some(GasId::PLASMA),
            _ => None,
        }
    }

    /// `GLOB.vine_mutations_list` weights: `IDEAL_MAX_SEVERITY - severity`.
    pub fn weight(self) -> i64 {
        IDEAL_MAX_SEVERITY - i64::from(self.severity())
    }
}

/// A set of [`Mutation`]s.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mutations(pub u32);

impl Mutations {
    pub fn has(self, m: Mutation) -> bool {
        self.0 & m.bit() != 0
    }

    pub fn iter(self) -> impl Iterator<Item = Mutation> {
        Mutation::ALL.into_iter().filter(move |&m| self.has(m))
    }

    pub fn severity(self) -> i32 {
        self.iter().map(Mutation::severity).sum()
    }
}

/// One `/obj/structure/spacevine`.
#[derive(Component, Clone, Debug)]
pub struct Vine {
    /// Its controller entity (tg `master`).
    pub master: Entity,
    pub growth_stage: u8,
    pub can_spread: bool,
    pub can_tangle: bool,
    pub mutations: Mutations,
    pub integrity: f32,
    pub max_integrity: f32,
    /// `light_state`: whether growing makes it opaque.
    pub blocks_light: bool,
    pub fire_proof: bool,
    pub freeze_proof: bool,
    /// `atom_colours[FIXED_COLOUR_PRIORITY]`, inherited by offshoots.
    pub color: [u8; 4],
}

/// `/datum/spacevine_controller`.
#[derive(Component, Clone, Debug)]
pub struct VineController {
    pub vines: Vec<Entity>,
    pub queue: VecDeque<Entity>,
    pub spread_multiplier: f64,
    pub spread_cap: f64,
    pub mutativeness: f64,
    pub max_mutation_severity: i32,
    pub minimum_spread_rate: f64,
    pub next_process: f64,
}

impl VineController {
    /// `New(location, muts, potency, production, event)`'s stats.
    pub fn new(potency: Option<f64>, production: Option<f64>, event: bool, now: f64) -> Self {
        let mut c = Self {
            vines: Vec::new(),
            queue: VecDeque::new(),
            spread_multiplier: 5.0,
            spread_cap: 30.0,
            mutativeness: 1.0,
            max_mutation_severity: 20,
            minimum_spread_rate: 1.0,
            next_process: now + PROCESS_SECS,
        };
        if let Some(potency) = potency {
            c.mutativeness = potency * MUTATIVENESS_SCALE;
            c.max_mutation_severity =
                (potency * MAX_SEVERITY_LINEAR + MAX_SEVERITY_CONSTANT).round() as i32;
        }
        if let Some(production) = production
            && production <= MAX_PRODUCTIVITY
        {
            let span = MAX_PRODUCTIVITY + 1.0 - production;
            c.spread_cap = SPREAD_CAP_LINEAR * span + SPREAD_CAP_CONSTANT;
            c.spread_multiplier = SPREAD_MULTIPLIER_MAX / span;
        }
        if event {
            c.max_mutation_severity += MAX_SEVERITY_EVENT_BONUS;
            c.minimum_spread_rate = 3.0;
        }
        c
    }

    /// The process's `spread_max` for `vine_count` vines (BandaStation: start bonus
    /// `20 - m·n²/800`, base `1·n/m`, tg's 5, 400 and 0.5).
    pub fn spread_max(&self, vine_count: usize, seconds_per_tick: f64) -> usize {
        let n = vine_count as f64;
        let bonus = (20.0 - self.spread_multiplier * n * n / 800.0).max(0.0);
        let base = n / self.spread_multiplier;
        let low = (seconds_per_tick * self.minimum_spread_rate).max(1.0);
        (seconds_per_tick * (base + bonus))
            .clamp(low, self.spread_cap.max(low))
            .round() as usize
    }
}

// --- Event --------------------------------------------------------------------------------

/// `/datum/round_event/spacevine` (with the admin's location, mutations, potency and
/// production when set).
#[derive(Default)]
pub struct SpaceVines {
    pub override_turf: Option<TilePos>,
    pub override_mutations: Option<Vec<Mutation>>,
    pub potency: Option<f64>,
    pub production: Option<f64>,
}

pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(SpaceVines::default())
}

impl RoundEvent for SpaceVines {
    fn start(&mut self, world: &mut World, _: &mut EventState) {
        let candidates = match self.override_turf {
            Some(pos) => vec![pos],
            None => hallway_candidates(world),
        };
        let Some(&pos) = world.resource_mut::<EventRng>().pick(&candidates) else {
            return;
        };
        let muts = match &self.override_mutations {
            Some(muts) => muts.clone(),
            None => {
                let m = *world
                    .resource_mut::<EventRng>()
                    .pick(&Mutation::ALL)
                    .expect("non-empty");
                vec![m]
            }
        };
        let potency = self
            .potency
            .unwrap_or_else(|| world.resource_mut::<EventRng>().0.range(50, 100) as f64);
        let production = self
            .production
            .unwrap_or_else(|| world.resource_mut::<EventRng>().0.range(1, 4) as f64);
        spawn_controller(world, pos, &muts, Some(potency), Some(production), true);
    }
}

/// Up to 100 sampled open hallway floors a vine can enter.
fn hallway_candidates(world: &mut World) -> Vec<TilePos> {
    let floors: Vec<TilePos> = super::station_tiles(world)
        .into_iter()
        .filter(|&(area, _)| super::area_type(world, area).is_some_and(|p| is_under(p, HALLWAY)))
        .map(|(_, p)| p)
        .filter(|&p| is_open(world, p))
        .collect();
    let mut sampled = floors;
    let rng = &mut world.resource_mut::<EventRng>().0;
    for i in (1..sampled.len()).rev() {
        let j = rng.range(0, i as i64) as usize;
        sampled.swap(i, j);
    }
    sampled.truncate(100);
    sampled.retain(|&p| can_enter(world, p));
    sampled
}

/// `new /datum/spacevine_controller(location, muts, potency, production, event)`.
pub fn spawn_controller(
    world: &mut World,
    pos: TilePos,
    muts: &[Mutation],
    potency: Option<f64>,
    production: Option<f64>,
    event: bool,
) -> Entity {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let controller = world
        .spawn((
            Name::new("spacevine_controller"),
            VineController::new(potency, production, event, now),
        ))
        .id();
    spawn_vine(world, controller, pos, None, muts);
    controller
}

// --- Vines --------------------------------------------------------------------------------

fn is_open(world: &World, pos: TilePos) -> bool {
    turf_at(world, pos)
        .and_then(|t| path_of(world, t))
        .is_some_and(|p| is_under(&p, "/turf/open") && !is_under(&p, "/turf/open/openspace"))
}

/// `turf.Enter(vine)`: an open turf without a dense thing a `PASSTABLE | PASSGRILLE` mover
/// cannot pass.
fn can_enter(world: &World, pos: TilePos) -> bool {
    if !is_open(world, pos) {
        return false;
    }
    world
        .resource::<SpatialIndex>()
        .dense_on_tile(pos)
        .iter()
        .all(|&d| {
            let path = path_of(world, d).unwrap_or_default();
            is_under(&path, "/obj/structure/table") || is_under(&path, crate::obj_damage::GRILLE)
        })
}

fn icon(world: &World, state: &str) -> IconStateId {
    world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(ICON, state))
        .unwrap_or(IconStateId(u32::MAX))
}

/// `spawn_spacevine_piece(location, parent, muts)`.
pub fn spawn_vine(
    world: &mut World,
    controller: Entity,
    pos: TilePos,
    parent: Option<Entity>,
    muts: &[Mutation],
) -> Option<Entity> {
    let mut vine = Vine {
        master: controller,
        growth_stage: 0,
        can_spread: true,
        can_tangle: true,
        mutations: Mutations::default(),
        integrity: MAX_INTEGRITY,
        max_integrity: MAX_INTEGRITY,
        blocks_light: true,
        fire_proof: false,
        freeze_proof: false,
        color: [255, 255, 255, 255],
    };
    let add = |vine: &mut Vine, m: Mutation| {
        vine.mutations.0 |= m.bit();
        // `add_atom_colour(hue, FIXED_COLOUR_PRIORITY)`: the latest hue wins.
        if let Some([r, g, b]) = m.hue() {
            vine.color = [r, g, b, vine.color[3]];
        }
    };
    for &m in muts {
        add(&mut vine, m);
    }
    let (mutativeness, max_severity) = world
        .get::<VineController>(controller)
        .map(|c| (c.mutativeness, c.max_mutation_severity))?;
    if let Some(parent) = parent.and_then(|p| world.get::<Vine>(p)).cloned() {
        vine.mutations.0 |= parent.mutations.0;
        vine.color = parent.color;
        if world.resource_mut::<EventRng>().0.prob(mutativeness as f32) {
            let left: Vec<(Mutation, i64)> = Mutation::ALL
                .into_iter()
                .filter(|&m| !vine.mutations.has(m))
                .map(|m| (m, m.weight()))
                .collect();
            if let Some(m) = pick_weight(&left, &mut world.resource_mut::<EventRng>().0)
                && vine.mutations.severity() + m.severity() <= max_severity
            {
                add(&mut vine, m);
            }
        }
    }
    // `on_birth`.
    let (mut layer, mut plane) = (VINE_LAYER, GAME_PLANE);
    if vine.mutations.has(Mutation::Transparency) {
        vine.blocks_light = false;
        vine.color[3] = 125;
    }
    if vine.mutations.has(Mutation::Timid) {
        (layer, plane) = (ABOVE_OPEN_TURF_LAYER, FLOOR_PLANE);
        vine.blocks_light = false;
        vine.can_tangle = false;
    }
    let radioactive = vine.mutations.has(Mutation::Radioactive);
    let color = vine.color;
    let icon_state = icon(world, "Light1");
    let e = world
        .spawn((
            Name::new(VINE),
            DmName {
                name: "space vine".to_owned(),
                desc: "An extremely expansionistic species of vine.".to_owned(),
            },
            pos,
            Appearance {
                icon_state,
                layer: Layer(layer),
                plane,
                color: (color != [255, 255, 255, 255]).then_some(color),
                pixel_offset: [0, 0],
                rotation: 0,
                overlays: vec![],
                mouse_opacity: MouseOpacity::Opaque,
            },
            crate::obj_damage::Anchored(true),
            Networked,
            vine,
        ))
        .id();
    if radioactive {
        // `/datum/element/radioactive(chance = DEFAULT_RADIATION_CHANCE / 3)`.
        let now = crate::tools::now(world);
        world
            .entity_mut(e)
            .insert(crate::radiation::Radioactive::new(
                crate::radiation::DEFAULT_RADIATION_CHANCE / 3.0,
                now,
            ));
    }
    if let Some(mut c) = world.get_mut::<VineController>(controller) {
        c.vines.push(e);
        c.queue.push_back(e);
    }
    Some(e)
}

/// `/obj/structure/spacevine/Destroy` -> `VineDestroyed`: its death mutations, the tangled mob
/// freed; the last vine of a controller leaves kudzu seeds with its mutations' controller
/// stats.
pub fn destroy_vine(world: &mut World, e: Entity) {
    let (Some(vine), Some(&pos)) = (world.get::<Vine>(e).cloned(), world.get::<TilePos>(e)) else {
        return;
    };
    world.entity_mut(e).remove::<Vine>();
    unbuckle_all(world, e);
    world.despawn(e);
    if vine.mutations.has(Mutation::Explosive) {
        explosion(world, pos, Explosion::new(0, 0, 2).cause("explosive vine"));
    }
    let Some(mut c) = world.get_mut::<VineController>(vine.master) else {
        return;
    };
    c.vines.retain(|&v| v != e);
    c.queue.retain(|&v| v != e);
    if !c.vines.is_empty() {
        return;
    }
    let (mutativeness, multiplier) = (c.mutativeness, c.spread_multiplier);
    world.despawn(vine.master);
    if let Some(seed) = crate::construction::spawn_obj(world, KUDZU_SEEDS, pos)
        && let Some(mut s) = world.get_mut::<crate::botany::seeds::Seed>(seed)
    {
        s.potency = (mutativeness / MUTATIVENESS_SCALE).clamp(0.0, 100.0) as f32;
        s.production = (MAX_PRODUCTIVITY + 1.0 - SPREAD_MULTIPLIER_MAX / multiplier)
            .clamp(1.0, MAX_PRODUCTIVITY) as f32;
    }
}

/// The mobs tangled in `vine`.
fn tangled(world: &mut World, vine: Entity) -> Vec<Entity> {
    let mut q = world.query::<(Entity, &Buckled)>();
    q.iter(world)
        .filter(|(_, b)| b.0 == vine)
        .map(|(m, _)| m)
        .collect()
}

fn unbuckle(world: &mut World, mob: Entity) {
    world
        .entity_mut(mob)
        .remove::<(Buckled, crate::furniture::Seated)>();
}

fn unbuckle_all(world: &mut World, vine: Entity) {
    for mob in tangled(world, vine) {
        unbuckle(world, mob);
    }
}

/// `grow()`: medium, then heavy; its `on_grow` mutations.
pub fn grow(world: &mut World, e: Entity) {
    let Some(mut vine) = world.get::<Vine>(e).cloned() else {
        return;
    };
    let roll = world.resource_mut::<EventRng>().0.range(1, 3);
    let state = if vine.growth_stage == 0 {
        vine.growth_stage = 1;
        if vine.blocks_light {
            world.entity_mut(e).insert(Opaque);
            set_optics(world, e, |o| o.opaque = true);
        }
        format!("Med{roll}")
    } else {
        vine.growth_stage = 2;
        format!("Hvy{roll}")
    };
    let id = icon(world, &state);
    if let Some(mut a) = world.get_mut::<Appearance>(e) {
        a.icon_state = id;
    }
    let m = vine.mutations;
    if m.has(Mutation::Light) && vine.growth_stage > 0 {
        set_optics(world, e, |o| {
            o.light = Some(LightSource {
                range: 4.0,
                power: 0.3,
                color: [255, 255, 255],
                height: 1.0,
                angle: 360,
                dir: Dir::North,
                offset: [0, 0],
            });
        });
    }
    vine.fire_proof |= m.has(Mutation::FireProof);
    vine.freeze_proof |= m.has(Mutation::ColdProof);
    if m.has(Mutation::Hardened) {
        if vine.growth_stage > 0 {
            world.entity_mut(e).insert(Density);
        }
        // `modify_max_integrity(initial(max_integrity) * 2)`: integrity scaled with it.
        let new_max = MAX_INTEGRITY * 2.0;
        if vine.integrity > 0.0 {
            vine.integrity = (vine.integrity * new_max / vine.max_integrity)
                .round()
                .max(1.0);
        }
        vine.max_integrity = new_max;
    }
    let stage = vine.growth_stage;
    let pos = world.get::<TilePos>(e).copied();
    world.entity_mut(e).insert(vine);
    if m.has(Mutation::Flowering)
        && stage == 2
        && let Some(pos) = pos
    {
        flower(world, pos);
    }
}

fn set_optics(world: &mut World, e: Entity, f: impl FnOnce(&mut Optics)) {
    match world.get_mut::<Optics>(e) {
        Some(mut o) => f(&mut o),
        None => {
            let mut o = Optics::default();
            f(&mut o);
            world.entity_mut(e).insert(o);
        }
    }
}

/// Flowering's `on_grow`: no bud within 5 tiles, then 10% (`FLOWERING_MUTATION_SPAWN_PROB`).
fn flower(world: &mut World, pos: TilePos) {
    let near = {
        let index = world.resource::<SpatialIndex>();
        (-5..=5).any(|dx| {
            (-5..=5).any(|dy| {
                index
                    .on_tile(TilePos::new(pos.x + dx, pos.y + dy, pos.z))
                    .iter()
                    .any(|&b| path_of(world, b).is_some_and(|p| is_under(&p, FLOWER_BUD)))
            })
        })
    };
    if !near && world.resource_mut::<EventRng>().0.prob(10.0) {
        crate::construction::spawn_obj(world, FLOWER_BUD, pos);
    }
}

/// The living mobs on `pos`.
fn mobs_on(world: &World, pos: TilePos) -> Vec<Entity> {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&m| world.get::<Health>(m).is_some())
        .collect()
}

/// `entangle_mob()`: a fully grown vine with nobody in it catches one mob 25% of the time.
fn entangle_mob(world: &mut World, e: Entity) {
    if !tangled(world, e).is_empty() || world.resource_mut::<EventRng>().0.prob(75.0) {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    for mob in mobs_on(world, pos) {
        entangle(world, e, mob);
        if !tangled(world, e).is_empty() {
            break;
        }
    }
}

/// `entangle(victim)`: its `on_buckle` mutations, then buckled when alive and free.
pub fn entangle(world: &mut World, e: Entity, mob: Entity) {
    let Some(vine) = world.get::<Vine>(e).cloned() else {
        return;
    };
    if vine.mutations.has(Mutation::AggressiveSpread) {
        aggrospread_act(world, &vine, e, mob);
    }
    let alive = world.get::<Stat>(mob).is_some_and(|s| *s != Stat::Dead);
    let free = world.get::<Buckled>(mob).is_none();
    if alive && free && vine.can_tangle && world.get_entity(e).is_ok() {
        let verb = *world
            .resource_mut::<EventRng>()
            .pick(&["wind", "tangle", "tighten"])
            .expect("non-empty");
        crate::chem::tell_mob(
            world,
            mob,
            Span::Danger,
            &format!("The vines {verb} around you!"),
        );
        world
            .entity_mut(mob)
            .insert((Buckled(e), crate::furniture::Seated { chair: e, up: 0 }));
    }
}

/// `spread()`: a random cardinal neighbour that is not space, open to the vine and free of
/// vines (Vine eating: of vines without it), its `on_spread` mutations, then an offshoot.
fn spread(world: &mut World, e: Entity) {
    let (Some(vine), Some(&pos)) = (world.get::<Vine>(e).cloned(), world.get::<TilePos>(e)) else {
        return;
    };
    let dir = *world
        .resource_mut::<EventRng>()
        .pick(&Dir::CARDINALS)
        .expect("non-empty");
    let to = pos.step(dir);
    if !world.resource::<SpatialIndex>().in_bounds(to)
        || is_space(world, to)
        || !can_enter(world, to)
    {
        return;
    }
    if turf_at(world, to)
        .and_then(|t| path_of(world, t))
        .is_some_and(|p| is_under(&p, "/turf/open/chasm") || is_under(&p, "/turf/open/lava"))
    {
        return;
    }
    let taken: Vec<Entity> = vines_on(world, to);
    if let Some(&other) = taken.first() {
        let eats = vine.mutations.has(Mutation::VineEating);
        let other_eats = world
            .get::<Vine>(other)
            .is_some_and(|v| v.mutations.has(Mutation::VineEating));
        if !eats || other_eats {
            return;
        }
    }
    if vine.mutations.has(Mutation::VineEating) {
        for prey in taken {
            destroy_vine(world, prey);
        }
    }
    if vine.mutations.has(Mutation::AggressiveSpread) {
        for mob in mobs_on(world, to) {
            aggrospread_act(world, &vine, e, mob);
        }
    }
    spawn_vine(world, vine.master, to, Some(e), &[]);
}

fn vines_on(world: &World, pos: TilePos) -> Vec<Entity> {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&v| world.get::<Vine>(v).is_some())
        .collect()
}

/// `damage(mob, amount, kind)`: `apply_damage` spread over the body.
fn hurt(world: &mut World, mob: Entity, kind: DamageType, amount: f32) {
    let (Some(mut health), Some(&stat)) =
        (world.get::<Health>(mob).cloned(), world.get::<Stat>(mob))
    else {
        return;
    };
    let mut new = stat;
    crate::health::apply_damage(&mut health, &mut new, kind, amount);
    world.entity_mut(mob).insert(health);
    if new != stat {
        world.entity_mut(mob).insert(new);
    }
}

/// `aggrospread_act`: a living mob in the way is threshed (75 brute), a human 40% stung by
/// thorns (50 brute, 1 s stun) if Thorny, else 80% lacerated (60 brute, 2 s down), else
/// smashed (60 brute, 3 s down).
// shortcut: no armor, wounds, limb pick or throw for the smash; the damage spreads over the body.
fn aggrospread_act(world: &mut World, vine: &Vine, e: Entity, mob: Entity) {
    if world.get::<Stat>(mob).is_none_or(|s| *s == Stat::Dead) {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let _ = e;
    let human = world.get::<Inventory>(mob).is_some();
    if !human {
        hurt(world, mob, DamageType::Brute, 75.0);
        playsound(
            world,
            PlaySound::new(pos, WHIP, 50.0, true).extrarange(-1.0),
        );
        crate::chem::tell_mob(
            world,
            mob,
            Span::Danger,
            "You are brutally threshed by space vine!",
        );
        return;
    }
    let rng = &mut world.resource_mut::<EventRng>().0;
    let thorn = vine.mutations.has(Mutation::Thorns) && rng.prob(40.0);
    let lacerate = !thorn && rng.prob(80.0);
    let (amount, sound, text) = if thorn {
        crate::status_effects::apply(world, mob, crate::status_effects::Incap::Stun, 1.0);
        (50.0, PIERCE, "You are nailed by a sharp thorn!")
    } else if lacerate {
        crate::status_effects::knockdown(world, mob, 2.0, 0.0);
        (60.0, WHIP, "You are lacerated by an outburst of vines!")
    } else {
        crate::status_effects::knockdown(world, mob, 3.0, 0.0);
        (60.0, HIT_KICK, "You are smashed by a large vine!")
    };
    hurt(world, mob, DamageType::Brute, amount);
    playsound(
        world,
        PlaySound::new(pos, sound, 50.0, true).extrarange(-1.0),
    );
    crate::chem::tell_mob(world, mob, Span::Danger, text);
}

/// Whether the mob's thick clothing covers all of `parts`.
fn thick_covered(world: &World, mob: Entity, parts: u32) -> bool {
    let Some(inv) = world.get::<Inventory>(mob) else {
        return false;
    };
    let covered = inv
        .equipped()
        .filter_map(|item| world.get::<DmClothing>(item))
        .filter(|c| c.clothing_flags & THICKMATERIAL != 0)
        .fold(0, |acc, c| acc | c.body_parts_covered);
    covered & parts == parts
}

/// `on_cross` of a vine's mutations for a living mob stepping on it.
pub fn on_cross(world: &mut World, e: Entity, mob: Entity) {
    let Some(vine) = world.get::<Vine>(e).cloned() else {
        return;
    };
    let m = vine.mutations;
    let silicon = world.get::<crate::silicon::chassis::Chassis>(mob).is_some();
    if m.has(Mutation::Toxicity) && !silicon && world.resource_mut::<EventRng>().0.prob(10.0) {
        if m.has(Mutation::Thorns) {
            crate::chem::tell_mob(
                world,
                mob,
                Span::Warning,
                "You are pricked by thorns and feel a strange sensation.",
            );
            hurt(world, mob, DamageType::Tox, 20.0);
        } else if !thick_covered(world, mob, FULL_BODY) {
            crate::chem::tell_mob(
                world,
                mob,
                Span::Warning,
                "You accidentally touch the vine and feel a strange sensation.",
            );
            hurt(world, mob, DamageType::Tox, 20.0);
        }
    }
    if m.has(Mutation::Thorns) && world.resource_mut::<EventRng>().0.prob(10.0) {
        hurt(world, mob, DamageType::Brute, 15.0);
        crate::chem::tell_mob(
            world,
            mob,
            Span::Danger,
            "You cut yourself on the thorny vines.",
        );
    }
    if m.has(Mutation::Flowering) && world.resource_mut::<EventRng>().0.prob(25.0) {
        entangle(world, e, mob);
    }
    if m.has(Mutation::Slippery) && vine.growth_stage > 0 {
        // `/datum/component/slippery` (5 s, `NO_SLIP_WHEN_WALKING`) after it grew.
        crate::janitor::try_slip(world, mob, (5.0, true));
    }
}

// --- Processing ---------------------------------------------------------------------------

/// `/datum/spacevine_controller/process` of every due controller, plus the vines' air
/// reactions (`atmos_expose`).
pub fn process_vines(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &VineController)>();
    let due: Vec<Entity> = q
        .iter(world)
        .filter(|(_, c)| c.next_process <= now)
        .map(|(e, _)| e)
        .collect();
    for controller in due {
        if let Some(mut c) = world.get_mut::<VineController>(controller) {
            c.next_process = now + PROCESS_SECS;
        }
        process_controller(world, controller, PROCESS_SECS);
    }
}

/// One controller tick.
pub fn process_controller(world: &mut World, controller: Entity, seconds_per_tick: f64) {
    let Some(vines) = world
        .get::<VineController>(controller)
        .map(|c| c.vines.clone())
    else {
        return;
    };
    if vines.is_empty() {
        world.despawn(controller);
        return;
    }
    for &v in &vines {
        atmos_expose(world, v);
    }
    let Some(mut c) = world.get_mut::<VineController>(controller) else {
        return;
    };
    let spread_max = c.spread_max(c.vines.len(), seconds_per_tick);
    let queue: Vec<Entity> = c.queue.drain(..).collect();
    let mut rest = Vec::new();
    let mut processed = Vec::new();
    let mut iter = queue.into_iter();
    for v in iter.by_ref() {
        let Some(vine) = world.get::<Vine>(v).cloned() else {
            continue;
        };
        if !vine.can_spread {
            rest.push(v);
            continue;
        }
        processed.push(v);
        process_mutations(world, v, &vine);
        if world.get::<Vine>(v).is_none() {
            continue;
        }
        if vine.growth_stage >= 2 {
            entangle_mob(world, v);
        } else {
            let p = 100.0 * (1.0 - 0.9f64.powf(seconds_per_tick));
            if world.resource_mut::<EventRng>().0.prob(p as f32) {
                grow(world, v);
            }
        }
        spread(world, v);
        if processed.len() >= spread_max {
            break;
        }
    }
    rest.extend(iter);
    let Some(mut c) = world.get_mut::<VineController>(controller) else {
        return;
    };
    // Offshoots born this tick sit in `queue`: after the waiting vines, before the processed.
    let born: Vec<Entity> = c.queue.drain(..).collect();
    let alive = |v: &Entity| c.vines.contains(v);
    let order: VecDeque<Entity> = rest
        .into_iter()
        .chain(born)
        .chain(processed)
        .filter(alive)
        .collect();
    c.queue = order;
}

/// `process_mutation`: the gas eaters take `3 × growth_stage` moles.
fn process_mutations(world: &mut World, e: Entity, vine: &Vine) {
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    let amount = GAS_REMOVAL * f64::from(vine.growth_stage);
    for m in vine.mutations.iter() {
        let Some(gas) = m.eats() else {
            continue;
        };
        if let Some(mut atmos) = world.get_resource_mut::<Atmos>() {
            atmos.modify_air(pos, |air| {
                let left = (air.get(gas) - amount).max(0.0);
                air.set(gas, left);
                air.garbage_collect();
            });
        }
    }
}

/// `atmos_expose`: Temperature stabilisation pulls the air toward 20 °C; hot air (above
/// `FIRE_MINIMUM_TEMPERATURE_TO_SPREAD`) burns a vine that is not fire proof, air below 100 K
/// stops one that is not cold proof from spreading until it warms.
fn atmos_expose(world: &mut World, e: Entity) {
    let (Some(vine), Some(&pos)) = (world.get::<Vine>(e).cloned(), world.get::<TilePos>(e)) else {
        return;
    };
    let Some(mut atmos) = world.get_resource_mut::<Atmos>() else {
        return;
    };
    if vine.mutations.has(Mutation::TempStabilisation) {
        atmos.modify_air(pos, |air| {
            let capacity = air.heat_capacity();
            if capacity <= 0.0 {
                return;
            }
            let energy =
                ((air.temperature - T20C).abs() * capacity).min(TEMP_STABILISATION_MAX_ENERGY);
            let delta = energy / capacity;
            if delta < 0.1 {
                return;
            }
            air.temperature += if air.temperature > T20C {
                -delta
            } else {
                delta
            };
        });
    }
    let Some(temp) = atmos.temperature(pos) else {
        return;
    };
    let mut can_spread = vine.can_spread;
    if !can_spread && (temp >= VINE_FREEZING_POINT || vine.freeze_proof) {
        can_spread = true;
    }
    if temp > FIRE_MIN_TEMP && !vine.fire_proof {
        destroy_vine(world, e);
        return;
    }
    if temp < VINE_FREEZING_POINT && !vine.freeze_proof {
        can_spread = false;
    }
    if can_spread != vine.can_spread
        && let Some(mut v) = world.get_mut::<Vine>(e)
    {
        v.can_spread = can_spread;
    }
}

/// `/obj/structure/spacevine/ex_act`: Explosive vines go off (devastation: half a second
/// later in tg, at once here), others are gone `34 × severity`% of the time. True for vines.
pub fn ex_act(world: &mut World, e: Entity, severity: Severity) -> bool {
    let Some(mutations) = world.get::<Vine>(e).map(|v| v.mutations) else {
        return false;
    };
    let level = match severity {
        Severity::Light => 1.0,
        Severity::Heavy => 2.0,
        Severity::Devastate => 3.0,
    };
    if mutations.has(Mutation::Explosive) || world.resource_mut::<EventRng>().0.prob(34.0 * level) {
        destroy_vine(world, e);
    }
    true
}

// --- Interaction --------------------------------------------------------------------------

/// The ×4 multipliers of `attacked_by` and the `on_hit` mutations (Fire proof: burn ×0,
/// Hardened: sharp ×0.5).
pub fn hit_multiplier(mutations: Mutations, burn: bool, sharp: bool) -> f32 {
    let mut mult = 1.0;
    if burn {
        mult *= 4.0;
        if mutations.has(Mutation::FireProof) {
            mult *= 0.0;
        }
    }
    if sharp {
        mult *= 4.0;
        if mutations.has(Mutation::Hardened) {
            mult *= 0.5;
        }
    }
    mult
}

/// `attackby` -> `attacked_by` of a held item on a vine: its `on_hit` mutations (thorns cut the
/// bare-handed 10% of the time), then `force × multiplier` off its integrity; the slash, tap
/// or welder sound. True when `target` is a vine.
pub fn attacked_by(world: &mut World, actor: Actor, target: Entity) -> bool {
    let (Some(item), Some(vine)) = (actor.item, world.get::<Vine>(target).cloned()) else {
        return false;
    };
    let Some(dm) = world.get::<DmItem>(item).cloned() else {
        return false;
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(mut next) = world.get_mut::<crate::combat::NextMove>(actor.mob) {
        next.0 = now + f64::from(dm.attack_speed) / 10.0;
    }
    if vine.mutations.has(Mutation::Thorns)
        && !thick_covered(world, actor.mob, HANDS)
        && world.resource_mut::<EventRng>().0.prob(10.0)
    {
        hurt(world, actor.mob, DamageType::Brute, 15.0);
        crate::tools::tell(
            world,
            actor.client,
            Span::Danger,
            "You cut yourself on the thorny vines.",
        );
    }
    let damtype = DamageType::from_dm(&dm.damtype).unwrap_or(DamageType::Brute);
    let sharp = world
        .get::<DmWounding>(item)
        .is_some_and(|w| w.sharpness != 0);
    let damage = dm.force * hit_multiplier(vine.mutations, damtype == DamageType::Burn, sharp);
    let Some(&pos) = world.get::<TilePos>(target) else {
        return true;
    };
    let sound = match damtype {
        DamageType::Burn => PlaySound::new(pos, WELDER, 100.0, true),
        _ if damage > 0.0 => PlaySound::new(pos, SLASH, 50.0, true),
        _ => PlaySound::new(pos, TAP, 50.0, true),
    };
    playsound(world, sound);
    let destroyed = match world.get_mut::<Vine>(target) {
        Some(mut v) => {
            v.integrity -= damage;
            v.integrity <= 0.0
        }
        None => false,
    };
    if destroyed {
        destroy_vine(world, target);
    }
    true
}

/// `attack_hand` -> `user_unbuckle_mob`: an empty hand frees the mob tangled in the vine.
pub fn attack_hand(world: &mut World, target: Entity) -> bool {
    if world.get::<Vine>(target).is_none() {
        return false;
    }
    let mobs = tangled(world, target);
    for &mob in &mobs {
        unbuckle(world, mob);
    }
    !mobs.is_empty()
}

/// `resist_buckle` of a mob tangled in vines: free at once (vines have no `unbuckle` delay).
pub(super) fn resist_vines(world: &mut World, mut intents: Local<MessageCursor<IntentReceived>>) {
    let clients: Vec<Entity> = intents
        .read(world.resource::<Messages<IntentReceived>>())
        .filter(|i| matches!(i.intent, ClientIntent::Resist))
        .map(|i| i.client)
        .collect();
    for client in clients {
        let Some(mob) = world.resource::<crate::server::Players>().mob(client) else {
            continue;
        };
        let in_vine = world
            .get::<Buckled>(mob)
            .is_some_and(|b| world.get::<Vine>(b.0).is_some());
        if in_vine {
            unbuckle(world, mob);
        }
    }
}

/// `COMSIG_ATOM_ENTERED` of vines: living mobs stepping onto them.
pub(super) fn vine_contacts(world: &mut World, mut moves: Local<MessageCursor<Moved>>) {
    let moved: Vec<Moved> = moves
        .read(world.resource::<Messages<Moved>>())
        .copied()
        .collect();
    for Moved { entity, to, .. } in moved {
        if world.get::<Health>(entity).is_none() {
            continue;
        }
        for vine in vines_on(world, to) {
            on_cross(world, vine, entity);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_stats_follow_tg_and_bandastation() {
        let c = VineController::new(Some(100.0), Some(1.0), true, 0.0);
        assert!((c.mutativeness - 20.0).abs() < 1e-9);
        assert_eq!(c.max_mutation_severity, 25 + 10);
        assert!((c.spread_cap - 60.0).abs() < 1e-9);
        assert!((c.spread_multiplier - 5.0).abs() < 1e-9);
        assert!((c.minimum_spread_rate - 3.0).abs() < 1e-9);
        // One vine: bonus ~20, base 0.2 -> round(2 × 20.2) = 40.
        assert_eq!(c.spread_max(1, 2.0), 40);
        // Many vines: capped at 60.
        assert_eq!(c.spread_max(1000, 2.0), 60);
        let slow = VineController::new(Some(1.0), Some(10.0), false, 0.0);
        assert_eq!(slow.max_mutation_severity, 10);
        assert!((slow.spread_cap - 24.0).abs() < 1e-9);
        // 100 vines, multiplier 50: bonus 0, base 2 -> 4.
        assert_eq!(slow.spread_max(100, 2.0), 4);
    }

    #[test]
    fn mutations_weights_and_hits() {
        assert_eq!(Mutation::ALL.len(), 20);
        assert_eq!(Mutation::Explosive.weight(), 10);
        assert_eq!(Mutation::Light.weight(), 19);
        assert_eq!(Mutation::Transparency.hue(), None);
        assert_eq!(Mutation::Light.hue(), Some([0xB2, 0xEA, 0x70]));
        let none = Mutations::default();
        assert_eq!(hit_multiplier(none, true, false), 4.0);
        assert_eq!(hit_multiplier(none, false, true), 4.0);
        assert_eq!(hit_multiplier(none, false, false), 1.0);
        let tough = Mutations(Mutation::FireProof.bit() | Mutation::Hardened.bit());
        assert_eq!(tough.severity(), 14);
        assert_eq!(hit_multiplier(tough, true, false), 0.0);
        assert_eq!(hit_multiplier(tough, false, true), 2.0);
    }

    #[test]
    fn drops_are_runtime_types() {
        let registry = ss13_mapload::runtime::runtime_types();
        for p in [KUDZU_SEEDS, FLOWER_BUD] {
            assert!(registry.contains(&p), "{p} not in ss13_mapload::runtime");
        }
    }
}
