//! tg `/obj/effect/hotspot` and `/turf/open/hotspot_expose`
//! (`code/modules/atmospherics/environmental/LINDA_fire.dm`): a fire on a turf. A hotspot
//! samples part of the turf's air at its own temperature and reacts it (`perform_exposure`);
//! once it fills the cell it "bypasses" and follows the turf's air, radiating heat to the
//! neighbours (spreading). It keeps its turf processing and burns out without fuel or oxygen.
//!
//! Hotspots live in their level ([`Hotspots`]); what they do to things on their turf leaves as
//! [`FireExposure`]s (tg `fire_act(temperature, volume)` of every atom there), their looks as
//! dirty tiles for the visuals.
//!
//! Touching hotspots form a [`HotGroup`] (tg `/datum/hot_group`), which places the looping fire
//! sound in its middle once it has more than [`MIN_SIZE_SOUND`] spots.

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
    /// `our_hot_group`: its slot in [`Hotspots::groups`].
    group: u32,
}

/// `MIN_SIZE_SOUND`: a group plays its sound only above this many spots.
pub const MIN_SIZE_SOUND: usize = 2;
/// `tiles_limit`: groups this big do not merge.
pub const HOT_GROUP_TILES_LIMIT: usize = 80;
/// `update_sound_center` cooldown, 5 s in fires.
const SOUND_CENTER_COOLDOWN_FIRES: u32 = 10;

