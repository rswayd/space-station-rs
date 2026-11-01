//! tg `/obj/effect/hotspot` and `/turf/open/hotspot_expose`
//! (`code/modules/atmospherics/environmental/LINDA_fire.dm`): a fire on a turf. A hotspot
//! samples part of the turf's air at its own temperature and reacts it (`perform_exposure`);
//! once it fills the cell it "bypasses" and follows the turf's air, radiating heat to the
//! neighbours (spreading). It keeps its turf processing and burns out without fuel or oxygen.
//!
//! Hotspots live in their level ([`Hotspots`]); what they do to things on their turf leaves as
//! [`FireExposure`]s (tg `fire_act(temperature, volume)` of every atom there), their looks as
//! dirty tiles for the visuals.
// shortcut: no `/datum/hot_group` (the fire sound loop) and no `burn_tile` (burnt floor icons).

use ss13_shared::Dir;

use super::{LevelAir, NONE};
use crate::gas::{CELL_VOLUME, GasId, GasMixture, TCMB};
use crate::reactions::{
    COLD_FIRE_SPREAD_RADIOSITY_SCALE, Exposures, FIRE_MINIMUM_TEMPERATURE_TO_EXIST,
    FIRE_MINIMUM_TEMPERATURE_TO_SPREAD, FIRE_SPREAD_RADIOSITY_SCALE,
    FREON_MAXIMUM_BURN_TEMPERATURE, Holder, PLASMA_MINIMUM_BURN_TEMPERATURE,
};

/// `icon_state` of a hotspot (`fire_stage`): `light`, `medium` or `heavy`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FireStage {
    Light,
    Medium,
    Heavy,
}

impl FireStage {
    pub fn icon_state(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Medium => "medium",
            Self::Heavy => "heavy",
        }
    }
}

/// One `/obj/effect/hotspot`.
#[derive(Clone, Debug, PartialEq)]
pub struct Hotspot {
    pub(super) cell: u32,
    /// How big the fire is; `CELL_VOLUME` and more fills the turf.
    pub volume: f64,
    /// Kelvin: ignition, then the temperature of what it burns.
    pub temperature: f64,
    pub just_spawned: bool,
    /// Follows the turf's air instead of sampling it.
    pub bypassing: bool,
    /// Burning freon.
    pub cold_fire: bool,
    pub stage: FireStage,
    pub dir: Dir,
    /// The temperature its color was last computed from (`update_color` runs every 7th process).
    pub color_temperature: f64,
    visual_update_tick: u32,
    serial: u64,
}

/// `fire_act(temperature, volume)` of everything on a tile.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FireExposure {
    pub tile: u32,
    pub temperature: f64,
    pub volume: f64,
}

pub(super) struct Hotspots {
    slots: Vec<Option<Hotspot>>,
    free: Vec<u32>,
    /// `SSair.hotspots`: live slots in creation order, with their serial.
    order: Vec<(u32, u64)>,
    next_serial: u64,
    exposures: Vec<FireExposure>,
    /// Cells whose hotspot appeared, changed its looks or went out.
    dirty: Vec<u32>,
    rng: u64,
}

impl Hotspots {
    pub(super) fn new(seed: u64) -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            order: Vec::new(),
            next_serial: 1,
            exposures: Vec::new(),
            dirty: Vec::new(),
            rng: seed | 1,
        }
    }

    /// `pick(GLOB.cardinals)` (xorshift; the looks only).
    fn pick_cardinal(&mut self) -> Dir {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        Dir::CARDINALS[(self.rng % 4) as usize]
    }
}

/// `INSUFFICIENT(path)`: under 0.5 mol.
fn insufficient(air: &GasMixture, gas: GasId) -> bool {
    air.get(gas) < 0.5
}

impl LevelAir {
    pub fn hotspot_count(&self) -> usize {
        self.hotspots.slots.len() - self.hotspots.free.len()
    }

    /// The hotspot on a tile.
    pub fn hotspot(&self, tile: u32) -> Option<&Hotspot> {
        let slot = self.cell(tile)?.hotspot;
        self.hotspots.slots.get(slot as usize)?.as_ref()
    }

