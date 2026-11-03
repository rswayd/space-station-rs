//! Superconduction: heat through windows, closed doors and solid turfs, and hot turfs radiating
//! to space. tg `/turf/proc/super_conduct` and its helpers (`LINDA_turf_tile.dm`),
//! `SSair.process_super_conductivity`.
//!
//! Every tile has its own [`Thermal`] (the turf's, apart from its air). Open tiles hotter than
//! [`MINIMUM_TEMPERATURE_START_SUPERCONDUCTION`] join the conducting list at the end of their
//! `process_cell` and stay awake while on it; each fire every listed tile shares heat with the
//! neighbours air does not reach (across windows, closed doors, into walls and rock), radiates
//! its solid heat to space and shares with its own air, and leaves the list below
//! [`MINIMUM_TEMPERATURE_FOR_SUPERCONDUCTION`].

use ss13_shared::Dir;

use super::{LevelAir, NONE, TileKind, VACUUM};
use crate::gas::{
    GasMixture, HEAT_CAPACITY_VACUUM, MINIMUM_TEMPERATURE_DELTA_TO_CONSIDER, MOLES_CELLSTANDARD,
    T0C, T20C, TCMB, conduction_energy,
};

/// `MINIMUM_TEMPERATURE_FOR_SUPERCONDUCTION`: a conducting tile below it stops.
pub const MINIMUM_TEMPERATURE_FOR_SUPERCONDUCTION: f64 = T20C + 80.0;
/// `MINIMUM_TEMPERATURE_START_SUPERCONDUCTION`: an open tile's air above it starts.
pub const MINIMUM_TEMPERATURE_START_SUPERCONDUCTION: f64 = T20C + 400.0;
/// `WINDOW_HEAT_TRANSFER_COEFFICIENT`: air to air across a window or door.
pub const WINDOW_HEAT_TRANSFER_COEFFICIENT: f64 = 0.1;
/// `M_CELL_WITH_RATIO` (tg compares a heat capacity with it).
const M_CELL_WITH_RATIO: f64 = MOLES_CELLSTANDARD * 0.005;
/// tg `INFINITY` (`__DEFINES/maths.dm`), the `/turf` default `heat_capacity`: finite, so a
/// turf with it is a heat reservoir rather than a NaN.
pub const DM_INFINITY: f64 = 1e31;

/// A turf's own heat: tg `thermal_conductivity`, `heat_capacity` and `temperature`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Thermal {
    pub conductivity: f64,
    pub heat_capacity: f64,
    /// Kelvin, the solid's (an open turf's air has its own).
    pub temperature: f64,
}

impl Thermal {
    /// `/turf/open/space`.
    pub const SPACE: Self = Self {
        conductivity: 0.4,
        heat_capacity: 700_000.0,
        temperature: TCMB,
    };
    /// `/turf`.
    pub const TURF: Self = Self {
        conductivity: 0.05,
        heat_capacity: DM_INFINITY,
        temperature: T20C,
    };
}

/// Per tile: its temperature, the archive of it and the turf type's constants.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Solid {
    temperature: f64,
    archived: f64,
    /// Fire it was archived in (tg `archived_cycle`, shared with the air's).
    pub(crate) cycle: u32,
    /// Index into [`LevelAir::materials`].
    material: u16,
    conducting: bool,
}

impl Solid {
    pub(crate) fn new(thermal: Thermal, material: u16) -> Self {
        Self {
            temperature: thermal.temperature,
            archived: thermal.temperature,
            cycle: 0,
            material,
            conducting: false,
        }
    }

    pub(crate) fn archive(&mut self, cycle: u32) {
        self.archived = self.temperature;
        self.cycle = cycle;
    }
}

impl LevelAir {
    /// Sets the turf's own heat (its type's `thermal_conductivity`, `heat_capacity` and
    /// `temperature`); [`Self::set_tile`] resets it to [`Thermal::SPACE`] or [`Thermal::TURF`].
    pub fn set_thermal(&mut self, tile: u32, thermal: Thermal) {
        let material = self.material_of(thermal.conductivity, thermal.heat_capacity);
        let conducting = self.solid[tile as usize].conducting;
        self.solid[tile as usize] = Solid {
            conducting,
            ..Solid::new(thermal, material)
        };
    }

