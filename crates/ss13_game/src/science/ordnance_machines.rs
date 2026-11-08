//! The ordnance lab's machines (server), after tg `research/ordnance/doppler_array.dm` and
//! `tank_compressor.dm`.
//!
//! - Doppler array (`/obj/machinery/doppler_array`): every explosion (`COMSIG_GLOB_EXPLOSION`,
//!   called from `crate::explosions::explosion`) on its z-level within 150 tiles in the
//!   direction it faces is recorded while it is powered with its panel closed (at most one per
//!   second, the cooldown spent even on explosions it cannot see): epicenter, temporal
//!   displacement, factual radii and, for a capped bomb, the theoretical ones, and what reacted
//!   in the gas tank that blew up (`crate::explosions::sources::TankResults`). It says the
//!   findings. A record saved to the inserted data disk lists the explosive experiments it
//!   qualifies for with its shockwave radius as the score. Alt-click turns it while
//!   unanchored; a wrench (de)anchors it.
//! - Tank compressor (`binary/tank_compressor`): a gas tank and a data disk go in (not in combat
//!   mode, panel closed). While on, it pumps its input port (the green, facing side) into the
//!   tank at up to 20 L per SSair tick until the tank is over 5000 kPa, and stops when a leaking
//!   tank falls to the leak pressure. A leaking or rupturing tank's gas goes into its buffer
//!   and its explosion is absorbed; when the tank is destroyed (or ejected) a buffer of over 10
//!   moles is recorded (the gases and their moles), then the buffer goes out of the output port.
//!   A record saved to the disk lists the gaseous experiments whose gas it holds, the moles as
//!   the score. A tank over 2634 kPa cannot be ejected.
// shortcut: the doppler sees only its own z-level (no linked station levels), has no
// directional light or overlays and the default screwdriver/crowbar deconstruction is the
// machine framework's; the compressor cannot be rotated (its pipe ports are fixed at map load),
// has no overlays and no room shake on an absorbed explosion; timestamps are round time.

use std::sync::atomic::{AtomicU32, Ordering};

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_atmos::pipenet::Pipenet;
use ss13_atmos::{AtmosSystems, GASES, GasId, GasMixture, air_tick_due};
use ss13_mapload::DmObj;
use ss13_shared::chat::Span;
use ss13_shared::{Dir, Facing, TilePos};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use super::ordnance::*;
use super::scipaper::{EXPERIMENTS, MINIMUM_MOLE_COUNT, Requirement};
use crate::atmos_machinery::AtmosDevice;
use crate::construction::{panel_open, path_of};
use crate::explosions::sources::{
    Leaking, TANK_FRAGMENT_PRESSURE, TANK_LEAK_PRESSURE, TankResults, note_merge,
};
use crate::internals::TankAir;
use crate::modpc::files::{ComputerDisk, OrdnanceFile, OrdnanceRecord};
use crate::obj_damage::{Anchored, is_under};
use crate::power::Unpowered;
use crate::sound::{PlaySound, playsound};
use crate::storage::StoredIn;
use crate::tools::{Actor, ToolKind, tool_of, use_tool};

pub const DOPPLER: &str = "/obj/machinery/doppler_array";
pub const COMPRESSOR: &str = "/obj/machinery/atmospherics/components/binary/tank_compressor";
const TANK: &str = "/obj/item/tank";
/// `cooldown = 10` deciseconds, `max_dist`.
const DOPPLER_COOLDOWN: f64 = 1.0;
const MAX_DIST: u32 = 150;
/// `TANK_COMPRESSOR_PRESSURE_LIMIT`, `TANK_COMPRESSOR_MAX_TRANSFER_RATE`,
/// `SIGNIFICANT_AMOUNT_OF_MOLES`.
const PRESSURE_LIMIT: f64 = 5000.0;
const MAX_TRANSFER_RATE: f64 = 20.0;
const SIGNIFICANT_MOLES: f64 = 10.0;
/// `PUMP_MAX_PRESSURE + ONE_ATMOSPHERE`: 25 atmospheres plus one.
const EJECT_PRESSURE: f64 = 26.0 * 101.325;
const PING: &str = "sound/machines/ping.ogg";
const ERROR: &str = "sound/machines/terminal/terminal_error.ogg";
const CARD_SLIDE: &str = "sound/machines/card_slide.ogg";
const RATCHET: &str = "sound/items/tools/ratchet.ogg";
/// `default_unfasten_wrench(user, tool, time = 20)`, deciseconds.
const WRENCH_DS: f32 = 20.0;

/// Data file uids and compressor record ids, unique per round.
static NEXT_UID: AtomicU32 = AtomicU32::new(1);

fn next_uid() -> u32 {
    NEXT_UID.fetch_add(1, Ordering::Relaxed)
}

/// tg `/datum/data/tachyon_record`.
#[derive(Clone, Debug, PartialEq)]
pub struct TachyonRecord {
    /// `record_number` at the time: the window's `ref`.
    pub number: u32,
    pub timestamp: String,
    pub coordinates: String,
    pub displacement: f64,
    pub factual: [f32; 3],
    pub theory: Option<[f32; 3]>,
    /// `reaction_results`: `None` unless a gas tank exploded.
    pub results: Option<TankResults>,
    /// `explosion_identifier`.
    pub explosion_index: u32,
}

