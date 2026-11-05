//! Tile atmospherics after tg LINDA (`code/modules/atmospherics/environmental/`): every open
//! turf has a gas mixture; turfs whose air differs from a neighbour's are active and share
//! air each SSair fire (0.5 s), grouped into excited groups that average out and go to sleep.
//!
//! Per z-level state lives in [`Atmos`] ([`LevelAir`]): dense per-tile arrays, a compact cell
//! list for open turfs, the active list and the groups. A fire costs O(active turfs + live
//! groups); a stable station costs nothing. Levels are independent and fire in parallel.
//!
//! Inputs from the world: [`TurfAir`] on turf entities (read once at startup) and
//! [`AtmosBlocker`] on objects that can block air (doors, windows, ...), whose density
//! changes re-run adjacency of their tile (tg `air_update_turf`). Outputs: the per-tile query
//! API of [`Atmos`], [`PressureDifference`] messages (spacewind input), [`GasOverlay`] entities
//! for visible gases and [`scan::scan_message`] for analyzers.
//!
//! Gas reactions ([`reactions`]) run on every processed turf; fires ([`level::hotspot`]) are
//! lit with [`Atmos::hotspot_expose`], drawn as [`HotspotEffect`] entities (tg's sprite and
//! light) and burn what is on their tile through [`FireAct`] messages.

pub mod electrolysis;
pub mod gas;
pub mod level;
pub mod pipenet;
pub mod planetary;
pub mod reactions;
pub mod scan;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::world::DeferredWorld;
use bevy::platform::time::Instant;
use bevy::prelude::*;
use bevy::tasks::{ComputeTaskPool, TaskPool};
use ss13_shared::chat::ChatMessage;
use ss13_shared::{
    Appearance, Dir, Facing, IconStateId, Layer, LightSource, MouseOpacity, Optics, Overlay,
    TilePos,
};
use ss13_world::{Border, Density, SpatialIndex, WorldSystems};

pub use gas::{GAS_COUNT, GASES, GasId, GasInfo, GasMixture, ParseError};
pub use level::{
    FireExposure, FireStage, Hotspot, LevelAir, PressureDelta, SetupReport, Thermal, TileKind,
};

/// tg `SSair.wait`.
pub const FIRE_INTERVAL: f64 = 0.5;
/// `FLY_LAYER` and `ABOVE_GAME_PLANE` of `/obj/effect/overlay/gas`.
pub const GAS_OVERLAY_LAYER: f32 = 5.0;
pub const GAS_OVERLAY_PLANE: i16 = -3;

/// A turf's air, as the map defines it. Read once when atmos starts.
// Construction changes turfs through `Atmos::change_turf`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct TurfAir {
    pub kind: TileKind,
    /// tg `initial_gas_mix`.
    pub initial_gas_mix: Arc<str>,
    /// tg `create_gas_mixture`: the turf's `temperature` when its type or a map edit changes it.
    pub temperature: Option<f64>,
    /// `planetary_atmos`: relaxes to its initial mix.
    pub planetary: bool,
    /// The turf's own heat for superconduction.
    pub thermal: Thermal,
}

/// tg `can_atmos_pass` of an object other than `ATMOS_PASS_YES`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtmosPass {
    /// `ATMOS_PASS_NO`: always airtight.
    No,
    /// `ATMOS_PASS_DENSITY`: airtight while dense (doors, false walls).
    Density,
    /// `ATMOS_PASS_PROC`: windows, windoors and directional firelocks; while dense, a border
    /// object (`ss13_world::Border`) blocks the edges it faces, anything else (full-tile
    /// windows) the whole tile.
    Proc,
}

/// On objects that may block air. Changes of it, of `Density` and of `Border` on such an
/// entity recompute its tile's adjacency.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[component(on_insert = blocker_inserted, on_discard = blocker_discarded)]
pub struct AtmosBlocker(pub AtmosPass);

impl AtmosBlocker {
    /// Cardinal edges (BYOND dir bits) of its tile it blocks now.
    pub fn edges(self, dense: bool, border: Option<Border>) -> u8 {
        match (self.0, dense, border) {
            (AtmosPass::No, ..) => 0xF,
            (_, false, _) => 0,
            (AtmosPass::Density, true, _) => 0xF,
            (AtmosPass::Proc, true, Some(b)) => b.edges & 0xF,
            (AtmosPass::Proc, true, None) => 0xF,
        }
    }
}

/// On an [`AtmosBlocker`] that also stops heat while it blocks air (tg
/// `block_superconductivity`: opaque or heat-proof doors, plasma windows, atmos holobarriers).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlocksHeat;

/// Icon states of the gas overlays (`icons/effects/atmospherics.dmi`), by gas.
#[derive(Resource, Clone, Debug, Default)]
pub struct GasOverlayIcons(pub [Option<IconStateId>; GAS_COUNT]);

/// The entity drawing the visible gases of one tile (tg `atmos_overlay_types` in the turf's
/// `vis_contents`).
#[derive(Component, Clone, Copy, Debug)]
pub struct GasOverlay;

/// `GASFIRE_LAYER` and `ABOVE_GAME_PLANE` of `/obj/effect/hotspot`.
pub const HOTSPOT_LAYER: f32 = 5.05;
/// `LIGHT_RANGE_FIRE`, `light_power` of a hotspot.
pub const HOTSPOT_LIGHT_RANGE: f32 = 3.0;
pub const HOTSPOT_LIGHT_POWER: f32 = 1.0;

/// Icon states of hotspots by [`FireStage`] (`icons/effects/fire.dmi`), and BandaStation's
/// additive overlay of each stage (`modular_bandastation/aesthetics/fire/icons/fire.dmi`).
#[derive(Resource, Clone, Debug, Default)]
pub struct HotspotIcons {
    pub stages: [Option<IconStateId>; 3],
    pub glow: [Option<IconStateId>; 3],
}