    pub(crate) fn material_of(&mut self, conductivity: f64, heat_capacity: f64) -> u16 {
        let key = (conductivity, heat_capacity);
        let at = self.materials.iter().position(|&m| m == key);
        at.unwrap_or_else(|| {
            self.materials.push(key);
            self.materials.len() - 1
        }) as u16
    }

    /// The turf's own heat now.
    pub fn thermal(&self, tile: u32) -> Thermal {
        let solid = self.solid[tile as usize];
        let (conductivity, heat_capacity) = self.materials[solid.material as usize];
        Thermal {
            conductivity,
            heat_capacity,
            temperature: solid.temperature,
        }
    }

    /// A blocked tile's temperature and heat capacity (`/turf/GetTemperature`,
    /// `GetHeatCapacity`); `None` for tiles with air.
    pub fn solid_heat(&self, tile: u32) -> Option<(f64, f64)> {
        (*self.kind.get(tile as usize)? == TileKind::Blocked).then(|| {
            let t = self.thermal(tile);
            (t.temperature, t.heat_capacity)
        })
    }

    /// `/turf/TakeTemperature` of a blocked tile, then its `temperature_expose(air,
    /// temperature)`. False for tiles with air.
    pub fn take_solid_temperature(&mut self, tile: u32, delta: f64) -> bool {
        if self.kind.get(tile as usize) != Some(&TileKind::Blocked) {
            return false;
        }
        let solid = &mut self.solid[tile as usize];
        solid.temperature += delta;
        let temperature = solid.temperature;
        if temperature >= super::EXPOSE_REPORT_MIN {
            self.exposed.push((tile, temperature));
        }
        true
    }

    /// A new turf over the same air (`CHANGETURF_INHERIT_AIR`): its own heat, and a fire may
    /// burn it again.
    pub fn replace_turf_keeping_air(&mut self, tile: u32, thermal: Thermal) {
        self.set_thermal(tile, thermal);
        if let Some(c) = self.open_cell(tile) {
            self.cells[c as usize].burn_reported = false;
        }
    }

    /// Edges of `tile` whose air blockers also stop heat (tg `block_superconductivity`).
    pub fn set_heat_block(&mut self, tile: u32, edges: u8) {
        self.heat_block[tile as usize] = edges;
    }

    /// Tiles on the conducting list (tg `active_super_conductivity`).
    pub fn conducting_count(&self) -> usize {
        self.conducting.len()
    }

    fn constants(&self, tile: u32) -> (f64, f64) {
        self.materials[self.solid[tile as usize].material as usize]
    }

    /// The tile's air cell when it is an open turf with air.
    fn open_cell(&self, tile: u32) -> Option<u32> {
        let c = self.cell_of[tile as usize];
        (self.kind[tile as usize] == TileKind::Open && c != NONE).then_some(c)
    }

    /// tg `archive()` if not done this fire: the air too for open turfs.
    fn archive_turf(&mut self, tile: u32) {
        if self.solid[tile as usize].cycle >= self.fire_count {
            return;
        }
        match self.open_cell(tile) {
            Some(c) => self.archive(c),
            None => self.solid[tile as usize].archive(self.fire_count),
        }
    }

    /// `consider_superconductivity`: joins the conducting list when hot enough.
    pub(crate) fn consider_superconductivity(&mut self, tile: u32, starting: bool) -> bool {
        let threshold = if starting {
            MINIMUM_TEMPERATURE_START_SUPERCONDUCTION
        } else {
            MINIMUM_TEMPERATURE_FOR_SUPERCONDUCTION
        };
        let hot = match (self.kind[tile as usize], self.open_cell(tile)) {
            // The immutable vacuum is never hot.
            (TileKind::Space, _) => false,
            (_, Some(c)) => {
                let mix = &self.cells[c as usize].mix;
                mix.temperature >= threshold && mix.heat_capacity() >= M_CELL_WITH_RATIO
            }
            (_, None) => self.solid[tile as usize].temperature >= threshold,
        };
        if !hot || self.constants(tile).0 == 0.0 {
            return false;
        }
        let solid = &mut self.solid[tile as usize];
        if !solid.conducting {
            solid.conducting = true;
            self.conducting.push(tile);
        }
        true
    }