    /// Fire exposures since the last call.
    pub fn drain_exposures(&mut self, mut f: impl FnMut(FireExposure)) {
        for e in self.hotspots.exposures.drain(..) {
            f(e);
        }
    }

    /// Tiles whose hotspot appeared, changed its looks or went out since the last call, with
    /// the hotspot now there.
    pub fn drain_hotspot_visuals(&mut self, mut f: impl FnMut(u32, Option<&Hotspot>)) {
        let dirty = std::mem::take(&mut self.hotspots.dirty);
        for c in dirty {
            let cell = &self.cells[c as usize];
            let hotspot = self
                .hotspots
                .slots
                .get(cell.hotspot as usize)
                .and_then(Option::as_ref);
            f(cell.tile, hotspot);
        }
    }

    fn hotspot_mut(&mut self, slot: u32) -> &mut Hotspot {
        self.hotspots.slots[slot as usize]
            .as_mut()
            .expect("live hotspot slot")
    }

    fn mark_hotspot_dirty(&mut self, c: u32) {
        if !self.hotspots.dirty.contains(&c) {
            self.hotspots.dirty.push(c);
        }
    }

    /// `/turf/open/hotspot_expose(exposed_temperature, exposed_volume, soh)`: lights a fire
    /// when the air has oxygen and fuel and the heat suffices, or feeds the one burning there
    /// (`soh`). True when a hotspot was created.
    pub fn hotspot_expose(
        &mut self,
        tile: u32,
        exposed_temperature: f64,
        exposed_volume: f64,
        soh: bool,
    ) -> bool {
        let exposed_temperature = exposed_temperature.max(TCMB);
        let c = self.cell_of[tile as usize];
        if c == NONE {
            return false;
        }
        let air = &self.cells[c as usize].mix;
        if air.get(GasId::O2) < 0.5 {
            return false;
        }
        let plas_trit_h2 = air.get(GasId::PLASMA) > 0.5
            || air.get(GasId::TRITIUM) > 0.5
            || air.get(GasId::HYDROGEN) > 0.5;
        let freon = air.get(GasId::FREON) > 0.5;
        let slot = self.cells[c as usize].hotspot;
        if slot != NONE {
            if soh {
                let hotspot = self.hotspot_mut(slot);
                if plas_trit_h2 {
                    hotspot.temperature = hotspot.temperature.max(exposed_temperature);
                    hotspot.volume = hotspot.volume.max(exposed_volume);
                } else if freon {
                    hotspot.temperature = hotspot.temperature.min(exposed_temperature);
                    hotspot.volume = hotspot.volume.max(exposed_volume);
                }
            }
            return false;
        }
        if (exposed_temperature > PLASMA_MINIMUM_BURN_TEMPERATURE && plas_trit_h2)
            || (exposed_temperature < FREON_MAXIMUM_BURN_TEMPERATURE && freon)
        {
            self.new_hotspot(c, exposed_volume * 25.0, exposed_temperature);
            self.add_to_active(c, false);
            return true;
        }
        false
    }

    /// `/obj/effect/hotspot/Initialize(mapload, starting_volume, starting_temperature)`.
    fn new_hotspot(&mut self, c: u32, volume: f64, temperature: f64) {
        let serial = self.hotspots.next_serial;
        self.hotspots.next_serial += 1;
        let dir = self.hotspots.pick_cardinal();
        let hotspot = Hotspot {
            cell: c,
            volume,
            temperature,
            just_spawned: true,
            bypassing: false,
            cold_fire: temperature <= FREON_MAXIMUM_BURN_TEMPERATURE,
            stage: FireStage::Light,
            dir,
            color_temperature: temperature,
            visual_update_tick: 0,
            serial,
        };
        let slot = match self.hotspots.free.pop() {
            Some(slot) => {
                self.hotspots.slots[slot as usize] = Some(hotspot);
                slot
            }
            None => {
                self.hotspots.slots.push(Some(hotspot));
                (self.hotspots.slots.len() - 1) as u32
            }
        };
        self.hotspots.order.push((slot, serial));
        self.cells[c as usize].hotspot = slot;
        self.perform_exposure(slot);
        // `air_update_turf(FALSE, FALSE)`.
        self.add_to_active(c, false);
        // Spawned on a turf that already processed this fire: no protection from the next
        // process (`just_spawned = (our_turf.current_cycle < SSair.times_fired)`); between
        // fires every turf is behind the next one.
        let behind = !self.in_fire || self.cells[c as usize].current_cycle < self.fire_count;
        let hotspot = self.hotspot_mut(slot);
        hotspot.just_spawned = behind;
        hotspot.color_temperature = hotspot.temperature;
        self.mark_hotspot_dirty(c);
    }