/// The entity drawing one tile's hotspot.
#[derive(Component, Clone, Copy, Debug)]
pub struct HotspotEffect;

/// tg `fire_act(exposed_temperature, exposed_volume)` of everything on `pos`, from a hotspot's
/// exposure.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct FireAct {
    pub pos: TilePos,
    pub temperature: f64,
    pub volume: f64,
}

/// tg `high_pressure_delta`: a turf that pushed or lost air this fire, for spacewind.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct PressureDifference {
    pub pos: TilePos,
    /// kPa.
    pub difference: f64,
    /// Towards the lower pressure.
    pub dir: Dir,
}

/// SSair's order inside one fire (tg `SSair.fire`): pipenets, then atmos machines, then turfs.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum AtmosSystems {
    /// Decides whether this frame is an SSair fire ([`AirTick`]).
    Tick,
    /// `process_pipenets` ([`pipenet::Pipenet::process`]).
    Pipenets,
    /// `process_atmos_machinery`: game systems that run on [`AirTick::due`] frames.
    Machinery,
    /// Blocker changes, the turf fire, then visuals.
    Fire,
}

/// Whether this frame is an SSair fire (every [`FIRE_INTERVAL`] seconds). Machines that act
/// once per fire (tg `process_atmos`) run in [`AtmosSystems::Machinery`] under
/// [`air_tick_due`].
#[derive(Resource, Default, Debug)]
pub struct AirTick {
    pub due: bool,
    since: f64,
    /// Fires so far.
    pub fires: u64,
}

/// Run condition: this frame is an SSair fire.
pub fn air_tick_due(tick: Res<AirTick>) -> bool {
    tick.due
}

/// All tile air. Empty until the first `PostStartup` after the map loaded.
#[derive(Resource, Default)]
pub struct Atmos {
    levels: Vec<Option<LevelAir>>,
    initialized: bool,
    /// Tiles whose blockers changed, with tg's `remove` flag (something closed).
    dirty: Vec<(TilePos, bool)>,
    /// Fires so far.
    pub fires: u64,
    /// CPU time of the last fire.
    pub last_fire: Duration,
    /// CPU time of each level in the last fire, by z (they run in parallel).
    pub last_fire_levels: Vec<(u16, Duration)>,
    /// What each level's setup found, by z.
    pub setup: Vec<(u16, SetupReport)>,
    /// `SSair.atmos_gen`: this round's gas string per planetary atmosphere id.
    pub atmos_gen: Vec<(&'static str, String)>,
    /// Tiles air moves between across levels (tg `zAirOut`/`zAirIn`): `(upper, lower)`, the
    /// upper one open space. Kept by the game ([`Self::set_vertical`]).
    vertical: Vec<(TilePos, TilePos)>,
}

impl Atmos {
    pub fn level(&self, z: u16) -> Option<&LevelAir> {
        self.levels.get(usize::from(z)).and_then(Option::as_ref)
    }

    /// Every level with its z.
    pub fn levels(&self) -> impl Iterator<Item = (u16, &LevelAir)> {
        self.levels
            .iter()
            .enumerate()
            .filter_map(|(z, l)| l.as_ref().map(|l| (z as u16, l)))
    }

    pub fn level_mut(&mut self, z: u16) -> Option<&mut LevelAir> {
        self.levels.get_mut(usize::from(z)).and_then(Option::as_mut)
    }

    /// Adds a level built by hand (tests, tools); [`AtmosPlugin`] builds them from the map.
    pub fn insert_level(&mut self, z: u16, level: LevelAir) {
        let z = usize::from(z);
        if self.levels.len() <= z {
            self.levels.resize_with(z + 1, || None);
        }
        self.levels[z] = Some(level);
        self.initialized = true;
    }

    fn locate(&self, pos: TilePos) -> Option<(&LevelAir, u32)> {
        let level = self.level(pos.z)?;
        Some((level, level.tile(pos.x, pos.y)?))
    }

    fn locate_mut(&mut self, pos: TilePos) -> Option<(&mut LevelAir, u32)> {
        let level = self.level_mut(pos.z)?;
        let tile = level.tile(pos.x, pos.y)?;
        Some((level, tile))
    }

    /// The air on a tile: its own, the vacuum in space, `None` in walls and off the map.
    pub fn air(&self, pos: TilePos) -> Option<&GasMixture> {
        let (level, tile) = self.locate(pos)?;
        level.air(tile)
    }

    /// A wall's (or any airless turf's) own temperature and heat capacity; `None` where the
    /// tile has air and off the map.
    pub fn solid_heat(&self, pos: TilePos) -> Option<(f64, f64)> {
        let (level, tile) = self.locate(pos)?;
        level.solid_heat(tile)
    }

    /// `/turf/TakeTemperature` of an airless turf; it is exposed to its new temperature like a
    /// superconducting one. False where the tile has air.
    pub fn take_solid_temperature(&mut self, pos: TilePos, delta: f64) -> bool {
        self.locate_mut(pos)
            .is_some_and(|(level, tile)| level.take_solid_temperature(tile, delta))
    }

    /// kPa.
    pub fn pressure(&self, pos: TilePos) -> Option<f64> {
        self.air(pos).map(GasMixture::pressure)
    }

    /// Kelvin.
    pub fn temperature(&self, pos: TilePos) -> Option<f64> {
        self.air(pos).map(|a| a.temperature)
    }

    pub fn moles(&self, pos: TilePos, gas: GasId) -> Option<f64> {
        self.air(pos).map(|a| a.get(gas))
    }

    /// kPa of one gas (what lungs check).
    pub fn partial_pressure(&self, pos: TilePos, gas: GasId) -> Option<f64> {
        self.air(pos).map(|a| a.partial_pressure(gas))
    }