    /// `SSair.process_super_conductivity`: the list as it was, from its end.
    pub(crate) fn process_super_conductivity(&mut self) {
        let run = self.conducting.clone();
        for &tile in run.iter().rev() {
            self.super_conduct(tile);
        }
    }

    /// `conductivity_directions`: solid turfs every way; open ones where air does not reach
    /// and no blocker stops heat (tg `atmos_supeconductivity`).
    fn conductivity_directions(&mut self, tile: u32) -> u8 {
        if self.open_cell(tile).is_none() {
            self.archive_turf(tile);
            return 0xF;
        }
        let mut dirs = 0;
        for dir in Dir::CARDINALS {
            let blocked = self.heat_block[tile as usize] & dir as u8 != 0
                || self
                    .neighbor(tile, dir)
                    .is_some_and(|n| self.heat_block[n as usize] & dir.reverse() as u8 != 0);
            if !self.adjacent(tile, dir) && !blocked {
                dirs |= dir as u8;
            }
        }
        dirs
    }

    /// `/turf/proc/super_conduct`.
    fn super_conduct(&mut self, tile: u32) {
        let dirs = self.conductivity_directions(tile);
        for dir in Dir::CARDINALS {
            if dirs & dir as u8 == 0 {
                continue;
            }
            let Some(n) = self.neighbor(tile, dir) else {
                continue;
            };
            if self.constants(n).0 == 0.0 {
                continue;
            }
            self.archive_turf(n);
            self.neighbor_conduct_with_src(n, tile);
            self.consider_superconductivity(n, false);
        }
        self.radiate_to_space(tile);
        self.finish_superconduction(tile);
    }

    /// `neighbor.neighbor_conduct_with_src(src)`.
    // shortcut: tg also wakes a space neighbour (`add_to_active` of a space turf); here space
    // never processes on its own.
    fn neighbor_conduct_with_src(&mut self, n: u32, src: u32) {
        let n_kind = self.kind[n as usize];
        let n_cell = self.open_cell(n);
        let src_cell = self.open_cell(src);
        let n_open = n_cell.is_some() || n_kind == TileKind::Space;
        if !n_open {
            match src_cell {
                Some(c) => {
                    let t = self.share_air_with_solid(c, n);
                    self.solid[n as usize].temperature = t;
                }
                None => self.share_temperature_mutual_solid(src, n, self.constants(n).0),
            }
            // `temperature_expose(null, temperature)` of the solid neighbour.
            let temperature = self.solid[n as usize].temperature;
            if temperature >= super::EXPOSE_REPORT_MIN {
                self.exposed.push((n, temperature));
            }
            return;
        }
        match (src_cell, n_cell) {
            (Some(s), Some(o)) => {
                let (src_cell, n_cell) = self.pair(s, o);
                src_cell.mix.temperature_share(
                    &mut n_cell.mix,
                    WINDOW_HEAT_TRANSFER_COEFFICIENT,
                    None,
                );
            }
            (Some(s), None) => {
                let mut space = VACUUM.clone();
                self.cells[s as usize].mix.temperature_share(
                    &mut space,
                    WINDOW_HEAT_TRANSFER_COEFFICIENT,
                    None,
                );
            }
            (None, Some(o)) => {
                let t = self.share_air_with_solid(o, src);
                self.solid[src as usize].temperature = t;
            }
            (None, None) => {
                let (conductivity, heat_capacity) = self.constants(src);
                let solid = self.solid[src as usize].temperature;
                let t = VACUUM
                    .clone()
                    .temperature_share_solid(conductivity, solid, heat_capacity);
                self.solid[src as usize].temperature = t;
            }
        }
        if let Some(o) = n_cell {
            self.add_to_active(o, false);
        }
    }