    /// `perform_exposure`: the first tick (or a small fire) reacts a `volume / CELL_VOLUME`
    /// sample of the turf's air at the hotspot's temperature; a bypassing or cold fire follows
    /// the turf's air. Volume and temperature then come from that air's fire reactions, and
    /// everything on the turf is exposed (not for cold fires).
    fn perform_exposure(&mut self, slot: u32) {
        let (c, volume, temperature, just_spawned, cold_fire) = {
            let h = self.hotspots.slots[slot as usize]
                .as_ref()
                .expect("live hotspot slot");
            (h.cell, h.volume, h.temperature, h.just_spawned, h.cold_fire)
        };
        let bypassing = !just_spawned && volume > CELL_VOLUME * 0.95;
        let reference = if bypassing || cold_fire {
            let air = &self.cells[c as usize].mix;
            (air.hotspot_growth(), air.temperature)
        } else {
            let mut affected = self.cells[c as usize]
                .mix
                .remove_ratio(volume / CELL_VOLUME);
            affected.temperature = temperature;
            affected.react(Holder::Hotspot, CELL_VOLUME, &mut Exposures::default());
            let reference = (affected.hotspot_growth(), affected.temperature);
            // `location.assume_air(affected)`.
            self.cells[c as usize].mix.merge(&affected);
            self.update_visuals(c);
            self.add_to_active(c, false);
            reference
        };
        let tile = self.cells[c as usize].tile;
        let hotspot = self.hotspot_mut(slot);
        hotspot.bypassing = bypassing;
        (hotspot.volume, hotspot.temperature) = reference;
        if !cold_fire {
            let exposure = FireExposure {
                tile,
                temperature: hotspot.temperature,
                volume: hotspot.volume,
            };
            self.hotspots.exposures.push(exposure);
        }
    }

    /// `SSair.process_hotspots`: every hotspot that existed when the phase began, newest first.
    pub(super) fn process_hotspots(&mut self) {
        let slots = &self.hotspots.slots;
        self.hotspots.order.retain(|&(slot, serial)| {
            slots[slot as usize]
                .as_ref()
                .is_some_and(|h| h.serial == serial)
        });
        let run = self.hotspots.order.clone();
        for &(slot, serial) in run.iter().rev() {
            if self.hotspots.slots[slot as usize]
                .as_ref()
                .is_some_and(|h| h.serial == serial)
            {
                self.process_hotspot(slot);
            }
        }
    }