    /// tg `atmos_spawn_air`: adds a gas string's mixture to an open tile.
    pub fn spawn_air(&mut self, pos: TilePos, text: &str) -> Result<bool, ParseError> {
        let giver = GasMixture::parse(text)?;
        Ok(self.merge_air(pos, &giver))
    }

    /// tg `assume_air`; false when the tile has no air of its own.
    pub fn merge_air(&mut self, pos: TilePos, giver: &GasMixture) -> bool {
        self.locate_mut(pos)
            .is_some_and(|(level, tile)| level.merge_into(tile, giver))
    }

    /// tg `remove_air`: `amount` moles out of an open tile.
    pub fn remove_air(&mut self, pos: TilePos, amount: f64) -> Option<GasMixture> {
        let (level, tile) = self.locate_mut(pos)?;
        level.remove_from(tile, amount)
    }

    /// Changes an open tile's mixture in place and wakes it (tg code that edits
    /// `return_air()` and calls `air_update_turf`); `None` in walls, space and off the map.
    pub fn modify_air<R>(
        &mut self,
        pos: TilePos,
        f: impl FnOnce(&mut GasMixture) -> R,
    ) -> Option<R> {
        let (level, tile) = self.locate_mut(pos)?;
        level.modify(tile, f)
    }

    /// tg `get_atmos_adjacent_turfs(alldir)`: open neighbours air flows to, and with `alldir`
    /// the diagonals reachable through two of them.
    pub fn adjacent_turfs(&self, pos: TilePos, alldir: bool) -> Vec<TilePos> {
        let Some((level, tile)) = self.locate(pos) else {
            return Vec::new();
        };
        let mut out: Vec<TilePos> = Dir::CARDINALS
            .into_iter()
            .filter(|&d| level.adjacent(tile, d))
            .map(|d| pos.step(d))
            .collect();
        if alldir {
            let cardinal = out.clone();
            for diag in [
                Dir::NorthEast,
                Dir::NorthWest,
                Dir::SouthEast,
                Dir::SouthWest,
            ] {
                let s = pos.step(diag);
                let Some(st) = level.tile(s.x, s.y) else {
                    continue;
                };
                let matching = Dir::CARDINALS
                    .into_iter()
                    .filter(|&d| level.adjacent(st, d) && cardinal.contains(&s.step(d)))
                    .count();
                if matching >= 2 {
                    out.push(s);
                }
            }
        }
        out
    }

    /// tg `remove_ratio` on an open tile's air (the supermatter's breath); `None` off open tiles.
    pub fn remove_ratio_air(&mut self, pos: TilePos, ratio: f64) -> Option<GasMixture> {
        let (level, tile) = self.locate_mut(pos)?;
        level.remove_ratio_from(tile, ratio)
    }

    /// The tile on `pos` or one it shares air with is space (tg: a `/turf/open/space` in
    /// `atmos_adjacent_turfs + src`).
    pub fn space_exposed(&self, pos: TilePos) -> bool {
        let Some((level, tile)) = self.locate(pos) else {
            return false;
        };
        level.kind(tile) == TileKind::Space
            || Dir::CARDINALS.into_iter().any(|dir| {
                let (dx, dy) = dir.offset();
                level.adjacent(tile, dir)
                    && level
                        .tile(pos.x + dx, pos.y + dy)
                        .is_some_and(|n| level.kind(n) == TileKind::Space)
            })
    }

    /// tg `hotspot_expose(exposed_temperature, exposed_volume, soh)` of the turf on `pos`:
    /// lights a fire where the air burns, or feeds the one there (`soh`). True when lit.
    pub fn hotspot_expose(
        &mut self,
        pos: TilePos,
        temperature: f64,
        volume: f64,
        soh: bool,
    ) -> bool {
        self.locate_mut(pos)
            .is_some_and(|(level, tile)| level.hotspot_expose(tile, temperature, volume, soh))
    }

    /// The fire on `pos` (tg `active_hotspot`).
    pub fn hotspot(&self, pos: TilePos) -> Option<&Hotspot> {
        let (level, tile) = self.locate(pos)?;
        level.hotspot(tile)
    }

    pub fn hotspots(&self) -> usize {
        self.levels
            .iter()
            .flatten()
            .map(LevelAir::hotspot_count)
            .sum()
    }

    /// Analyzer readout of a tile, `None` in walls (tg `return_analyzable_air` is null there).
    pub fn scan(&self, pos: TilePos, target: &str) -> Option<ChatMessage> {
        self.air(pos).map(|air| scan::scan_message(target, air))
    }

    pub fn active_turfs(&self) -> usize {
        self.levels
            .iter()
            .flatten()
            .map(LevelAir::active_count)
            .sum()
    }

    /// The fire sound loops of the hot groups (tg `/datum/hot_group/sound`): where each plays
    /// and its `drop_off_dist`.
    pub fn fire_sounds(&self) -> Vec<(TilePos, i32)> {
        let mut out = Vec::new();
        for (z, level) in self.levels.iter().enumerate() {
            let Some(level) = level else { continue };
            out.extend(level.fire_sounds().map(|(tile, drop_off)| {
                let (x, y) = level.pos(tile);
                (TilePos::new(x, y, z as u16), drop_off)
            }));
        }
        out
    }

    /// Turfs a heavy fire burnt since the last call (tg `burn_tile()` of the hotspot's turf).
    pub fn drain_burnt_turfs(&mut self) -> Vec<TilePos> {
        let mut out = Vec::new();
        for (z, level) in self.levels.iter_mut().enumerate() {
            let Some(level) = level else { continue };
            let mut tiles = Vec::new();
            level.drain_burns(|t| tiles.push(t));
            out.extend(tiles.into_iter().map(|t| {
                let (x, y) = level.pos(t);
                TilePos::new(x, y, z as u16)
            }));
        }
        out
    }