    /// `temperature_share_open_to_solid`: the air of cell `c` with the solid of `solid`;
    /// the solid's new temperature.
    fn share_air_with_solid(&mut self, c: u32, solid: u32) -> f64 {
        let (conductivity, heat_capacity) = self.constants(solid);
        let temperature = self.solid[solid as usize].temperature;
        self.cells[c as usize]
            .mix
            .temperature_share_solid(conductivity, temperature, heat_capacity)
    }

    /// `src.share_temperature_mutual_solid(sharer, coefficient)`.
    fn share_temperature_mutual_solid(&mut self, src: u32, sharer: u32, coefficient: f64) {
        let (ours, theirs) = (self.solid[src as usize], self.solid[sharer as usize]);
        let (our_capacity, their_capacity) = (self.constants(src).1, self.constants(sharer).1);
        let delta = theirs.archived - ours.archived;
        if delta.abs() <= MINIMUM_TEMPERATURE_DELTA_TO_CONSIDER
            || our_capacity == 0.0
            || their_capacity == 0.0
        {
            return;
        }
        let heat = coefficient * conduction_energy(delta, our_capacity, their_capacity);
        self.solid[src as usize].temperature += heat / our_capacity;
        self.solid[sharer as usize].temperature -= heat / their_capacity;
    }

    /// `radiate_to_spess`: a solid above 0 °C loses heat to the vacuum.
    fn radiate_to_space(&mut self, tile: u32) {
        let (conductivity, heat_capacity) = self.constants(tile);
        let solid = &mut self.solid[tile as usize];
        if solid.temperature <= T0C {
            return;
        }
        let delta = solid.archived - TCMB;
        if heat_capacity <= 0.0 || delta.abs() <= MINIMUM_TEMPERATURE_DELTA_TO_CONSIDER {
            return;
        }
        let heat = conduction_energy(conductivity * delta, HEAT_CAPACITY_VACUUM, heat_capacity);
        solid.temperature -= heat / heat_capacity;
    }

    /// `finish_superconduction`: off the list when cool, else an open turf shares with its air.
    fn finish_superconduction(&mut self, tile: u32) {
        let cell = self.open_cell(tile);
        let temperature = match cell {
            Some(c) => self.cells[c as usize].mix.temperature,
            None => self.solid[tile as usize].temperature,
        };
        if temperature < MINIMUM_TEMPERATURE_FOR_SUPERCONDUCTION {
            self.solid[tile as usize].conducting = false;
            self.conducting.retain(|&t| t != tile);
            return;
        }
        if let Some(c) = cell {
            let t = self.share_air_with_solid(c, tile);
            self.solid[tile as usize].temperature = t;
        }
    }
}