impl TachyonRecord {
    pub fn name(&self) -> String {
        format!("Log Recording #{}", self.number)
    }
}

/// The doppler array's state (`next_announce`, `record_number`, `records`, `inserted_disk`).
#[derive(Component, Clone, Debug)]
pub struct Doppler {
    pub next_announce: f64,
    pub record_number: u32,
    pub records: Vec<TachyonRecord>,
    pub disk: Option<Entity>,
}

impl Default for Doppler {
    fn default() -> Self {
        Self {
            next_announce: 0.0,
            record_number: 1,
            records: Vec::new(),
            disk: None,
        }
    }
}

/// What the doppler's window shows.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct DopplerView(pub DopplerArrayData);

/// tg `/datum/data/compressor_record`.
#[derive(Clone, Debug, PartialEq)]
pub struct CompressorRecord {
    /// Unique per round: the window's `ref` and the record's identity in papers.
    pub id: u32,
    pub name: String,
    /// `experiment_source`: the tank's name.
    pub source: String,
    pub timestamp: String,
    pub gases: Vec<(GasId, f64)>,
}

/// The tank compressor's state.
#[derive(Component, Clone, Debug)]
pub struct Compressor {
    pub active: bool,
    pub transfer_rate: f64,
    /// `leaked_gas_buffer`.
    pub buffer: GasMixture,
    pub records: Vec<CompressorRecord>,
    pub last_pressure: f64,
    pub record_number: u32,
    pub tank: Option<Entity>,
    pub disk: Option<Entity>,
}

impl Default for Compressor {
    fn default() -> Self {
        Self {
            active: false,
            transfer_rate: MAX_TRANSFER_RATE,
            buffer: GasMixture::default(),
            records: Vec::new(),
            last_pressure: 0.0,
            record_number: 1,
            tank: None,
            disk: None,
        }
    }
}

/// What the compressor's window shows.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct CompressorView(pub TankCompressorData);

pub struct OrdnancePlugin;

impl Plugin for OrdnancePlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::EmptyHand, 1787, |w, c| {
            !c.modifiers.right && interact(w, c.client, c.mob, c.target)
        });
        app.add_click_handler(ClickPhase::HeldOnTarget, 1527, |w, c| {
            c.held
                .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target))
        });
        app.register_ui::<DopplerArrayUi>()
            .register_ui::<TankCompressorUi>()
            .add_observer(attach_doppler)
            .add_systems(
                Update,
                (
                    (doppler_acts, compressor_acts).in_set(UiSystems::Handle),
                    refresh_compressor_views,
                ),
            )
            .add_systems(
                Update,
                process_compressors
                    .run_if(air_tick_due)
                    .in_set(AtmosSystems::Machinery),
            );
    }
}