    /// Hot turfs that processed since the last call, with their air temperature (tg
    /// `temperature_expose` → `COMSIG_TURF_EXPOSE`, which starts atmos-sensitive atoms there).
    pub fn drain_exposed_turfs(&mut self) -> Vec<(TilePos, f64)> {
        let mut out = Vec::new();
        for (z, level) in self.levels.iter_mut().enumerate() {
            let Some(level) = level else { continue };
            let mut tiles = Vec::new();
            level.drain_exposed(|t, temperature| tiles.push((t, temperature)));
            out.extend(tiles.into_iter().map(|(t, temperature)| {
                let (x, y) = level.pos(t);
                (TilePos::new(x, y, z as u16), temperature)
            }));
        }
        out
    }

    /// Starts or stops reporting the tile on `pos` ([`LevelAir::watch`]).
    pub fn watch(&mut self, pos: TilePos, on: bool) {
        if let Some((level, tile)) = self.locate_mut(pos) {
            level.watch(tile, on);
        }
    }

    /// Watched tiles processed or with their adjacency recomputed since the last call, each
    /// once.
    pub fn drain_watch_reports(&mut self) -> Vec<TilePos> {
        let mut out = Vec::new();
        for (z, level) in self.levels.iter_mut().enumerate() {
            let Some(level) = level else { continue };
            let mut tiles = Vec::new();
            level.drain_watch_reports(|t| tiles.push(t));
            tiles.sort_unstable();
            tiles.dedup();
            out.extend(tiles.into_iter().map(|t| {
                let (x, y) = level.pos(t);
                TilePos::new(x, y, z as u16)
            }));
        }
        out
    }

    /// Turfs water vapor condensed on since the last call (tg `water_vapor_gas_act`,
    /// `freeze_turf`), for the game to wet or freeze.
    pub fn drain_condensed(&mut self) -> Vec<(TilePos, reactions::Condensation)> {
        let mut out = Vec::new();
        for (z, level) in self.levels.iter_mut().enumerate() {
            let Some(level) = level else { continue };
            let mut tiles = Vec::new();
            level.drain_condensed(|t, kind| tiles.push((t, kind)));
            out.extend(tiles.into_iter().map(|(t, kind)| {
                let (x, y) = level.pos(t);
                (TilePos::new(x, y, z as u16), kind)
            }));
        }
        out
    }

    /// Turfs the halon combustion asked resin foam for since the last call (tg `do_foam` of
    /// `/datum/effect_system/fluid_spread/foam/metal/resin/halon`), for the game to make.
    pub fn drain_resin(&mut self) -> Vec<TilePos> {
        let mut out = Vec::new();
        for (z, level) in self.levels.iter_mut().enumerate() {
            let Some(level) = level else { continue };
            let mut tiles = Vec::new();
            level.drain_resin(|t| tiles.push(t));
            out.extend(tiles.into_iter().map(|t| {
                let (x, y) = level.pos(t);
                TilePos::new(x, y, z as u16)
            }));
        }
        out
    }

    /// Radiation pulses gas reactions asked for since the last call (tg `radiation_pulse` in
    /// the tritium fire and proto-nitrate responses), for the game to send.
    pub fn drain_radiation(&mut self) -> Vec<(TilePos, reactions::RadiationRelease)> {
        let mut out = Vec::new();
        for (z, level) in self.levels.iter_mut().enumerate() {
            let Some(level) = level else { continue };
            let mut tiles = Vec::new();
            level.drain_radiation(|t, pulse| tiles.push((t, pulse)));
            out.extend(tiles.into_iter().map(|(t, pulse)| {
                let (x, y) = level.pos(t);
                (TilePos::new(x, y, z as u16), pulse)
            }));
        }
        out
    }

    pub fn excited_groups(&self) -> usize {
        self.levels
            .iter()
            .flatten()
            .map(LevelAir::group_count)
            .sum()
    }

    /// tg `ChangeTurf` without `CHANGETURF_INHERIT_AIR` across kinds: the tile on `pos` gets
    /// the air of `turf` (a wall drops its air, a new floor starts with its `initial_gas_mix`).
    /// Does nothing before atmos is built (it reads `TurfAir` then) or off the map.
    pub fn change_turf(&mut self, pos: TilePos, turf: &TurfAir) {
        if !self.initialized {
            return;
        }
        let air = (turf.kind == TileKind::Open).then(|| initial_mix(turf, &self.atmos_gen));
        if let Some((level, tile)) = self.locate_mut(pos) {
            level.change_tile(tile, turf.kind, air);
            level.set_thermal(tile, turf.thermal);
        }
    }

    /// `ChangeTurf` with `CHANGETURF_INHERIT_AIR` between open turfs: the air stays, the new
    /// turf brings its own heat ([`LevelAir::replace_turf_keeping_air`]).
    pub fn replace_turf_keeping_air(&mut self, pos: TilePos, turf: &TurfAir) {
        if !self.initialized {
            return;
        }
        if let Some((level, tile)) = self.locate_mut(pos) {
            level.replace_turf_keeping_air(tile, turf.thermal);
        }
    }

    /// Records that a blocker on `pos` changed (`closing`: tg `air_update_turf(TRUE, TRUE)`).
    pub fn blocker_changed(&mut self, pos: TilePos, closing: bool) {
        if self.initialized {
            self.dirty.push((pos, closing));
        }
    }

    /// One SSair fire on every level, in parallel when there are several.
    /// Links or unlinks `upper` (open space) and `lower` (the tile under it) for air.
    pub fn set_vertical(&mut self, upper: TilePos, lower: TilePos, linked: bool) {
        let at = self.vertical.iter().position(|&p| p == (upper, lower));
        match (at, linked) {
            (None, true) => self.vertical.push((upper, lower)),
            (Some(i), false) => {
                self.vertical.swap_remove(i);
            }
            _ => {}
        }
    }