impl GasMixture {
    /// `temperature_share(null, coefficient, sharer_temperature, sharer_heat_capacity)`:
    /// conduction with a solid; returns the solid's new temperature.
    pub(crate) fn temperature_share_solid(
        &mut self,
        conduction_coefficient: f64,
        sharer_temperature: f64,
        sharer_heat_capacity: f64,
    ) -> f64 {
        let temperature_delta = self.temperature_archived - sharer_temperature;
        if temperature_delta.abs() <= MINIMUM_TEMPERATURE_DELTA_TO_CONSIDER {
            return sharer_temperature;
        }
        let self_heat_capacity = self.heat_capacity_archive();
        if sharer_heat_capacity <= crate::gas::MINIMUM_HEAT_CAPACITY
            || self_heat_capacity <= crate::gas::MINIMUM_HEAT_CAPACITY
        {
            return sharer_temperature;
        }
        let heat = conduction_energy(
            conduction_coefficient * temperature_delta,
            sharer_heat_capacity,
            self_heat_capacity,
        );
        self.temperature = (self.temperature - heat / self_heat_capacity).max(TCMB);
        (sharer_temperature + heat / sharer_heat_capacity).max(TCMB)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gas::GasMixture;

    fn hot_air(t: f64) -> GasMixture {
        GasMixture::parse(&format!("o2=22;n2=82;TEMP={t}")).unwrap()
    }

    /// A hot floor (x=1), a solid `/turf` (x=2), a cold floor (x=3).
    fn rooms() -> LevelAir {
        let mut level = LevelAir::new(3, 1);
        level.set_tile(0, TileKind::Open, Some(hot_air(2000.0)), None);
        level.set_tile(1, TileKind::Blocked, None, None);
        level.set_tile(2, TileKind::Open, Some(hot_air(T20C)), None);
        let floor = Thermal {
            conductivity: 0.02,
            heat_capacity: 20_000.0,
            temperature: T20C,
        };
        level.set_thermal(0, floor);
        level.set_thermal(2, floor);
        level.set_thermal(1, Thermal::TURF);
        level.setup();
        level.activate(0, false);
        level
    }

    fn temp(level: &LevelAir, tile: u32) -> f64 {
        level.air(tile).unwrap().temperature
    }

    /// A hot room keeps conducting into its floor and the turf next to it, and cools.
    #[test]
    fn hot_air_conducts_into_solids() {
        let mut level = rooms();
        let mut pressure = Vec::new();
        level.fire(&mut pressure);
        assert_eq!(level.conducting_count(), 1, "the hot tile joined the list");
        let after_one = temp(&level, 0);
        assert!(after_one < 2000.0);
        assert!(level.thermal(0).temperature > T20C, "the floor warmed");
        for _ in 0..20 {
            level.fire(&mut pressure);
        }
        assert!(temp(&level, 0) < after_one);
        // The middle `/turf` is a 1e31 J/K reservoir: it does not warm.
        assert!((level.thermal(1).temperature - T20C).abs() < 1e-6);
        assert!(level.active_count() >= 1, "a conducting tile stays awake");
    }

    /// Air to air through a window: the cold side warms only without a heat blocker.
    #[test]
    fn heat_crosses_windows_but_not_heat_proof_doors() {
        for (block, warms) in [(false, true), (true, false)] {
            let mut level = LevelAir::new(2, 1);
            level.set_tile(0, TileKind::Open, Some(hot_air(2000.0)), None);
            level.set_tile(1, TileKind::Open, Some(hot_air(T20C)), None);
            level.set_edge_block(0, Dir::East as u8);
            if block {
                level.set_heat_block(0, Dir::East as u8);
            }
            level.setup();
            level.activate(0, false);
            let mut pressure = Vec::new();
            for _ in 0..5 {
                level.fire(&mut pressure);
            }
            assert_eq!(temp(&level, 1) > T20C + 1.0, warms, "heat block {block}");
        }
    }

    /// Below the stop threshold a tile leaves the list.
    #[test]
    fn cool_tiles_stop() {
        let mut level = rooms();
        let mut pressure = Vec::new();
        level.fire(&mut pressure);
        assert_eq!(level.conducting_count(), 1);
        level.modify(0, |mix| mix.temperature = T20C);
        level.fire(&mut pressure);
        assert_eq!(level.conducting_count(), 0);
    }

    /// Solid to solid follows `share_temperature_mutual_solid` and conserves heat.
    #[test]
    fn mutual_solid_conserves_heat() {
        let mut level = LevelAir::new(2, 1);
        level.set_tile(0, TileKind::Blocked, None, None);
        level.set_tile(1, TileKind::Blocked, None, None);
        let rock = |t| Thermal {
            conductivity: 0.05,
            heat_capacity: 1000.0,
            temperature: t,
        };
        level.set_thermal(0, rock(1000.0));
        level.set_thermal(1, rock(300.0));
        level.share_temperature_mutual_solid(0, 1, 0.05);
        let (a, b) = (level.thermal(0).temperature, level.thermal(1).temperature);
        // delta 700, energy 700 * 500 = 350000, * 0.05 = 17500 J; /1000 = 17.5 K.
        assert!((a - (1000.0 - 17.5)).abs() < 1e-9 && (b - 317.5).abs() < 1e-9);
    }
}