/// tg `/datum/hot_group`: hotspots that spread from each other.
#[derive(Clone, Debug, Default)]
struct HotGroup {
    spots: Vec<u32>,
    /// `x_coord`, `y_coord`: one entry per spot, removed by value (`-=`).
    xs: Vec<i32>,
    ys: Vec<i32>,
    /// First fire the sound center may move again.
    cooldown_until: u32,
    /// The looping sound once started: its tile and `drop_off_dist`.
    sound: Option<(u32, i32)>,
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
    groups: Vec<Option<HotGroup>>,
    free_groups: Vec<u32>,
    /// Tiles a heavy fire burnt (`burn_tile()`), each once per cell.
    burns: Vec<u32>,
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
            groups: Vec::new(),
            free_groups: Vec::new(),
            burns: Vec::new(),
        }
    }

    pub(super) fn clear_burns(&mut self) {
        self.burns.clear();
    }

    fn next_random(&mut self) -> u64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng
    }

    /// `pick(GLOB.cardinals)` (xorshift; the looks only).
    fn pick_cardinal(&mut self) -> Dir {
        Dir::CARDINALS[(self.next_random() % 4) as usize]
    }

    fn new_group(&mut self) -> u32 {
        match self.free_groups.pop() {
            Some(g) => {
                self.groups[g as usize] = Some(HotGroup::default());
                g
            }
            None => {
                self.groups.push(Some(HotGroup::default()));
                (self.groups.len() - 1) as u32
            }
        }
    }

    fn group(&mut self, g: u32) -> &mut HotGroup {
        self.groups[g as usize]
            .as_mut()
            .expect("hotspots point at live groups")
    }

    fn free_group(&mut self, g: u32) {
        self.groups[g as usize] = None;
        self.free_groups.push(g);
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

    /// Tiles whose floor a heavy (non-freon) fire burnt since the last call (tg `burn_tile()`).
    pub fn drain_burns(&mut self, mut f: impl FnMut(u32)) {
        for tile in self.hotspots.burns.drain(..) {
            f(tile);
        }
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
            group: NONE,
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
        self.join_hot_group(slot);
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
            let mut exposures = Exposures::default();
            affected.react(Holder::Hotspot, CELL_VOLUME, &mut exposures);
            let tile = self.cells[c as usize].tile;
            self.radiation
                .extend(exposures.radiation.iter().flatten().map(|&r| (tile, r)));
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
        if bypassing && !cold_fire && !self.cells[c as usize].burn_reported {
            self.cells[c as usize].burn_reported = true;
            self.hotspots.burns.push(self.cells[c as usize].tile);
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
        if hotspot.group != NONE {
            self.remove_from_hot_group(hotspot.group, slot, c);
        }
        if self.cells[c as usize].hotspot == slot {
            self.cells[c as usize].hotspot = NONE;
        }
        self.mark_hotspot_dirty(c);
    }

    /// The hot groups' looping fire sounds: the tile each plays on and its `drop_off_dist`
    /// (`falloff_distance` and `extra_range` of the sound).
    pub fn fire_sounds(&self) -> impl Iterator<Item = (u32, i32)> + '_ {
        self.hotspots
            .groups
            .iter()
            .flatten()
            .filter_map(|g| g.sound)
    }

    pub fn hot_group_count(&self) -> usize {
        self.hotspots.groups.len() - self.hotspots.free_groups.len()
    }

    /// `/obj/effect/hotspot/Initialize`: join the group of an air-adjacent hotspot, merge
    /// with the other groups met, or start a group.
    fn join_hot_group(&mut self, slot: u32) {
        let c = self.hotspot_mut(slot).cell;
        let tile = self.cells[c as usize].tile;
        let mut ours = NONE;
        for dir in Dir::CARDINALS {
            if !self.adjacent(tile, dir) {
                continue;
            }
            let Some(other) = self
                .neighbor(tile, dir)
                .and_then(|n| self.cell(n))
                .map(|cell| cell.hotspot)
                .filter(|&h| h != NONE)
            else {
                continue;
            };
            let theirs = self.hotspots.slots[other as usize]
                .as_ref()
                .map_or(NONE, |h| h.group);
            if theirs == NONE {
                continue;
            }
            if ours == NONE {
                self.add_to_hot_group(theirs, slot);
                ours = theirs;
            } else if ours != theirs {
                ours = self.merge_hot_groups(ours, theirs);
            }
        }
        if ours == NONE {
            let g = self.hotspots.new_group();
            self.add_to_hot_group(g, slot);
        }
    }

    /// `add_to_group`.
    fn add_to_hot_group(&mut self, g: u32, slot: u32) {
        let c = self.hotspot_mut(slot).cell;
        self.hotspot_mut(slot).group = g;
        let (x, y) = self.pos(self.cells[c as usize].tile);
        let group = self.hotspots.group(g);
        group.spots.push(slot);
        group.xs.push(x);
        group.ys.push(y);
        self.maybe_update_fire_sound(g);
    }

    /// `merge_hot_groups`: the bigger group (a coin toss on ties) takes the other's spots
    /// unless either has reached [`HOT_GROUP_TILES_LIMIT`]. Returns the group `ours` is now.
    // tg checks the cooldown and size of `src` afterwards even when `src` was merged away
    // (and starts a sound on the deleted group); the surviving group is checked instead.
    fn merge_hot_groups(&mut self, ours: u32, theirs: u32) -> u32 {
        let (a, b) = (
            self.hotspots.group(ours).spots.len(),
            self.hotspots.group(theirs).spots.len(),
        );
        if a >= HOT_GROUP_TILES_LIMIT || b >= HOT_GROUP_TILES_LIMIT {
            return ours;
        }
        let keep_ours = a > b || (a == b && self.hotspots.next_random().is_multiple_of(2));
        let (saving, sacrificial) = if keep_ours {
            (ours, theirs)
        } else {
            (theirs, ours)
        };
        let gone = self.hotspots.groups[sacrificial as usize]
            .take()
            .expect("live group");
        self.hotspots.free_groups.push(sacrificial);
        for &spot in &gone.spots {
            self.hotspot_mut(spot).group = saving;
        }
        let group = self.hotspots.group(saving);
        group.spots.extend(gone.spots);
        group.xs.extend(gone.xs);
        group.ys.extend(gone.ys);
        self.maybe_update_fire_sound(saving);
        saving
    }

    /// `remove_from_group`: the group (and its sound) goes with its last spot.
    fn remove_from_hot_group(&mut self, g: u32, slot: u32, c: u32) {
        let (x, y) = self.pos(self.cells[c as usize].tile);
        let group = self.hotspots.group(g);
        group.spots.retain(|&s| s != slot);
        if let Some(i) = group.xs.iter().position(|&v| v == x) {
            group.xs.remove(i);
        }
        if let Some(i) = group.ys.iter().position(|&v| v == y) {
            group.ys.remove(i);
        }
        if group.spots.is_empty() {
            self.hotspots.free_group(g);
        }
    }

    /// The `update_sound_center` cooldown and size check, then `update_sound`: the middle of
    /// the group's bounding box (`round` is floor) and its larger side as the drop-off.
    fn maybe_update_fire_sound(&mut self, g: u32) {
        let fc = self.fire_count;
        let group = self.hotspots.group(g);
        if fc < group.cooldown_until || group.spots.len() <= MIN_SIZE_SOUND {
            return;
        }
        group.cooldown_until = fc + SOUND_CENTER_COOLDOWN_FIRES;
        let bounds = |v: &[i32]| {
            let (lo, hi) = (*v.iter().min()?, *v.iter().max()?);
            Some((lo, hi))
        };
        let (Some((x0, x1)), Some((y0, y1))) = (bounds(&group.xs), bounds(&group.ys)) else {
            return;
        };
        let middle = |lo: i32, hi: i32| (f64::from(lo + hi) / 2.0).floor() as i32;
        let drop_off = (y1 - y0).max(x1 - x0).max(1);
        let (x, y) = (middle(x0, x1), middle(y0, y1));
        if let Some(tile) = self.tile(x, y) {
            self.hotspots.group(g).sound = Some((tile, drop_off));
        }
    }

    /// The turf goes away (`ChangeTurf` deletes its hotspot).
    pub fn remove_hotspot_of(&mut self, c: u32) {
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

    /// Touching hotspots share a group; it places its sound once it has 3 spots, merges
    /// when a fire bridges two groups and goes away with the fire.
    #[test]
    fn hot_groups_follow_tg() {
        let mut level = room(5, "plasma=20;o2=20;TEMP=293.15");
        for x in [0, 1] {
            assert!(level.hotspot_expose(x, 700.0, 5.0, false));
        }
        assert_eq!(level.hot_group_count(), 1);
        assert_eq!(level.fire_sounds().count(), 0, "two spots are too few");
        assert!(level.hotspot_expose(2, 700.0, 5.0, false));
        // x 1..=3: the middle is x 2 (tile 1), the drop-off the width, 2.
        assert_eq!(level.fire_sounds().collect::<Vec<_>>(), [(1, 2)]);
        assert!(level.hotspot_expose(4, 700.0, 5.0, false));
        assert_eq!(level.hot_group_count(), 2, "not touching");
        assert!(level.hotspot_expose(3, 700.0, 5.0, false));
        assert_eq!(level.hot_group_count(), 1, "the bridge merged them");
        assert_eq!(level.fire_sounds().count(), 1);
        for x in 0..5 {
            level.change_tile(x, TileKind::Blocked, None);
        }
        assert_eq!(
            (level.hot_group_count(), level.fire_sounds().count()),
            (0, 0)
        );
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