/// Map-loaded doppler arrays listen for explosions from the start (`Initialize`).
fn attach_doppler(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    if let Ok(obj) = objs.get(add.entity)
        && is_under(&obj.path, DOPPLER)
    {
        commands
            .entity(add.entity)
            .insert_if_new((Doppler::default(), DopplerView::default()));
    }
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn say(world: &mut World, e: Entity, text: &str) {
    crate::atmos_machinery::machine_say(world, e, text);
}

fn sound(world: &mut World, e: Entity, file: &str, volume: f32) {
    if let Some(pos) = crate::explosions::turf_of(world, e) {
        playsound(world, PlaySound::new(pos, file, volume, false));
    }
}

fn inside(world: &World, e: Option<Entity>, container: Entity) -> Option<Entity> {
    e.filter(|&e| world.get::<StoredIn>(e).is_some_and(|s| s.0 == container))
}

/// Numbers as DM prints them (whole ones without a fraction).
fn dm_num(n: f64) -> String {
    let r = (n * 1000.0).round() / 1000.0;
    if r.fract() == 0.0 {
        format!("{r:.0}")
    } else {
        format!("{r}")
    }
}

fn storage(world: &World, disk: Option<Entity>) -> String {
    match disk.and_then(|d| world.get::<ComputerDisk>(d)) {
        Some(d) => format!("{} / {} GQ", d.used_capacity(), d.max_capacity),
        None => " /  GQ".to_owned(),
    }
}

/// BYOND `get_dir(from, to)`.
fn get_dir(from: TilePos, to: TilePos) -> u8 {
    let mut d = 0;
    if to.y > from.y {
        d |= 1;
    } else if to.y < from.y {
        d |= 2;
    }
    if to.x > from.x {
        d |= 4;
    } else if to.x < from.x {
        d |= 8;
    }
    d
}

fn facing(world: &World, e: Entity) -> Dir {
    world.get::<Facing>(e).map_or(Dir::South, |f| f.0)
}

/// `/obj/item/disk/computer`: the item is a data disk.
fn is_disk(world: &World, e: Entity) -> bool {
    world.get::<ComputerDisk>(e).is_some()
}

fn put_in(world: &mut World, item: Entity, machine: Entity) {
    crate::storage::put_inside(world, item, machine);
}

/// `eject_disk(user)` / `eject_tank(user)`: into the user's hands when adjacent, else down.
fn eject_item(world: &mut World, item: Entity, machine: Entity, user: Option<Entity>) {
    let adjacent = user.filter(|&u| {
        match (
            world.get::<TilePos>(u),
            crate::explosions::turf_of(world, machine),
        ) {
            (Some(a), Some(b)) => a.chebyshev(b).is_some_and(|d| d <= 1),
            _ => false,
        }
    });
    match adjacent {
        Some(mob) => crate::assemblies::server::to_hands(world, mob, item),
        None => crate::assemblies::server::drop_out(world, item, machine),
    }
}

// ---------------------------------------------------------------------------------------------
// Doppler array

/// The doppler state of `e`, with what left it forgotten (`Exited`).
pub fn doppler(world: &mut World, e: Entity) -> Option<Doppler> {
    if let Some(d) = world.get::<Doppler>(e) {
        let mut d = d.clone();
        d.disk = inside(world, d.disk, e);
        return Some(d);
    }
    if !path_of(world, e).is_some_and(|p| is_under(&p, DOPPLER)) {
        return None;
    }
    let d = Doppler::default();
    world
        .entity_mut(e)
        .insert((d.clone(), DopplerView::default()));
    Some(d)
}

fn set_doppler(world: &mut World, e: Entity, d: Doppler) {
    let records = d
        .records
        .iter()
        .map(|r| {
            let mut reaction_results: Vec<String> = Vec::new();
            if let Some(results) = r.results {
                reaction_results.extend(
                    ss13_atmos::reactions::reacted_ids(results.reactions)
                        .map(|id| reaction_name(id).to_owned()),
                );
                if results.overpressure {
                    reaction_results.push("Tank overpressurized before reaction".to_owned());
                }
            }
            DopplerRecord {
                id: r.number,
                name: r.name(),
                timestamp: r.timestamp.clone(),
                coordinates: r.coordinates.clone(),
                displacement: r.displacement,
                factual: r.factual,
                theory: r.theory,
                reaction_results,
            }
        })
        .collect();
    let view = DopplerArrayData {
        disk: d.disk.map(|disk| crate::chem::name_of(world, disk)),
        storage: storage(world, d.disk),
        records,
    };
    world.entity_mut(e).insert((d, DopplerView(view)));
}

/// `/datum/gas_reaction` `name` by tg id.
pub fn reaction_name(id: &str) -> &str {
    match id {
        "vapor" => "Water Vapor Condensation",
        "sterilization" => "Dry Heat Sterilization",
        "plasmafire" => "Plasma Combustion",
        "h2fire" => "Hydrogen Combustion",
        "tritfire" => "Tritium Combustion",
        "freonfire" => "Freon Combustion",
        "nitrousformation" => "Nitrous Oxide Formation",
        "nitrous_decomp" => "Nitrous Oxide Decomposition",
        "bzformation" => "BZ Gas Formation",
        "pluox_formation" => "Pluoxium Formation",
        "nitrium_formation" => "Nitrium Formation",
        "nitrium_decomp" => "Nitrium Decomposition",
        "freonformation" => "Freon Formation",
        "nobformation" => "Hyper-Noblium Condensation",
        "halon_o2removal" => "Halon Oxygen Absorption",
        "healium_formation" => "Healium Formation",
        "zauker_formation" => "Zauker Formation",
        "zauker_decomp" => "Zauker Decomposition",
        "proto_nitrate_formation" => "Proto-Nitrate Formation",
        "proto_nitrate_hydrogen_response" => "Proto-Nitrate Hydrogen Response",
        "proto_nitrate_tritium_response" => "Proto-Nitrate Tritium Response",
        "proto_nitrate_bz_response" => "Proto-Nitrate BZ Response",
        "antinoblium_replication" => "Anti-Noblium Replication",
        other => other,
    }
}

/// The arguments of `COMSIG_GLOB_EXPLOSION`.
#[derive(Clone, Debug, PartialEq)]
pub struct SensedExplosion {
    pub epicenter: TilePos,
    /// Devastation, heavy and light range after the cap.
    pub ranges: [f32; 3],
    /// Before the cap.
    pub orig_ranges: [f32; 3],
    /// Seconds the explosion took to compute.
    pub took: f64,
    pub source: Option<Entity>,
    pub index: u32,
}

/// `sense_explosion` of every doppler array.
// Cost: one pass over the doppler arrays (a few per map) per explosion.
pub fn sense_explosion(world: &mut World, ex: &SensedExplosion) {
    // `istype(explosion_cause, /obj/item/tank)`: the tank's `explosion_information()`.
    let results = ex
        .source
        .filter(|&s| world.get::<TankAir>(s).is_some())
        .map(|s| world.get::<TankResults>(s).copied().unwrap_or_default());
    let mut q = world.query::<(Entity, &Doppler)>();
    let arrays: Vec<Entity> = q.iter(world).map(|(e, _)| e).collect();
    for e in arrays {
        if world.get::<Unpowered>(e).is_some() || panel_open(world, e) {
            continue;
        }
        let Some(zone) = crate::explosions::turf_of(world, e) else {
            continue;
        };
        if zone.z != ex.epicenter.z {
            continue;
        }
        let Some(mut d) = doppler(world, e) else {
            continue;
        };
        let time = now(world);
        if d.next_announce > time {
            continue;
        }
        d.next_announce = time + DOPPLER_COOLDOWN;
        let dir = facing(world, e) as u8;
        let far = zone
            .chebyshev(ex.epicenter)
            .is_none_or(|dist| dist > MAX_DIST);
        if far || get_dir(zone, ex.epicenter) & dir == 0 {
            world.entity_mut(e).insert(d);
            continue;
        }
        let [dev, heavy, light] = ex.ranges;
        let [odev, oheavy, olight] = ex.orig_ranges;
        let capped = dev < odev || heavy < oheavy || light < olight;
        let stamp = crate::records::manifest::round_timestamp(world);
        let record = TachyonRecord {
            number: d.record_number,
            timestamp: format!("{stamp} (PT: {stamp})"),
            coordinates: format!("{}, {}", ex.epicenter.x, ex.epicenter.y),
            displacement: ex.took,
            factual: ex.ranges,
            theory: capped.then_some(ex.orig_ranges),
            results,
            explosion_index: ex.index,
        };
        let f = |n: f32| dm_num(f64::from(n));
        let mut messages = vec![
            "Explosive disturbance detected.".to_owned(),
            format!(
                "Epicenter at: grid ({}, {}). Temporal displacement of tachyons: {} seconds.",
                ex.epicenter.x,
                ex.epicenter.y,
                dm_num(ex.took)
            ),
            format!(
                "Factual: Epicenter radius: {}. Outer radius: {}. Shockwave radius: {}.",
                f(dev),
                f(heavy),
                f(light)
            ),
        ];
        if capped {
            messages.push(format!(
                "Theoretical: Epicenter radius: {}. Outer radius: {}. Shockwave radius: {}.",
                f(odev),
                f(oheavy),
                f(olight)
            ));
        }
        d.record_number += 1;
        d.records.push(record);
        set_doppler(world, e, d);
        for m in messages {
            say(world, e, &m);
        }
    }
}

/// `apply_experiments(record)`: the explosive experiments the record qualifies for, with the
/// shockwave radius (theoretical when capped) as the score.
pub fn explosive_experiments(record: &TachyonRecord) -> Vec<(String, f64)> {
    let radius = record.theory.unwrap_or(record.factual);
    let score = f64::from(radius[2]);
    if score == 0.0 {
        return Vec::new();
    }
    let mut passed = Vec::new();
    for e in &EXPERIMENTS {
        let Requirement::Explosive {
            reactions,
            sanitized_reactions,
            require_all,
            sanitized_misc,
            allow_any_source,
        } = e.requirement
        else {
            continue;
        };
        let Some(results) = record.results else {
            // No reaction results: only the experiments that are always fair game.
            if allow_any_source {
                passed.push((e.path.to_owned(), score));
            }
            continue;
        };
        let happened: Vec<&str> = ss13_atmos::reactions::reacted_ids(results.reactions).collect();
        let fitting = reactions.iter().filter(|r| happened.contains(*r)).count();
        let missing = reactions.len() - fitting;
        let extra = happened.iter().filter(|r| !reactions.contains(*r)).count();
        if sanitized_reactions && extra > 0 {
            continue;
        }
        if sanitized_misc && results.overpressure {
            continue;
        }
        if require_all && missing > 0 {
            continue;
        }
        if !require_all && !reactions.is_empty() && fitting == 0 {
            continue;
        }
        passed.push((e.path.to_owned(), score));
    }
    passed
}

fn doppler_print(world: &mut World, e: Entity, d: &Doppler, number: u32) {
    let (Some(record), Some(disk)) = (d.records.iter().find(|r| r.number == number), d.disk) else {
        return;
    };
    let file = OrdnanceFile {
        uid: next_uid(),
        filename: format!("Doppler Array {}", record.name()),
        possible: explosive_experiments(record),
        record: OrdnanceRecord::Explosion(record.explosion_index),
    };
    let added = world
        .get_mut::<ComputerDisk>(disk)
        .is_some_and(|mut disk| disk.add_data(file));
    sound(world, e, if added { PING } else { ERROR }, 25.0);
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DopplerAct {
    Delete(u32),
    Save(u32),
    EjectDisk,
}

impl ServerUi for DopplerArrayUi {
    type Source = &'static DopplerView;
    type UserParam = ();
    type Act = DopplerAct;

    fn static_data(_: &DopplerView) {}

    fn data(view: &DopplerView, _: Entity, _: &()) -> DopplerArrayData {
        view.0.clone()
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<DopplerAct, UiReject> {
        Ok(match action {
            acts::DELETE_RECORD => DopplerAct::Delete(record_ref(params)?),
            acts::SAVE_RECORD => DopplerAct::Save(record_ref(params)?),
            acts::EJECT_DISK => DopplerAct::EjectDisk,
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

fn record_ref(params: &UiParams) -> Result<u32, UiReject> {
    params
        .get(acts::REF)
        .and_then(|r| r.parse().ok())
        .ok_or_else(|| UiReject::BadParams("ref: expected a record".into()))
}

/// `ui_act`: records looked up in the machine's own list (`locate(ref) in records`).
pub fn apply_doppler_act(world: &mut World, actor: Entity, e: Entity, act: DopplerAct) {
    let Some(mut d) = doppler(world, e) else {
        return;
    };
    match act {
        DopplerAct::Delete(n) => d.records.retain(|r| r.number != n),
        DopplerAct::Save(n) => doppler_print(world, e, &d, n),
        DopplerAct::EjectDisk => {
            if let Some(disk) = d.disk.take() {
                eject_item(world, disk, e, Some(actor));
                sound(world, e, CARD_SLIDE, 50.0);
            }
        }
    }
    set_doppler(world, e, d);
}

fn doppler_acts(world: &mut World, mut cursor: Local<MessageCursor<UiActed<DopplerArrayUi>>>) {
    let pending: Vec<(Entity, Entity, DopplerAct)> = cursor
        .read(world.resource::<Messages<UiActed<DopplerArrayUi>>>())
        .map(|a| (a.actor, a.source, a.act))
        .collect();
    for (actor, e, act) in pending {
        apply_doppler_act(world, actor, e, act);
    }
}

/// `simple_rotation` (alt-click): counterclockwise while unanchored. True for a doppler.
pub fn click_alt(world: &mut World, client: Entity, target: Entity) -> bool {
    if doppler(world, target).is_none() {
        return false;
    }
    if crate::obj_damage::anchored(world, target) {
        crate::tools::tell(world, client, Span::Warning, "Он прикручен к полу!");
        return true;
    }
    let ccw = match facing(world, target) {
        Dir::North => Dir::West,
        Dir::West => Dir::South,
        Dir::South => Dir::East,
        _ => Dir::North,
    };
    world.entity_mut(target).insert(Facing(ccw));
    true
}

// ---------------------------------------------------------------------------------------------
// Tank compressor

/// The compressor state of `e` (given on first use), with what left it forgotten.
pub fn compressor(world: &mut World, e: Entity) -> Option<Compressor> {
    if let Some(c) = world.get::<Compressor>(e) {
        let mut c = c.clone();
        c.tank = inside(world, c.tank, e);
        c.disk = inside(world, c.disk, e);
        return Some(c);
    }
    if !path_of(world, e).is_some_and(|p| is_under(&p, COMPRESSOR)) {
        return None;
    }
    let c = Compressor::default();
    world
        .entity_mut(e)
        .insert((c.clone(), CompressorView::default()));
    Some(c)
}

fn compressor_view(world: &World, c: &Compressor) -> TankCompressorData {
    let tank = c.tank.and_then(|t| world.get::<TankAir>(t).map(|a| (t, a)));
    TankCompressorData {
        tank_present: c.tank.is_some(),
        tank_pressure: tank.map(|(_, a)| a.pressure()),
        leaking: tank.is_some_and(|(t, _)| world.get::<Leaking>(t).is_some()),
        active: c.active,
        transfer_rate: c.transfer_rate,
        last_pressure: c.last_pressure,
        disk: c.disk.map(|d| crate::chem::name_of(world, d)),
        storage: storage(world, c.disk),
        records: c
            .records
            .iter()
            .map(|r| CompressorRecordUi {
                id: r.id,
                name: r.name.clone(),
                source: r.source.clone(),
                timestamp: r.timestamp.clone(),
                gases: r
                    .gases
                    .iter()
                    .map(|(g, m)| (GASES[usize::from(g.0)].name.to_owned(), *m))
                    .collect(),
            })
            .collect(),
    }
}

fn set_compressor(world: &mut World, e: Entity, c: Compressor) {
    let view = compressor_view(world, &c);
    world.entity_mut(e).insert(c);
    match world.get_mut::<CompressorView>(e) {
        Some(mut v) => {
            v.set_if_neq(CompressorView(view));
        }
        None => {
            world.entity_mut(e).insert(CompressorView(view));
        }
    }
}

/// The compressor holding `tank`, if any.
pub fn compressor_of(world: &World, tank: Entity) -> Option<Entity> {
    let holder = world.get::<StoredIn>(tank)?.0;
    world
        .get::<Compressor>(holder)
        .is_some_and(|c| c.tank == Some(tank))
        .then_some(holder)
}

/// `assume_air`: gas from the held tank into the buffer.
pub fn assume_air(world: &mut World, e: Entity, gas: &GasMixture) {
    if let Some(mut c) = world.get_mut::<Compressor>(e) {
        c.buffer.merge(gas);
    }
}

/// `explosion_handle` (`COMSIG_ATOM_INTERNAL_EXPLOSION`): absorbed.
pub fn absorb_explosion(world: &mut World, e: Entity) {
    say(world, e, "Internal explosion detected and absorbed.");
}

/// `tank_destruction`: the held tank was destroyed with `pressure` in it.
pub fn tank_destroyed(world: &mut World, e: Entity, tank: Entity, pressure: f64) {
    let Some(mut c) = compressor(world, e) else {
        return;
    };
    flush_buffer(world, e, &mut c, Some(tank));
    c.last_pressure = pressure;
    c.active = false;
    c.tank = None;
    set_compressor(world, e, c);
}

/// `flush_buffer`: a significant buffer is recorded; then it goes out of the output port.
fn flush_buffer(world: &mut World, e: Entity, c: &mut Compressor, tank: Option<Entity>) {
    let moles = c.buffer.total_moles();
    if moles == 0.0 {
        return;
    }
    if moles > SIGNIFICANT_MOLES {
        // `record_data()`.
        let stamp = crate::records::manifest::round_timestamp(world);
        let gases = c
            .buffer
            .moles
            .iter()
            .enumerate()
            .filter(|(_, m)| **m > 0.0)
            .map(|(i, m)| (GasId(i as u8), *m))
            .collect();
        c.records.push(CompressorRecord {
            id: next_uid(),
            name: format!("Log Recording #{}", c.record_number),
            source: tank.map_or_else(String::new, |t| crate::chem::name_of(world, t)),
            timestamp: format!("{stamp} (PT: {stamp})"),
            gases,
        });
        c.record_number += 1;
        say(world, e, "Buffer data stored.");
    } else {
        say(
            world,
            e,
            &format!(
                "Buffer data discarded. Required moles for storage: {} moles.",
                dm_num(SIGNIFICANT_MOLES)
            ),
        );
    }
    let removed = std::mem::take(&mut c.buffer);
    // `airs[1]`: the output port.
    let device = world.get::<AtmosDevice>(e).map(|d| d.id);
    if let Some(id) = device
        && let Some(mut net) = world.get_resource_mut::<Pipenet>()
        && let Some(out) = net.air_mut(id, 0)
    {
        out.merge(&removed);
        net.update_parents(id);
    }
    say(
        world,
        e,
        "Gas stored in buffer flushed to output port. Compressor ready to start the next \
         experiment.",
    );
}

/// `apply_experiments(record)`: the gaseous experiments whose gas the record holds.
pub fn gaseous_experiments(record: &CompressorRecord) -> Vec<(String, f64)> {
    EXPERIMENTS
        .iter()
        .filter_map(|e| match e.requirement {
            Requirement::Gaseous(gas) => record
                .gases
                .iter()
                .find(|(g, m)| *g == gas && *m > MINIMUM_MOLE_COUNT)
                .map(|(_, m)| (e.path.to_owned(), *m)),
            Requirement::Explosive { .. } => None,
        })
        .collect()
}

fn compressor_print(world: &mut World, e: Entity, c: &Compressor, id: u32) {
    let (Some(record), Some(disk)) = (c.records.iter().find(|r| r.id == id), c.disk) else {
        return;
    };
    let file = OrdnanceFile {
        uid: next_uid(),
        filename: format!("Gas Compressor {}", record.name),
        possible: gaseous_experiments(record),
        record: OrdnanceRecord::Compressor(record.id),
    };
    let added = world
        .get_mut::<ComputerDisk>(disk)
        .is_some_and(|mut disk| disk.add_data(file));
    sound(world, e, if added { PING } else { ERROR }, 25.0);
}

/// `eject_tank(user)`: not over the eject pressure; the buffer is flushed first.
fn eject_tank(world: &mut World, e: Entity, c: &mut Compressor, user: Option<Entity>) -> bool {
    let Some(tank) = c.tank else {
        return false;
    };
    let pressure = world.get::<TankAir>(tank).map_or(0.0, TankAir::pressure);
    if pressure >= EJECT_PRESSURE {
        return false;
    }
    flush_buffer(world, e, c, Some(tank));
    eject_item(world, tank, e, user);
    c.tank = None;
    c.active = false;
    true
}

/// `process_atmos`: a glorified volume pump from the input port into the tank.
// Cost: one pass over the compressors (one or two per map) per SSair tick.
fn process_compressors(world: &mut World) {
    let mut q = world.query::<(Entity, &Compressor, &AtmosDevice)>();
    let running: Vec<(Entity, Entity, f64, ss13_atmos::pipenet::DeviceId)> = q
        .iter(world)
        .filter(|(_, c, _)| c.active && c.transfer_rate > 0.0)
        .filter_map(|(e, c, d)| Some((e, c.tank?, c.transfer_rate, d.id)))
        .collect();
    for (e, tank, rate, id) in running {
        if world.get::<StoredIn>(tank).is_none_or(|s| s.0 != e) {
            continue;
        }
        let Some(tank_pressure) = world.get::<TankAir>(tank).map(TankAir::pressure) else {
            continue;
        };
        // "Prevent pumping if tank is taking damage but still below pressure limit."
        let stalled = world.get::<Leaking>(tank).is_some() && tank_pressure <= TANK_LEAK_PRESSURE;
        let removed = {
            let Some(mut net) = world.get_resource_mut::<Pipenet>() else {
                return;
            };
            let Some(input) = net.air_mut(id, 1) else {
                continue;
            };
            if input.total_moles() == 0.0
                || input.pressure() < 0.01
                || tank_pressure > PRESSURE_LIMIT
            {
                continue;
            }
            if stalled {
                None
            } else {
                let volume = input.volume;
                let removed = input.remove_ratio(rate / volume);
                net.update_parents(id);
                Some(removed)
            }
        };
        let Some(removed) = removed else {
            if let Some(mut c) = world.get_mut::<Compressor>(e) {
                c.active = false;
            }
            continue;
        };
        if removed.total_moles() == 0.0 {
            continue;
        }
        if let Some(mut t) = world.get_mut::<TankAir>(tank) {
            t.air.merge(&removed);
        }
        note_merge(world, tank);
    }
}

/// The window kept in step with the tank's pressure.
fn refresh_compressor_views(world: &mut World) {
    let mut q = world.query::<(Entity, &Compressor)>();
    let machines: Vec<Entity> = q
        .iter(world)
        .filter(|(_, c)| c.tank.is_some())
        .map(|(e, _)| e)
        .collect();
    for e in machines {
        if let Some(c) = compressor(world, e) {
            set_compressor(world, e, c);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CompressorAct {
    ChangeRate(f64),
    ToggleInjection,
    EjectTank,
    EjectDisk,
    Delete(u32),
    Save(u32),
}

impl ServerUi for TankCompressorUi {
    type Source = &'static CompressorView;
    type UserParam = ();
    type Act = CompressorAct;

    fn static_data(_: &CompressorView) -> TankCompressorStatic {
        TankCompressorStatic {
            max_transfer: MAX_TRANSFER_RATE,
            leak_pressure: TANK_LEAK_PRESSURE.round(),
            fragment_pressure: TANK_FRAGMENT_PRESSURE.round(),
            eject_pressure: EJECT_PRESSURE,
        }
    }

    fn data(view: &CompressorView, _: Entity, _: &()) -> TankCompressorData {
        view.0.clone()
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<CompressorAct, UiReject> {
        Ok(match action {
            acts::CHANGE_RATE => CompressorAct::ChangeRate(params.number(acts::TARGET)?),
            acts::TOGGLE_INJECTION => CompressorAct::ToggleInjection,
            acts::EJECT_TANK => CompressorAct::EjectTank,
            acts::EJECT_DISK => CompressorAct::EjectDisk,
            acts::DELETE_RECORD => CompressorAct::Delete(record_ref(params)?),
            acts::SAVE_RECORD => CompressorAct::Save(record_ref(params)?),
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

/// `ui_act`, every check at the moment of effect.
pub fn apply_compressor_act(world: &mut World, actor: Entity, e: Entity, act: CompressorAct) {
    let Some(mut c) = compressor(world, e) else {
        return;
    };
    match act {
        CompressorAct::ChangeRate(rate) => c.transfer_rate = rate.clamp(0.0, MAX_TRANSFER_RATE),
        CompressorAct::ToggleInjection => c.active = !c.active,
        CompressorAct::EjectTank => {
            eject_tank(world, e, &mut c, Some(actor));
        }
        CompressorAct::EjectDisk => {
            if let Some(disk) = c.disk.take() {
                eject_item(world, disk, e, Some(actor));
                sound(world, e, CARD_SLIDE, 50.0);
            }
        }
        CompressorAct::Delete(id) => c.records.retain(|r| r.id != id),
        CompressorAct::Save(id) => compressor_print(world, e, &c, id),
    }
    set_compressor(world, e, c);
}

fn compressor_acts(world: &mut World, mut cursor: Local<MessageCursor<UiActed<TankCompressorUi>>>) {
    let pending: Vec<(Entity, Entity, CompressorAct)> = cursor
        .read(world.resource::<Messages<UiActed<TankCompressorUi>>>())
        .map(|a| (a.actor, a.source, a.act))
        .collect();
    for (actor, e, act) in pending {
        apply_compressor_act(world, actor, e, act);
    }
}

// ---------------------------------------------------------------------------------------------
// Clicks

/// An empty hand: the machine's window.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if let Some(d) = doppler(world, target) {
        set_doppler(world, target, d);
        let title = crate::chem::name_of(world, target);
        world.write_message(OpenUi::new::<DopplerArrayUi>(client, mob, target, title));
        return true;
    }
    if let Some(c) = compressor(world, target) {
        set_compressor(world, target, c);
        let title = crate::chem::name_of(world, target);
        world.write_message(OpenUi::new::<TankCompressorUi>(client, mob, target, title));
        return true;
    }
    false
}

/// `item_interaction`: disks into both, tanks into the compressor, a wrench on the doppler.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if let Some(mut d) = doppler(world, target) {
        if is_disk(world, held) {
            if let Some(old) = d.disk.take() {
                eject_item(world, old, target, Some(mob));
                sound(world, target, CARD_SLIDE, 50.0);
            }
            put_in(world, held, target);
            d.disk = Some(held);
            set_doppler(world, target, d);
            return true;
        }
        let combat = crate::medical::surgery::combat_mode(world, mob);
        if !combat && tool_of(world, held).is_some_and(|(kind, _)| kind == ToolKind::Wrench) {
            let anchored = world.get::<Anchored>(target).map_or_else(
                || world.get::<DmObj>(target).is_some_and(|o| o.anchored),
                |a| a.0,
            );
            sound(world, target, RATCHET, 50.0);
            use_tool(
                world,
                Actor {
                    client,
                    mob,
                    item: Some(held),
                },
                target,
                WRENCH_DS,
                0,
                crate::construction::Job::Unfasten { anchored },
            );
            return true;
        }
        return false;
    }
    let Some(mut c) = compressor(world, target) else {
        return false;
    };
    if crate::medical::surgery::combat_mode(world, mob) || panel_open(world, target) {
        return false;
    }
    if is_disk(world, held) {
        if let Some(old) = c.disk.take() {
            eject_item(world, old, target, Some(mob));
            sound(world, target, CARD_SLIDE, 50.0);
        }
        put_in(world, held, target);
        c.disk = Some(held);
        set_compressor(world, target, c);
        return true;
    }
    if !path_of(world, held).is_some_and(|p| is_under(&p, TANK))
        || world.get::<TankAir>(held).is_none()
    {
        return false;
    }
    if c.tank.is_some() && !eject_tank(world, target, &mut c, Some(mob)) {
        crate::balloon::balloon_alert(world, mob, target, "застрял внутри!");
        set_compressor(world, target, c);
        return true;
    }
    put_in(world, held, target);
    c.tank = Some(held);
    c.last_pressure = 0.0;
    set_compressor(world, target, c);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(theory: Option<[f32; 3]>, results: Option<TankResults>) -> TachyonRecord {
        TachyonRecord {
            number: 1,
            timestamp: String::new(),
            coordinates: String::new(),
            displacement: 0.0,
            factual: [2.0, 4.0, 8.0],
            theory,
            results,
            explosion_index: 1,
        }
    }

    fn mask(ids: &[&str]) -> u32 {
        ss13_atmos::reactions::reaction_ids()
            .enumerate()
            .filter(|(_, id)| ids.contains(id))
            .fold(0, |m, (i, _)| m | 1 << i)
    }

    fn names(passed: &[(String, f64)]) -> Vec<&str> {
        passed
            .iter()
            .map(|(p, _)| p.rsplit('/').next().unwrap())
            .collect()
    }

    #[test]
    fn explosive_experiments_filter_by_reactions() {
        // Not a tank: only the experiments that take any source.
        let plain = explosive_experiments(&record(None, None));
        assert_eq!(names(&plain), vec!["lowyieldbomb"]);
        assert_eq!(plain[0].1, 8.0);
        // A capped bomb scores its theoretical radius.
        let capped = explosive_experiments(&record(Some([5.0, 10.0, 20.0]), None));
        assert_eq!(capped[0].1, 20.0);
        // A hydrogen fire.
        let h2 = TankResults {
            reactions: mask(&["h2fire"]),
            overpressure: false,
        };
        assert_eq!(
            names(&explosive_experiments(&record(None, Some(h2)))),
            vec!["lowyieldbomb", "highyieldbomb", "hydrogenbomb"]
        );
        // No reactions at all: the pressure bomb.
        let none = TankResults::default();
        assert_eq!(
            names(&explosive_experiments(&record(None, Some(none)))),
            vec!["lowyieldbomb", "highyieldbomb", "pressurebomb"]
        );
        // Overpressure from merging spoils the sanitized ones.
        let over = TankResults {
            reactions: mask(&["h2fire", "plasmafire"]),
            overpressure: true,
        };
        assert_eq!(
            names(&explosive_experiments(&record(None, Some(over)))),
            vec!["lowyieldbomb", "highyieldbomb"]
        );
        let nob = TankResults {
            reactions: mask(&["nobformation"]),
            overpressure: false,
        };
        assert!(names(&explosive_experiments(&record(None, Some(nob)))).contains(&"nobliumbomb"));
    }

    #[test]
    fn gaseous_experiments_need_the_gas() {
        let r = CompressorRecord {
            id: 1,
            name: String::new(),
            source: String::new(),
            timestamp: String::new(),
            gases: vec![(GasId::PLASMA, 300.0), (GasId::N2O, 0.005)],
        };
        assert_eq!(
            gaseous_experiments(&r),
            vec![(
                "/datum/experiment/ordnance/gaseous/plasma".to_owned(),
                300.0
            )]
        );
    }

    #[test]
    fn get_dir_is_byond_get_dir() {
        let at = |x, y| TilePos::new(x, y, 1);
        assert_eq!(get_dir(at(5, 5), at(5, 9)), 1);
        assert_eq!(get_dir(at(5, 5), at(9, 1)), 4 | 2);
        assert_eq!(get_dir(at(5, 5), at(5, 5)), 0);
        assert_eq!(reaction_name("nobformation"), "Hyper-Noblium Condensation");
    }
}