    /// `/obj/effect/hotspot/process`.
    fn process_hotspot(&mut self, slot: u32) {
        let hotspot = self.hotspot_mut(slot);
        if hotspot.just_spawned {
            hotspot.just_spawned = false;
            return;
        }
        let c = hotspot.cell;
        hotspot.cold_fire = hotspot.temperature <= FREON_MAXIMUM_BURN_TEMPERATURE;
        let dying = (hotspot.temperature < FIRE_MINIMUM_TEMPERATURE_TO_EXIST && !hotspot.cold_fire)
            || hotspot.volume <= 1.0;
        let g = self.cells[c as usize].group;
        if g != NONE {
            self.reset_cooldowns(g);
        }
        let air = &self.cells[c as usize].mix;
        let no_fuel = insufficient(air, GasId::PLASMA)
            && insufficient(air, GasId::TRITIUM)
            && insufficient(air, GasId::HYDROGEN)
            && insufficient(air, GasId::FREON);
        if dying || no_fuel || insufficient(air, GasId::O2) {
            self.remove_hotspot(slot);
            return;
        }
        self.perform_exposure(slot);

        let hotspot = self.hotspot_mut(slot);
        let stage = if hotspot.bypassing {
            FireStage::Heavy
        } else if hotspot.volume > CELL_VOLUME * 0.4 {
            FireStage::Medium
        } else {
            FireStage::Light
        };
        let (bypassing, cold_fire) = (hotspot.bypassing, hotspot.cold_fire);
        let restage = stage != hotspot.stage;
        // `set_fire_stage`: a new stage turns it and recolors it.
        if restage {
            hotspot.stage = stage;
        }
        let recolor = hotspot.visual_update_tick.is_multiple_of(7);
        hotspot.visual_update_tick += 1;
        if restage {
            let dir = self.hotspots.pick_cardinal();
            self.hotspot_mut(slot).dir = dir;
        }
        if restage || recolor {
            let hotspot = self.hotspot_mut(slot);
            hotspot.color_temperature = hotspot.temperature;
            self.mark_hotspot_dirty(c);
        }
        if bypassing {
            // Radiated heat spreads it.
            let air_temperature = self.cells[c as usize].mix.temperature;
            if air_temperature > FIRE_MINIMUM_TEMPERATURE_TO_SPREAD || cold_fire {
                let scale = if cold_fire {
                    COLD_FIRE_SPREAD_RADIOSITY_SCALE
                } else {
                    FIRE_SPREAD_RADIOSITY_SCALE
                };
                let radiated = air_temperature * scale;
                let tile = self.cells[c as usize].tile;
                for dir in Dir::CARDINALS {
                    if self.cells[c as usize].adj & dir as u8 == 0 {
                        continue;
                    }
                    let Some(n) = self.neighbor(tile, dir) else {
                        continue;
                    };
                    let e = self.cell_of[n as usize];
                    if e != NONE && self.cells[e as usize].hotspot == NONE {
                        self.hotspot_expose(n, radiated, CELL_VOLUME / 4.0, false);
                    }
                }
            }
        }
    }

    /// `/obj/effect/hotspot/Destroy`.
    fn remove_hotspot(&mut self, slot: u32) {
        let Some(hotspot) = self.hotspots.slots[slot as usize].take() else {
            return;
        };
        self.hotspots.free.push(slot);
        let c = hotspot.cell;
        if self.cells[c as usize].hotspot == slot {
            self.cells[c as usize].hotspot = NONE;
        }
        self.mark_hotspot_dirty(c);
    }