    /// A side of a vertical link: its kind, air and open neighbours on its level.
    fn vertical_side(&self, pos: TilePos) -> Option<(TileKind, &GasMixture, Option<u8>)> {
        let (level, tile) = self.locate(pos)?;
        Some((
            level.kind(tile),
            level.air(tile)?,
            level.adjacent_count(tile),
        ))
    }

    pub fn vertical_links(&self) -> usize {
        self.vertical.len()
    }

    /// The share across each vertical link, after the levels fired: tg counts the tile above or
    /// below among `atmos_adjacent_turfs`, so the two share like neighbours (each side's
    /// coefficient `1 / (neighbours + 1)`, the other level counted). Space on one side vents
    /// the other. Idle links cost a comparison.
    // shortcut: the link is not part of the levels' excited groups (each side wakes as a whole
    // tile instead) and moves no pressure difference (spacewind) across levels.
    fn share_vertical(&mut self) {
        for i in 0..self.vertical.len() {
            let (upper, lower) = self.vertical[i];
            let (Some((ku, a, au)), Some((kl, b, bl))) =
                (self.vertical_side(upper), self.vertical_side(lower))
            else {
                continue;
            };
            let moves = a
                .moles
                .iter()
                .zip(b.moles)
                .any(|(&x, y)| (x - y).abs() > gas::MINIMUM_MOLES_DELTA_TO_MOVE)
                || (a.temperature - b.temperature).abs()
                    > gas::MINIMUM_TEMPERATURE_DELTA_TO_CONSIDER;
            if !moves || (ku != TileKind::Open && kl != TileKind::Open) {
                continue;
            }
            let (mut a, mut b) = (a.clone(), b.clone());
            let coeff = |n: Option<u8>| 1.0 / f64::from(n.unwrap_or(0) + 2);
            a.archive();
            b.archive();
            a.share(&mut b, coeff(au), coeff(bl));
            if ku == TileKind::Open {
                self.modify_air(upper, |m| *m = a);
            }
            if kl == TileKind::Open {
                self.modify_air(lower, |m| *m = b);
            }
        }
    }

    pub fn fire(&mut self) -> Vec<PressureDifference> {
        let start = Instant::now();
        self.fires += 1;
        let mut levels: Vec<(u16, &mut LevelAir)> = self
            .levels
            .iter_mut()
            .enumerate()
            .filter_map(|(z, l)| l.as_mut().map(|l| (z as u16, l)))
            .collect();
        let fire_level = |z: u16, level: &mut LevelAir| {
            let start = Instant::now();
            let mut out = Vec::new();
            level.fire(&mut out);
            (z, out, start.elapsed())
        };
        let per_level: Vec<(u16, Vec<PressureDelta>, Duration)> = if levels.len() > 1 {
            ComputeTaskPool::get_or_init(TaskPool::default).scope(|s| {
                for (z, level) in levels.drain(..) {
                    s.spawn(async move { fire_level(z, level) });
                }
            })
        } else {
            levels
                .drain(..)
                .map(|(z, level)| fire_level(z, level))
                .collect()
        };
        self.last_fire_levels = per_level.iter().map(|(z, _, t)| (*z, *t)).collect();
        self.share_vertical();
        let mut out = Vec::new();
        for (z, deltas, _) in per_level {
            let level = self.level(z).expect("fired above");
            out.extend(deltas.into_iter().map(|d| {
                let (x, y) = level.pos(d.tile);
                PressureDifference {
                    pos: TilePos::new(x, y, z),
                    difference: d.difference,
                    dir: d.dir,
                }
            }));
        }
        self.last_fire = start.elapsed();
        out
    }
}

pub struct AtmosPlugin;

impl Plugin for AtmosPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Atmos>()
            .init_resource::<AirTick>()
            .init_resource::<pipenet::Pipenet>()
            .init_resource::<OverlayEntities>()
            .init_resource::<HotspotEntities>()
            .add_message::<PressureDifference>()
            .add_message::<FireAct>()
            // Maps load in `PreStartup`, door fillers and other derived blockers in `Startup`.
            .add_systems(PostStartup, init_atmos)
            .configure_sets(
                Update,
                (
                    AtmosSystems::Tick,
                    AtmosSystems::Pipenets,
                    AtmosSystems::Machinery,
                    AtmosSystems::Fire,
                )
                    .chain()
                    .after(WorldSystems::Movement),
            )
            .add_systems(
                Update,
                (
                    tick_air.in_set(AtmosSystems::Tick),
                    process_pipenets
                        .run_if(air_tick_due)
                        .in_set(AtmosSystems::Pipenets),
                    (
                        apply_blocker_changes,
                        fire_air,
                        sync_gas_overlays,
                        emit_fire_acts,
                        sync_hotspots,
                    )
                        .chain()
                        .in_set(AtmosSystems::Fire),
                ),
            );
    }
}

fn blocker_inserted(mut world: DeferredWorld, ctx: HookContext) {
    if let Some(&pos) = world.get::<TilePos>(ctx.entity)
        && let Some(mut atmos) = world.get_resource_mut::<Atmos>()
    {
        atmos.blocker_changed(pos, true);
    }
}

fn blocker_discarded(mut world: DeferredWorld, ctx: HookContext) {
    if let Some(&pos) = world.get::<TilePos>(ctx.entity)
        && let Some(mut atmos) = world.get_resource_mut::<Atmos>()
    {
        atmos.blocker_changed(pos, false);
    }
}

fn density_inserted(
    insert: On<Insert<Density>>,
    blockers: Query<&TilePos, With<AtmosBlocker>>,
    mut atmos: ResMut<Atmos>,
) {
    if let Ok(&pos) = blockers.get(insert.entity) {
        atmos.blocker_changed(pos, true);
    }
}