    /// The turf goes away (`ChangeTurf` deletes its hotspot).
    pub(super) fn remove_hotspot_of(&mut self, c: u32) {
        let slot = self.cells[c as usize].hotspot;
        if slot != NONE {
            self.remove_hotspot(slot);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TileKind;
    use crate::level::PressureDelta;

    fn room(width: u32, air: &str) -> LevelAir {
        let mut level = LevelAir::new(width, 1);
        let mix = GasMixture::parse(air).unwrap();
        for x in 0..width {
            level.set_tile(x, TileKind::Open, Some(mix.clone()), None);
        }
        level.setup();
        level
    }

    fn fire(level: &mut LevelAir) {
        let mut out: Vec<PressureDelta> = Vec::new();
        level.fire(&mut out);
    }

    #[test]
    fn no_fuel_no_fire() {
        let mut level = room(3, "o2=22;n2=82;TEMP=293.15");
        assert!(!level.hotspot_expose(1, 700.0, 5.0, false));
        // Plasma but no oxygen.
        let mut level = room(3, "plasma=20;TEMP=293.15");
        assert!(!level.hotspot_expose(1, 700.0, 5.0, false));
        // Fuel and oxygen, too cold to light (<= 373.15 K).
        let mut level = room(3, "plasma=20;o2=20;TEMP=293.15");
        assert!(!level.hotspot_expose(1, 373.0, 5.0, false));
        assert_eq!(level.hotspot_count(), 0);
    }

    #[test]
    fn welder_flame_lights_plasma_and_it_burns_out() {
        // A welder's `open_flame()`: 700 K, 5 L -> a 125 L hotspot. Its first exposure takes
        // 125/2500 of the air (1 plasma, 1 O2 of 20 each), reacts it at 700 K and puts it back.
        let mut level = room(3, "plasma=20;o2=20;TEMP=293.15");
        assert!(level.hotspot_expose(1, 700.0, 5.0, false));
        let h = level.hotspot(1).unwrap().clone();
        // Sample at 700 K: scale (700-373.15)/1270 = 0.257362; O2/plasma = 1 < 10:
        // rate = (1/10)/9 * 0.257362 = 0.00285958; results = rate * (1 + 1.142638).
        let scale = (700.0 - 373.15) / 1270.0;
        let rate = 0.1 / 9.0 * scale;
        let growth = rate * (1.0 + (1.4 - scale)) * 40000.0;
        assert!((h.volume - growth).abs() < 0.05, "{} vs {growth}", h.volume);
        assert!(h.temperature > 700.0 && h.just_spawned && !h.bypassing);
        let mut exposures = Vec::new();
        level.drain_exposures(|e| exposures.push(e));
        assert_eq!(exposures.len(), 1);
        assert_eq!(exposures[0].tile, 1);
        // Burns: the turf stays active while the fire lasts, plasma goes down, heat goes up.
        let mut max_temp: f64 = 0.0;
        for _ in 0..200 {
            fire(&mut level);
            max_temp = max_temp.max(level.air(1).unwrap().temperature);
            if level.hotspot_count() == 0 && level.active_count() == 0 {
                break;
            }
        }
        let total: f64 = (0..3)
            .map(|t| level.air(t).unwrap().get(GasId::PLASMA))
            .sum();
        assert!(total < 60.0 - 5.0, "plasma burnt: {total}");
        assert!(max_temp > 1000.0, "it got hot: {max_temp}");
        // It spread to both neighbours at some point and burnt out at last (no fuel or O2).
        assert_eq!(level.hotspot_count(), 0);
    }

    #[test]
    fn a_new_hotspot_skips_its_first_process() {
        let mut level = room(1, "plasma=20;o2=20;TEMP=293.15");
        level.hotspot_expose(0, 700.0, 5.0, false);
        assert!(level.hotspot(0).unwrap().just_spawned);
        fire(&mut level);
        // Protection used up, nothing else changed by the hotspot pass.
        let h = level.hotspot(0).unwrap();
        assert!(!h.just_spawned);
    }

    #[test]
    fn hotspot_expose_with_soh_feeds_the_fire() {
        let mut level = room(1, "plasma=20;o2=20;TEMP=293.15");
        level.hotspot_expose(0, 700.0, 5.0, false);
        let before = level.hotspot(0).unwrap().clone();
        assert!(!level.hotspot_expose(0, 5000.0, 2500.0, false));
        assert_eq!(level.hotspot(0).unwrap(), &before, "no soh: unchanged");
        level.hotspot_expose(0, 5000.0, 2500.0, true);
        let h = level.hotspot(0).unwrap();
        assert_eq!((h.temperature, h.volume), (5000.0, 2500.0));
    }

    #[test]
    fn changing_the_turf_removes_its_hotspot() {
        let mut level = room(2, "plasma=20;o2=20;TEMP=293.15");
        level.hotspot_expose(0, 700.0, 5.0, false);
        level.drain_hotspot_visuals(|_, h| assert!(h.is_some()));
        level.change_tile(0, TileKind::Blocked, None);
        assert_eq!(level.hotspot_count(), 0);
        let mut seen = Vec::new();
        level.drain_hotspot_visuals(|t, h| seen.push((t, h.is_some())));
        assert_eq!(seen, [(0, false)]);
    }
}