fn density_removed(
    remove: On<Remove<Density>>,
    blockers: Query<&TilePos, With<AtmosBlocker>>,
    mut atmos: ResMut<Atmos>,
) {
    if let Ok(&pos) = blockers.get(remove.entity) {
        atmos.blocker_changed(pos, false);
    }
}

/// Builds every level from the turfs and blockers on the map (tg `SSair.Initialize`).
pub fn init_atmos(world: &mut World) {
    let start = Instant::now();
    let mut turfs = world.query::<(&TurfAir, &TilePos)>();
    let mut by_z: HashMap<u16, Vec<(TilePos, TurfAir)>> = HashMap::new();
    for (air, &pos) in turfs.iter(world) {
        by_z.entry(pos.z).or_default().push((pos, air.clone()));
    }
    let mut blockers = world.query::<(
        &AtmosBlocker,
        &TilePos,
        Has<Density>,
        Option<&Border>,
        Has<BlocksHeat>,
    )>();
    let mut edges: HashMap<TilePos, (u8, u8)> = HashMap::new();
    for (blocker, &pos, dense, border, heat) in blockers.iter(world) {
        let mask = blocker.edges(dense, border.copied());
        let entry = edges.entry(pos).or_default();
        entry.0 |= mask;
        if heat {
            entry.1 |= mask;
        }
    }
    let index = world.resource::<SpatialIndex>();
    let mut built = Vec::new();
    let mut mixes: HashMap<(Arc<str>, Option<u64>), GasMixture> = HashMap::new();
    // `generate_atmos`: once a round, from the clock.
    let seed = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    let atmos_gen = planetary::generate_atmos(seed);
    for (id, text) in &atmos_gen {
        info!("planetary atmosphere {id}: {text}");
    }
    for (z, tiles) in by_z {
        let Some((width, height)) = index.level_size(z) else {
            continue;
        };
        let mut level = LevelAir::new(width, height);
        for (pos, air) in tiles {
            let Some(tile) = level.tile(pos.x, pos.y) else {
                continue;
            };
            let key = (
                air.initial_gas_mix.clone(),
                air.temperature.map(f64::to_bits),
            );
            let mix = mixes
                .entry(key)
                .or_insert_with(|| initial_mix(&air, &atmos_gen))
                .clone();
            let planetary = air.planetary.then_some((&*air.initial_gas_mix, &mix));
            level.set_tile(tile, air.kind, Some(mix.clone()), planetary);
            level.set_thermal(tile, air.thermal);
        }
        for (&pos, &(mask, heat)) in edges.iter().filter(|(p, _)| p.z == z) {
            if let Some(tile) = level.tile(pos.x, pos.y) {
                level.set_edge_block(tile, mask);
                level.set_heat_block(tile, heat);
            }
        }
        let report = level.setup();
        built.push((z, level, report));
    }
    let mut atmos = world.resource_mut::<Atmos>();
    for (z, level, report) in built {
        info!(
            "atmos z{z}: {} cells, {} turfs with differing air at load (+{} connected, {} groups \
             vented to space), equalized and asleep",
            report.cells, report.differing, report.connected, report.vented_groups
        );
        atmos.insert_level(z, level);
        atmos.setup.push((z, report));
    }
    atmos.atmos_gen = atmos_gen;
    atmos.initialized = true;
    world.add_observer(density_inserted);
    world.add_observer(density_removed);
    info!("atmos ready in {:.2?}", start.elapsed());
}

/// The turf's starting air (`parse_gas_string`): a planetary atmosphere id becomes this
/// round's generated string (`preprocess_gas_string`); an unparsable string falls back to
/// station air.
fn initial_mix(air: &TurfAir, atmos_gen: &[(&'static str, String)]) -> GasMixture {
    let text = atmos_gen
        .iter()
        .find(|(id, _)| **id == *air.initial_gas_mix)
        .map_or(&*air.initial_gas_mix, |(_, s)| s.as_str());
    let mut mix = GasMixture::parse(text).unwrap_or_else(|e| {
        warn!(
            "initial_gas_mix {:?}: {e}; using station air",
            air.initial_gas_mix
        );
        GasMixture::parse("o2=22;n2=82;TEMP=293.15").expect("valid")
    });
    if let Some(t) = air.temperature {
        mix.temperature = t;
    }
    mix
}

fn apply_blocker_changes(
    mut atmos: ResMut<Atmos>,
    index: Res<SpatialIndex>,
    blockers: Query<(
        &AtmosBlocker,
        Has<Density>,
        Option<&Border>,
        Has<BlocksHeat>,
    )>,
) {
    if atmos.dirty.is_empty() {
        return;
    }
    let dirty = std::mem::take(&mut atmos.dirty);
    for (pos, closing) in dirty {
        let (mask, heat) = index
            .on_tile(pos)
            .iter()
            .filter_map(|&e| blockers.get(e).ok())
            .fold((0, 0), |(m, h), (b, dense, border, stops_heat)| {
                let edges = b.edges(dense, border.copied());
                (m | edges, if stops_heat { h | edges } else { h })
            });
        let Some((level, tile)) = atmos.locate_mut(pos) else {
            continue;
        };
        level.set_edge_block(tile, mask);
        level.set_heat_block(tile, heat);
        level.recalculate_adjacency(tile);
        if closing {
            level.deactivate(tile);
        } else {
            level.activate(tile, false);
        }
    }
}

fn tick_air(time: Res<Time>, atmos: Res<Atmos>, mut tick: ResMut<AirTick>) {
    tick.due = false;
    if !atmos.initialized {
        return;
    }
    tick.since += time.delta_secs_f64();
    if tick.since < FIRE_INTERVAL {
        return;
    }
    // A late fire does not catch up (tg's MC does not either).
    tick.since = (tick.since - FIRE_INTERVAL).min(FIRE_INTERVAL);
    tick.due = true;
    tick.fires += 1;
}

fn process_pipenets(mut net: ResMut<pipenet::Pipenet>) {
    if net.is_built() {
        let start = Instant::now();
        net.process();
        net.last_process = start.elapsed();
    }
}

fn fire_air(
    tick: Res<AirTick>,
    mut atmos: ResMut<Atmos>,
    mut out: MessageWriter<PressureDifference>,
) {
    if tick.due {
        out.write_batch(atmos.fire());
    }
}

#[derive(Resource, Default)]
struct OverlayEntities(HashMap<TilePos, Entity>);

/// The appearance of a tile's gas overlays: the first visible gas as the sprite, the others as
/// overlays, each with tg's alpha for its step. `None` when nothing shows.
pub fn gas_overlay_appearance(
    icons: &GasOverlayIcons,
    steps: &[u8; GAS_COUNT],
) -> Option<Appearance> {
    let mut visible = steps
        .iter()
        .zip(&icons.0)
        .filter_map(|(&step, icon)| (step > 0).then_some((*icon)?).map(|i| (i, step)));
    let (icon_state, step) = visible.next()?;
    let tint = |step| Some([255, 255, 255, gas::overlay_alpha(step)]);
    Some(Appearance {
        icon_state,
        layer: Layer(GAS_OVERLAY_LAYER),
        plane: GAS_OVERLAY_PLANE,
        color: tint(step),
        pixel_offset: [0, 0],
        rotation: 0,
        mouse_opacity: MouseOpacity::Transparent,
        overlays: visible
            .map(|(icon_state, step)| Overlay {
                icon_state,
                dir: None,
                color: tint(step),
                pixel_offset: [0, 0],
                plane: None,
                only_dirs: 0,
            })
            .collect(),
    })
}

fn sync_gas_overlays(
    mut atmos: ResMut<Atmos>,
    icons: Option<Res<GasOverlayIcons>>,
    mut entities: ResMut<OverlayEntities>,
    mut commands: Commands,
) {
    let mut changed = Vec::new();
    for (z, level) in atmos.levels.iter_mut().enumerate() {
        let Some(level) = level else { continue };
        let mut tiles = Vec::new();
        level.drain_visuals(|tile, steps| tiles.push((tile, *steps)));
        changed.extend(tiles.into_iter().map(|(tile, steps)| {
            let (x, y) = level.pos(tile);
            (TilePos::new(x, y, z as u16), steps)
        }));
    }
    let Some(icons) = icons else { return };
    for (pos, steps) in changed {
        match (gas_overlay_appearance(&icons, &steps), entities.0.get(&pos)) {
            (Some(look), Some(&e)) => {
                commands.entity(e).insert(look);
            }
            (Some(look), None) => {
                let e = commands.spawn((GasOverlay, look, pos)).id();
                entities.0.insert(pos, e);
            }
            (None, Some(&e)) => {
                commands.entity(e).despawn();
                entities.0.remove(&pos);
            }
            (None, None) => {}
        }
    }
}

/// Hotspot exposures (from the fire and from `hotspot_expose` calls since) as [`FireAct`]s.
fn emit_fire_acts(mut atmos: ResMut<Atmos>, mut out: MessageWriter<FireAct>) {
    for (z, level) in atmos.levels.iter_mut().enumerate() {
        let Some(level) = level else { continue };
        let mut acts = Vec::new();
        level.drain_exposures(|e| acts.push(e));
        out.write_batch(acts.into_iter().map(|e| {
            let (x, y) = level.pos(e.tile);
            FireAct {
                pos: TilePos::new(x, y, z as u16),
                temperature: e.temperature,
                volume: e.volume,
            }
        }));
    }
}

#[derive(Resource, Default)]
struct HotspotEntities(HashMap<TilePos, Entity>);

/// DM `LERP(a, b, amount)`.
fn lerp(a: f64, b: f64, amount: f64) -> f64 {
    a + (b - a) * amount
}

/// `gauss_lerp` of `update_color`.
fn gauss_lerp(x: f64, x1: f64, x2: f64) -> f64 {
    let b = (x1 + x2) * 0.5;
    let c = (x2 - x1) / 6.0;
    std::f64::consts::E.powf(-((x - b).powi(2) / (2.0 * c).powi(2)))
}

/// `heat2colour_r/g/b` (`code/__HELPERS/type2type.dm`).
pub fn heat2colour(temp: f64) -> [f64; 3] {
    let t = temp / 100.0;
    let r = if t <= 66.0 {
        255.0
    } else {
        (329.698_727_446 * (t - 60.0).powf(-0.133_204_759_2)).clamp(0.0, 255.0)
    };
    let g = if t <= 66.0 {
        (99.470_802_586_1 * t.ln() - 161.119_568_166_1).clamp(0.0, 255.0)
    } else {
        (288.122_168_529_3 * (t - 60.0).powf(-0.075_148_492)).clamp(0.0, 255.0)
    };
    let b = if t >= 66.0 {
        255.0
    } else if t <= 16.0 {
        0.0
    } else {
        (138.517_731_223_1 * (t - 10.0).ln() - 305.044_792_730_7).clamp(0.0, 255.0)
    };
    [r, g, b]
}

/// `update_color` of a hotspot at `temperature`: its sprite tint (RGBA) and light color.
// shortcut: tg's color matrix (a greyscale blend for fires outside 1000..3000 K) is drawn as
// its diagonal multiply, and the sparkle, lightning and fusion overlays of 200 000 K and hotter
// fires are not drawn; `BLEND_ADD` is drawn as normal alpha blending. Port with color matrices
// and blend modes in `Appearance`.
pub fn hotspot_color(temperature: f64, cold_fire: bool) -> ([u8; 4], [u8; 3]) {
    let [mut r, mut g, mut b] = heat2colour(temperature);
    // BandaStation: 180 instead of 255.
    let mut a = 180.0;
    let mut greyscale = 1.0;
    if cold_fire {
        r = 0.0;
        g = lerp(255.0, temperature, 1.2);
        b = lerp(255.0, temperature, 0.9);
        a = 100.0;
    } else if temperature < 5000.0 {
        let normal = gauss_lerp(temperature, 1000.0, 3000.0);
        r = lerp(r, 255.0, normal);
        g = lerp(g, 255.0, normal);
        b = lerp(b, 255.0, normal);
        a -= gauss_lerp(temperature, -5000.0, 5000.0) * 128.0;
        greyscale -= normal;
    }
    if temperature > 40000.0 {
        let purple = if temperature < lerp(40000.0, 200_000.0, 0.5) {
            gauss_lerp(temperature, 40000.0, 200_000.0)
        } else {
            1.0
        };
        r = lerp(r, 255.0, purple);
    }
    let byte = |v: f64| v.round().clamp(0.0, 255.0) as u8;
    let light = [
        byte(lerp(250.0, r, greyscale)),
        byte(lerp(160.0, g, greyscale)),
        byte(lerp(25.0, b, greyscale)),
    ];
    ([byte(r), byte(g), byte(b), byte(a)], light)
}

/// The sprite and light of a hotspot.
pub fn hotspot_look(icons: &HotspotIcons, hotspot: &Hotspot) -> Option<(Appearance, Optics)> {
    let stage = hotspot.stage as usize;
    let icon_state = icons.stages[stage]?;
    let (color, light) = hotspot_color(hotspot.color_temperature, hotspot.cold_fire);
    let appearance = Appearance {
        icon_state,
        layer: Layer(HOTSPOT_LAYER),
        plane: GAS_OVERLAY_PLANE,
        color: Some(color),
        pixel_offset: [0, 0],
        rotation: 0,
        mouse_opacity: MouseOpacity::Transparent,
        overlays: icons.glow[stage]
            .map(|glow| Overlay {
                icon_state: glow,
                dir: None,
                color: None,
                pixel_offset: [0, 0],
                plane: None,
                only_dirs: 0,
            })
            .into_iter()
            .collect(),
    };
    let optics = Optics {
        opaque: false,
        tile: None,
        light: Some(LightSource {
            range: HOTSPOT_LIGHT_RANGE,
            power: HOTSPOT_LIGHT_POWER,
            color: light,
            height: 1.0,
            angle: 360,
            dir: Dir::South,
            offset: [0, 0],
        }),
    };
    Some((appearance, optics))
}

/// Spawns, updates and removes the hotspot entities of tiles whose fire changed.
fn sync_hotspots(
    mut atmos: ResMut<Atmos>,
    icons: Option<Res<HotspotIcons>>,
    mut entities: ResMut<HotspotEntities>,
    mut commands: Commands,
) {
    let mut changed = Vec::new();
    for (z, level) in atmos.levels.iter_mut().enumerate() {
        let Some(level) = level else { continue };
        let mut tiles = Vec::new();
        level.drain_hotspot_visuals(|tile, hotspot| tiles.push((tile, hotspot.cloned())));
        changed.extend(tiles.into_iter().map(|(tile, hotspot)| {
            let (x, y) = level.pos(tile);
            (TilePos::new(x, y, z as u16), hotspot)
        }));
    }
    let Some(icons) = icons else { return };
    for (pos, hotspot) in changed {
        let look = hotspot
            .as_ref()
            .and_then(|h| Some((hotspot_look(&icons, h)?, h.dir)));
        match (look, entities.0.get(&pos)) {
            (Some(((appearance, optics), dir)), Some(&e)) => {
                commands.entity(e).insert((appearance, optics, Facing(dir)));
            }
            (Some(((appearance, optics), dir)), None) => {
                let e = commands
                    .spawn((HotspotEffect, appearance, optics, Facing(dir), pos))
                    .id();
                entities.0.insert(pos, e);
            }
            (None, Some(&e)) => {
                commands.entity(e).despawn();
                entities.0.remove(&pos);
            }
            (None, None) => {}
        }
    }
}

#[cfg(test)]
mod vertical_tests {
    use super::*;

    fn deck(moles: f64) -> LevelAir {
        let mut level = LevelAir::new(1, 1);
        let air = (moles > 0.0).then(|| {
            let mut m = GasMixture::vacuum();
            m.set(GasId::O2, moles);
            m.temperature = gas::T20C;
            m
        });
        level.set_tile(
            0,
            TileKind::Open,
            Some(air.unwrap_or_else(|| {
                let mut m = GasMixture::vacuum();
                m.temperature = gas::T20C;
                m
            })),
            None,
        );
        level.setup();
        level
    }

    #[test]
    fn air_falls_through_open_space_and_evens_out() {
        let mut atmos = Atmos::default();
        atmos.insert_level(1, deck(0.0));
        atmos.insert_level(2, deck(100.0));
        let (upper, lower) = (TilePos::new(1, 1, 2), TilePos::new(1, 1, 1));
        atmos.fire();
        assert_eq!(atmos.moles(lower, GasId::O2), Some(0.0), "not linked yet");
        atmos.set_vertical(upper, lower, true);
        atmos.fire();
        let (u, l) = (
            atmos.moles(upper, GasId::O2).unwrap(),
            atmos.moles(lower, GasId::O2).unwrap(),
        );
        // One neighbour each (the other level): coefficient 1/2, half the difference moves.
        assert!(
            (u - 50.0).abs() < 1e-6 && (l - 50.0).abs() < 1e-6,
            "{u} {l}"
        );
        for _ in 0..5 {
            atmos.fire();
        }
        assert!((atmos.moles(lower, GasId::O2).unwrap() - 50.0).abs() < 1e-6);
        atmos.set_vertical(upper, lower, false);
        assert_eq!(atmos.vertical_links(), 0);
    }
}
