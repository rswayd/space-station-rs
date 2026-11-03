//! One z-level of tile air: tg `SSair` (`controllers/subsystem/air.dm`) for the turfs of that
//! level, `/turf/open/process_cell` and `/datum/excited_group` (`LINDA_turf_tile.dm`).
//!
//! Only open turfs with their own air are cells. Walls (`blocks_air`) are nothing; space is the
//! immutable vacuum and is never in the active list: a space turf joins excited groups like in
//! tg (and counts as an empty member in their breakdown), and what it does when it processes
//! (an immutable share with each neighbour, which also sets the neighbour to 2.7 K) is done from
//! the neighbour's [`LevelAir::process_cell`], as if the space turf processed first that fire,
//! which it does in tg while it is in a group (it joined the active list after the floor, and
//! the list runs from its end). Then the neighbour's `run_later` full share empties it. Work per
//! fire is proportional to the active cells and live groups; nothing scans the level.

use ss13_shared::Dir;

use crate::gas::{
    CELL_VOLUME, GAS_COUNT, GasMixture, HEAT_CAPACITY_VACUUM, MINIMUM_AIR_TO_SUSPEND,
    MINIMUM_MOLES_DELTA_TO_MOVE, OPEN_HEAT_TRANSFER_COEFFICIENT, TCMB,
};

/// `temperature_expose` of a processed turf is reported from this air temperature up: below
/// every `should_atmos_process` threshold of the ported atoms (tg sends it at any temperature).
pub const EXPOSE_REPORT_MIN: f64 = crate::gas::T0C + 100.0;

/// `EXCITED_GROUP_BREAKDOWN_CYCLES`: fires before a group averages its air.
pub const EXCITED_GROUP_BREAKDOWN_CYCLES: u32 = 5;
/// `EXCITED_GROUP_DISMANTLE_CYCLES`: fires without significant sharing before a group sleeps.
pub const EXCITED_GROUP_DISMANTLE_CYCLES: u32 = EXCITED_GROUP_BREAKDOWN_CYCLES * 2 + 1;

pub(crate) const NONE: u32 = u32::MAX;
const NO_PLANETARY: u16 = u16::MAX;
use crate::reactions::{Exposures, Holder, REACTING, STOP_REACTIONS, VOLATILE_REACTION};

pub mod conduct;
pub mod hotspot;

pub use conduct::Thermal;
pub use hotspot::{FireExposure, FireStage, Hotspot};

pub(crate) static VACUUM: GasMixture = GasMixture::vacuum();

/// What a tile is for air.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileKind {
    /// `blocks_air` (walls) or a turf that never lets air pass: no air of its own.
    Blocked,
    /// An open turf with its own mixture.
    Open,
    /// `/turf/open/space`: the shared immutable vacuum.
    Space,
}

/// tg `pressure_difference`/`pressure_direction` of a turf after a fire (spacewind input).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PressureDelta {
    /// Tile index in the level, see [`LevelAir::pos`].
    pub tile: u32,
    /// kPa.
    pub difference: f64,
    /// Towards the lower pressure.
    pub dir: Dir,
}

/// What [`LevelAir::setup`] found, tg `setup_allturfs`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SetupReport {
    pub cells: usize,
    /// Turfs whose air differed from a neighbour's at load ("active turfs at roundstart").
    pub differing: usize,
    /// Turfs connected to them, equalized with them and put to sleep.
    pub connected: usize,
    /// Groups among them that touched space and were vented at once.
    pub vented_groups: usize,
}

pub(crate) struct Cell {
    tile: u32,
    pub(crate) mix: GasMixture,
    planetary: u16,
    /// Cardinal dir bits of the tiles air flows to (open cells and space).
    adj: u8,
    adj_count: u8,
    excited: bool,
    active_slot: u32,
    group: u32,
    /// `significant_share_ticker`.
    ticker: u32,
    archived_cycle: u32,
    current_cycle: u32,
    /// Removed from the active list during this fire's turf pass (tg `currentrun -= T`).
    skip_run: u32,
    pressure_difference: f64,
    pressure_direction: Dir,
    in_high_pressure: bool,
    overlay: [u8; GAS_COUNT],
    visual_dirty: bool,
    /// tg `active_hotspot`: its slot in [`hotspot::Hotspots`], `NONE` without.
    hotspot: u32,
    /// A heavy fire has burnt this turf (reported once; a new turf is a new cell).
    burn_reported: bool,
}

#[derive(Default)]
struct Group {
    turfs: Vec<u32>,
    /// Space tiles in the group (tg adds space turfs to `turf_list` too).
    space: Vec<u32>,
    breakdown_cooldown: u32,
    dismantle_cooldown: u32,
    reactions: u8,
    /// Index in `live`, `NONE` when free.
    slot: u32,
    /// Bumped on free, so snapshots can tell a reused slot from the group they saw.
    generation: u32,
}

pub struct LevelAir {
    width: u32,
    height: u32,
    kind: Vec<TileKind>,
    /// Per tile: cardinal edges its objects block (`CanAtmosPass` of windows, doors, ...).
    edge_block: Vec<u8>,
    cell_of: Vec<u32>,
    /// Per tile: the group a space tile is in.
    space_group: Vec<u32>,
    pub(crate) cells: Vec<Cell>,
    active: Vec<u32>,
    groups: Vec<Group>,
    free_groups: Vec<u32>,
    live_groups: Vec<u32>,
    planetary: Vec<(String, GasMixture)>,
    fire_count: u32,
    in_turf_pass: bool,
    high_pressure: Vec<u32>,
    visual_dirty: Vec<u32>,
    scratch: Vec<u32>,
    group_scratch: Vec<(u32, u32)>,
    /// Inside [`Self::fire`]: hotspots compare their turf's cycle with this fire.
    in_fire: bool,
    hotspots: hotspot::Hotspots,
    /// Per tile: the turf's own heat ([`conduct`]).
    solid: Vec<conduct::Solid>,
    /// Distinct (`thermal_conductivity`, `heat_capacity`) of the level's turfs.
    materials: Vec<(f64, f64)>,
    /// Per tile: cardinal edges whose blockers also stop heat (`block_superconductivity`).
    heat_block: Vec<u8>,
    /// tg `active_super_conductivity`, in insertion order.
    conducting: Vec<u32>,
    /// `temperature_expose(air, air.temperature)` at the end of `process_cell`, from
    /// [`EXPOSE_REPORT_MIN`]: (tile, temperature).
    exposed: Vec<(u32, f64)>,
    /// Turfs water vapor condensed on this fire.
    condensed: Vec<(u32, crate::reactions::Condensation)>,
}

impl LevelAir {
    /// A level of space.
    pub fn new(width: u32, height: u32) -> Self {
        let n = width as usize * height as usize;
        Self {
            width,
            height,
            kind: vec![TileKind::Space; n],
            edge_block: vec![0; n],
            cell_of: vec![NONE; n],
            space_group: vec![NONE; n],
            cells: Vec::new(),
            active: Vec::new(),
            groups: Vec::new(),
            free_groups: Vec::new(),
            live_groups: Vec::new(),
            planetary: Vec::new(),
            fire_count: 0,
            in_turf_pass: false,
            high_pressure: Vec::new(),
            visual_dirty: Vec::new(),
            scratch: Vec::new(),
            group_scratch: Vec::new(),
            in_fire: false,
            hotspots: hotspot::Hotspots::new(u64::from(width) << 32 | u64::from(height)),
            solid: vec![conduct::Solid::new(Thermal::SPACE, 0); n],
            materials: vec![
                (Thermal::SPACE.conductivity, Thermal::SPACE.heat_capacity),
                (Thermal::TURF.conductivity, Thermal::TURF.heat_capacity),
            ],
            heat_block: vec![0; n],
            conducting: Vec::new(),
            exposed: Vec::new(),
            condensed: Vec::new(),
        }
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Tile index of BYOND `(x, y)` (1-based).
    pub fn tile(&self, x: i32, y: i32) -> Option<u32> {
        let in_bounds = x >= 1 && y >= 1 && x as u32 <= self.width && y as u32 <= self.height;
        in_bounds.then(|| (y as u32 - 1) * self.width + (x as u32 - 1))
    }

    /// BYOND `(x, y)` of a tile index.
    pub fn pos(&self, tile: u32) -> (i32, i32) {
        (
            (tile % self.width) as i32 + 1,
            (tile / self.width) as i32 + 1,
        )
    }

    fn neighbor(&self, tile: u32, dir: Dir) -> Option<u32> {
        let (x, y) = self.pos(tile);
        let (dx, dy) = dir.offset();
        self.tile(x + dx, y + dy)
    }

    pub fn kind(&self, tile: u32) -> TileKind {
        self.kind[tile as usize]
    }

    /// Sets a tile before [`Self::setup`]: its kind and, for open tiles, its air and the
    /// planetary atmosphere (by gas string) it relaxes to.
    pub fn set_tile(
        &mut self,
        tile: u32,
        kind: TileKind,
        air: Option<GasMixture>,
        planetary: Option<(&str, &GasMixture)>,
    ) {
        let t = tile as usize;
        self.kind[t] = kind;
        self.set_thermal(
            tile,
            if kind == TileKind::Space {
                Thermal::SPACE
            } else {
                Thermal::TURF
            },
        );
        if kind != TileKind::Open {
            return;
        }
        let planetary = planetary.map_or(NO_PLANETARY, |(key, mix)| {
            let at = self
                .planetary
                .iter()
                .position(|(k, _)| k == key)
                .unwrap_or_else(|| {
                    let mut mix = mix.clone();
                    mix.archive();
                    self.planetary.push((key.to_owned(), mix));
                    self.planetary.len() - 1
                });
            at as u16
        });
        let mut mix = air.unwrap_or_default();
        mix.archive();
        if self.cell_of[t] != NONE {
            self.remove_hotspot_of(self.cell_of[t]);
        }
        let cell = Cell {
            tile,
            overlay: [0; GAS_COUNT],
            mix,
            planetary,
            adj: 0,
            adj_count: 0,
            excited: false,
            active_slot: NONE,
            group: NONE,
            ticker: 0,
            archived_cycle: 0,
            current_cycle: 0,
            skip_run: 0,
            pressure_difference: 0.0,
            pressure_direction: Dir::South,
            in_high_pressure: false,
            visual_dirty: false,
            hotspot: NONE,
            burn_reported: false,
        };
        if self.cell_of[t] == NONE {
            self.cell_of[t] = self.cells.len() as u32;
            self.cells.push(cell);
        } else {
            self.cells[self.cell_of[t] as usize] = cell;
        }
        // tg `setup_turf_visuals`.
        self.update_visuals(self.cell_of[t]);
    }

    /// tg `ChangeTurf` of a running level: the tile becomes `kind` with `air` (open tiles; the
    /// old cell's air is dropped), its adjacency is recomputed both ways and it and its air
    /// neighbours wake up. A tile that keeps its kind and air (`CHANGETURF_INHERIT_AIR` between
    /// open turfs) needs no call.
    pub fn change_tile(&mut self, tile: u32, kind: TileKind, air: Option<GasMixture>) {
        let t = tile as usize;
        let old = self.cell_of[t];
        if old != NONE {
            self.remove_hotspot_of(old);
            self.remove_from_active(old);
            let cell = &mut self.cells[old as usize];
            cell.adj = 0;
            cell.adj_count = 0;
            cell.overlay = [0; GAS_COUNT];
            // The overlay entity of the tile goes away with its gas.
            if !cell.visual_dirty {
                cell.visual_dirty = true;
                self.visual_dirty.push(old);
            }
            // shortcut: the detached cell stays in `cells` (a few hundred bytes per changed
            // tile); reuse freed cells if construction ever churns thousands of tiles.
            self.cell_of[t] = NONE;
        }
        self.kind[t] = kind;
        if kind == TileKind::Open {
            self.set_tile(tile, kind, air, None);
        } else {
            let thermal = if kind == TileKind::Space {
                Thermal::SPACE
            } else {
                Thermal::TURF
            };
            self.set_thermal(tile, thermal);
        }
        self.recalculate_adjacency(tile);
        self.activate(tile, true);
    }

    pub fn set_edge_block(&mut self, tile: u32, edges: u8) {
        self.edge_block[tile as usize] = edges;
    }

    pub(crate) fn cell(&self, tile: u32) -> Option<&Cell> {
        let c = *self.cell_of.get(tile as usize)?;
        (c != NONE).then(|| &self.cells[c as usize])
    }

    /// The tile's air: its own, the vacuum for space, `None` for walls.
    pub fn air(&self, tile: u32) -> Option<&GasMixture> {
        match self.kind.get(tile as usize)? {
            TileKind::Open => self.cell(tile).map(|c| &c.mix),
            TileKind::Space => Some(&VACUUM),
            TileKind::Blocked => None,
        }
    }

    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    pub fn group_count(&self) -> usize {
        self.live_groups.len()
    }

    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    /// Tiles of each kind: (blocked, open, space).
    pub fn kind_counts(&self) -> (usize, usize, usize) {
        let count = |k| self.kind.iter().filter(|&&x| x == k).count();
        (
            count(TileKind::Blocked),
            count(TileKind::Open),
            count(TileKind::Space),
        )
    }

    /// Air may flow from `tile` to its neighbour in `dir`.
    pub fn adjacent(&self, tile: u32, dir: Dir) -> bool {
        match self.cell(tile) {
            Some(c) => c.adj & dir as u8 != 0,
            None => self
                .neighbor(tile, dir)
                .and_then(|n| self.cell(n))
                .is_some_and(|c| c.adj & dir.reverse() as u8 != 0),
        }
    }

    /// tg `CANATMOSPASS` both ways between `tile` and its neighbour in `dir`. Two space tiles
    /// never connect (space turfs do not compute their own adjacency).
    fn passes(&self, tile: u32, dir: Dir) -> Option<u32> {
        let n = self.neighbor(tile, dir)?;
        let (a, b) = (self.kind(tile), self.kind(n));
        let open = a != TileKind::Blocked
            && b != TileKind::Blocked
            && (a == TileKind::Open || b == TileKind::Open)
            && self.edge_block[tile as usize] & dir as u8 == 0
            && self.edge_block[n as usize] & dir.reverse() as u8 == 0;
        open.then_some(n)
    }

    /// tg `immediate_calculate_adjacent_turfs` for one tile: its four edges, both sides.
    pub fn recalculate_adjacency(&mut self, tile: u32) {
        for dir in Dir::CARDINALS {
            let n = self.neighbor(tile, dir);
            let open = self.passes(tile, dir).is_some();
            self.set_adj_bit(tile, dir, open);
            if let Some(n) = n {
                self.set_adj_bit(n, dir.reverse(), open);
            }
        }
    }

    fn set_adj_bit(&mut self, tile: u32, dir: Dir, open: bool) {
        let c = self.cell_of[tile as usize];
        if c == NONE {
            return;
        }
        let cell = &mut self.cells[c as usize];
        if open {
            cell.adj |= dir as u8;
        } else {
            cell.adj &= !(dir as u8);
        }
        cell.adj_count = cell.adj.count_ones() as u8;
    }

    /// Open cells around a space tile that air reaches it from (its `atmos_adjacent_turfs`).
    fn space_adj_count(&self, space: u32) -> u32 {
        Dir::CARDINALS
            .into_iter()
            .filter(|&d| self.adjacent(space, d))
            .count() as u32
    }

    /// tg `setup_allturfs`: adjacency of every tile, then turfs whose air differs from a
    /// neighbour's are grouped with everything connected to them, equalized and put to sleep.
    pub fn setup(&mut self) -> SetupReport {
        self.fire_count += 1;
        for c in 0..self.cells.len() {
            let tile = self.cells[c].tile;
            let mut adj = 0;
            for dir in Dir::CARDINALS {
                if self.passes(tile, dir).is_some() {
                    adj |= dir as u8;
                }
            }
            self.cells[c].adj = adj;
            self.cells[c].adj_count = adj.count_ones() as u8;
        }
        // Every pair once: a cell skips neighbours checked before it.
        let mut differing = Vec::new();
        let mut done = vec![false; self.cells.len()];
        for c in 0..self.cells.len() {
            done[c] = true;
            let tile = self.cells[c].tile;
            for dir in Dir::CARDINALS {
                if self.cells[c].adj & dir as u8 == 0 {
                    continue;
                }
                let n = self.neighbor(tile, dir).expect("adjacent tiles exist");
                let other = match self.cell_of[n as usize] {
                    NONE => &VACUUM,
                    o if done[o as usize] => continue,
                    o => &self.cells[o as usize].mix,
                };
                if self.cells[c].mix.compare(other, false) {
                    let o = self.cell_of[n as usize];
                    for cell in [c as u32, o].into_iter().filter(|&x| x != NONE) {
                        if !self.cells[cell as usize].excited {
                            self.cells[cell as usize].excited = true;
                            differing.push(cell);
                        }
                    }
                    break;
                }
            }
        }
        let mut report = SetupReport {
            cells: self.cells.len(),
            differing: differing.len(),
            ..SetupReport::default()
        };
        if differing.is_empty() {
            return report;
        }
        // `resolve_active_graph` until no new turf joins.
        let mut connected = differing.len();
        let mut to_check = differing;
        while !to_check.is_empty() {
            let mut next = Vec::new();
            for &c in &to_check {
                let mut g = self.cells[c as usize].group;
                if g == NONE {
                    g = self.new_group();
                    self.add_turf(g, c);
                }
                let tile = self.cells[c as usize].tile;
                for dir in Dir::CARDINALS {
                    if self.cells[c as usize].adj & dir as u8 == 0 {
                        continue;
                    }
                    let n = self.neighbor(tile, dir).expect("adjacent tiles exist");
                    let o = self.cell_of[n as usize];
                    // Space joins the group too, which makes its breakdown a vacuum.
                    if o == NONE {
                        let sg = self.space_group[n as usize];
                        if sg == NONE {
                            self.add_space(g, n);
                        } else if sg != g {
                            self.merge_groups(g, sg);
                            g = self.cells[c as usize].group;
                        }
                        continue;
                    }
                    let og = self.cells[o as usize].group;
                    if og != NONE {
                        if og != g {
                            self.merge_groups(g, og);
                            g = self.cells[c as usize].group;
                        }
                    } else {
                        self.add_turf(g, o);
                    }
                    if !self.cells[o as usize].excited {
                        self.cells[o as usize].excited = true;
                        next.push(o);
                    }
                }
            }
            connected += next.len();
            to_check = next;
        }
        report.connected = connected - report.differing;
        let groups: Vec<u32> = self.live_groups.clone();
        for g in groups {
            let vent = !self.groups[g as usize].space.is_empty();
            if vent {
                report.vented_groups += 1;
            }
            self.self_breakdown(g, Some(vent), false);
            self.dismantle(g);
        }
        report
    }

    /// One SSair fire for this level: active turfs, then excited groups, then the pressure
    /// differences of the fire (tg `high_pressure_delta`), returned and reset.
    pub fn fire(&mut self, pressure: &mut Vec<PressureDelta>) {
        self.fire_count += 1;
        // Reports of the last fire nobody read (a world without the game side).
        self.exposed.clear();
        self.condensed.clear();
        self.hotspots.clear_burns();
        self.in_fire = true;
        let fc = self.fire_count;
        let mut run = std::mem::take(&mut self.scratch);
        run.clear();
        run.extend_from_slice(&self.active);
        self.in_turf_pass = true;
        for &c in run.iter().rev() {
            let cell = &self.cells[c as usize];
            if cell.excited && cell.skip_run != fc {
                self.process_cell(c);
            }
        }
        self.in_turf_pass = false;
        self.scratch = run;
        self.process_hotspots();
        self.process_excited_groups();
        self.process_super_conductivity();
        self.in_fire = false;
        for c in self.high_pressure.drain(..) {
            let cell = &mut self.cells[c as usize];
            pressure.push(PressureDelta {
                tile: cell.tile,
                difference: cell.pressure_difference,
                dir: cell.pressure_direction,
            });
            cell.pressure_difference = 0.0;
            cell.in_high_pressure = false;
        }
    }

    /// Tiles whose gas overlay steps changed since the last call, with the new steps.
    pub fn drain_visuals(&mut self, mut f: impl FnMut(u32, &[u8; GAS_COUNT])) {
        for c in self.visual_dirty.drain(..) {
            let cell = &mut self.cells[c as usize];
            cell.visual_dirty = false;
            f(cell.tile, &cell.overlay);
        }
    }

    /// The gas overlay steps of a tile (all zero when nothing shows).
    pub fn overlay(&self, tile: u32) -> [u8; GAS_COUNT] {
        self.cell(tile).map_or([0; GAS_COUNT], |c| c.overlay)
    }

    fn archive(&mut self, c: u32) {
        let fc = self.fire_count;
        let cell = &mut self.cells[c as usize];
        cell.mix.archive();
        cell.archived_cycle = fc;
        let tile = cell.tile;
        self.solid[tile as usize].archive(fc);
    }

    fn pair(&mut self, a: u32, b: u32) -> (&mut Cell, &mut Cell) {
        let (a, b) = (a as usize, b as usize);
        if a < b {
            let (lo, hi) = self.cells.split_at_mut(b);
            (&mut lo[a], &mut hi[0])
        } else {
            let (lo, hi) = self.cells.split_at_mut(a);
            (&mut hi[0], &mut lo[b])
        }
    }

    /// `/turf/open/process_cell`.
    fn process_cell(&mut self, c: u32) {
        let fc = self.fire_count;
        let tile = self.cells[c as usize].tile;
        // Space neighbours, and whether each processed (shared) already this fire.
        let mut space_dirs = [None; 4];
        for (slot, dir) in space_dirs.iter_mut().zip(Dir::CARDINALS) {
            if self.cells[c as usize].adj & dir as u8 == 0 {
                continue;
            }
            let n = self.neighbor(tile, dir).expect("adjacent tiles exist");
            if self.cell_of[n as usize] == NONE {
                let active = self.space_group[n as usize] != NONE;
                *slot = Some((n, dir, active));
                if active {
                    self.space_processes(c, n, dir);
                }
            }
        }

        if self.cells[c as usize].archived_cycle < fc {
            self.archive(c);
        }
        self.cells[c as usize].current_cycle = fc;
        let mut ticker = self.cells[c as usize].ticker + 1;
        let our_share_coeff = 1.0 / f64::from(self.cells[c as usize].adj_count + 1);

        for dir in Dir::CARDINALS {
            if self.cells[c as usize].adj & dir as u8 == 0 {
                continue;
            }
            let n = self.neighbor(tile, dir).expect("adjacent tiles exist");
            let e = self.cell_of[n as usize];
            if e == NONE {
                let processed = space_dirs
                    .iter()
                    .flatten()
                    .any(|&(s, _, active)| s == n && active);
                if !processed {
                    self.share_with_idle_space(c, n, dir, our_share_coeff, &mut ticker);
                }
                continue;
            }
            if fc <= self.cells[e as usize].current_cycle {
                continue;
            }
            self.archive(e);
            let mut our_group = self.cells[c as usize].group;
            let enemy_group = self.cells[e as usize].group;
            if our_group != NONE && enemy_group != NONE && our_group != enemy_group {
                self.merge_groups(our_group, enemy_group);
                our_group = self.cells[c as usize].group;
            }
            let should_share =
                if our_group != NONE && enemy_group != NONE && self.cells[e as usize].excited {
                    true
                } else if self.cells[c as usize]
                    .mix
                    .compare(&self.cells[e as usize].mix, true)
                {
                    self.add_to_active(e, false);
                    let existing = if our_group != NONE {
                        our_group
                    } else if enemy_group != NONE {
                        enemy_group
                    } else {
                        self.new_group()
                    };
                    if our_group == NONE {
                        self.add_turf(existing, c);
                    }
                    if enemy_group == NONE {
                        self.add_turf(existing, e);
                    }
                    true
                } else {
                    false
                };
            if !should_share {
                continue;
            }
            let enemy_coeff = 1.0 / f64::from(self.cells[e as usize].adj_count + 1);
            let (ours, theirs) = self.pair(c, e);
            let difference = ours
                .mix
                .share(&mut theirs.mix, our_share_coeff, enemy_coeff);
            let last_share = ours.mix.last_share;
            match difference {
                Some(d) if d > 0.0 => self.consider_pressure_difference(c, dir, d),
                Some(d) if d < 0.0 => self.consider_pressure_difference(e, dir.reverse(), -d),
                _ => {}
            }
            self.last_share_check(c, last_share, &mut ticker, Some(e));
        }

        let planetary = self.cells[c as usize].planetary;
        if planetary != NO_PLANETARY {
            self.archive(c);
            let atmosphere = self.planetary[usize::from(planetary)].1.clone();
            if self.cells[c as usize].mix.compare(&atmosphere, true) {
                if self.cells[c as usize].group == NONE {
                    let g = self.new_group();
                    self.add_turf(g, c);
                }
                let mut sink = atmosphere.clone();
                let mut conductor = atmosphere.clone();
                let mix = &mut self.cells[c as usize].mix;
                mix.share(&mut sink, 0.8, 0.8);
                let heat_capacity = conductor.heat_capacity() * 5.0;
                mix.temperature_share(
                    &mut conductor,
                    OPEN_HEAT_TRANSFER_COEFFICIENT,
                    Some(heat_capacity),
                );
                let last_share = mix.last_share;
                self.last_share_check(c, last_share, &mut ticker, None);
            }
        }

        for (_, dir, _) in space_dirs.into_iter().flatten() {
            self.archive(c);
            let mut space = VACUUM.clone();
            let mix = &mut self.cells[c as usize].mix;
            let difference = mix.share(&mut space, 1.0, 1.0);
            let last_share = mix.last_share;
            self.last_share_check(c, last_share, &mut ticker, None);
            if let Some(d) = difference.filter(|&d| d > 0.0) {
                self.consider_pressure_difference(c, dir, d);
            }
        }

        let mut exposures = Exposures::default();
        let reacting = self.cells[c as usize]
            .mix
            .react(Holder::Turf, CELL_VOLUME, &mut exposures);
        let g = self.cells[c as usize].group;
        if g != NONE {
            self.groups[g as usize].reactions |= reacting;
        }
        for &temperature in exposures.temperatures() {
            self.hotspot_expose(tile, temperature, CELL_VOLUME, false);
        }
        if let Some(condensed) = exposures.condensed {
            self.condensed.push((tile, condensed));
        }

        let conducting = self.consider_superconductivity(tile, true);
        self.update_visuals(c);
        if !conducting
            && self.cells[c as usize].hotspot == NONE
            && reacting & (REACTING | STOP_REACTIONS) == 0
        {
            if self.cells[c as usize].group == NONE {
                self.remove_from_active(c);
            }
            if ticker > EXCITED_GROUP_DISMANTLE_CYCLES {
                self.sleep_active_turf(c);
            }
        }
        self.cells[c as usize].ticker = ticker;
        let temperature = self.cells[c as usize].mix.temperature;
        if temperature >= EXPOSE_REPORT_MIN {
            self.exposed.push((tile, temperature));
        }
    }

    /// Turfs water vapor condensed on since the last call.
    pub fn drain_condensed(&mut self, mut f: impl FnMut(u32, crate::reactions::Condensation)) {
        for (tile, kind) in self.condensed.drain(..) {
            f(tile, kind);
        }
    }

    /// Turfs `temperature_expose`d since the last call (hot ones, see [`EXPOSE_REPORT_MIN`]).
    pub fn drain_exposed(&mut self, mut f: impl FnMut(u32, f64)) {
        for (tile, temperature) in self.exposed.drain(..) {
            f(tile, temperature);
        }
    }

    /// A space turf in a group processing before `c`: its `process_cell` towards `c`, sharing
    /// as `/datum/gas_mixture/immutable`, which also sets `c` to [`TCMB`].
    fn space_processes(&mut self, c: u32, space: u32, dir_to_space: Dir) {
        self.archive(c);
        let mut space_group = self.space_group[space as usize];
        let floor_group = self.cells[c as usize].group;
        if space_group != NONE && floor_group != NONE && space_group != floor_group {
            self.merge_groups(space_group, floor_group);
            space_group = self.space_group[space as usize];
        }
        let should_share =
            if space_group != NONE && floor_group != NONE && self.cells[c as usize].excited {
                true
            } else if VACUUM.compare(&self.cells[c as usize].mix, true) {
                self.add_to_active(c, false);
                let g = if space_group != NONE {
                    space_group
                } else if floor_group != NONE {
                    floor_group
                } else {
                    self.new_group()
                };
                if space_group == NONE {
                    self.add_space(g, space);
                }
                if floor_group == NONE {
                    self.add_turf(g, c);
                }
                true
            } else {
                false
            };
        if !should_share {
            return;
        }
        let space_coeff = 1.0 / f64::from(self.space_adj_count(space) + 1);
        let cell = &mut self.cells[c as usize];
        let our_coeff = 1.0 / f64::from(cell.adj_count + 1);
        let mut vacuum = VACUUM.clone();
        let difference = vacuum.share(&mut cell.mix, space_coeff, our_coeff);
        cell.mix.temperature = TCMB;
        if let Some(d) = difference.filter(|&d| d < 0.0) {
            self.consider_pressure_difference(c, dir_to_space, -d);
        }
        // `LAST_SHARE_CHECK` of the space turf; `c` is its enemy.
        let g = self.space_group[space as usize];
        if vacuum.last_share > MINIMUM_AIR_TO_SUSPEND {
            self.reset_cooldowns(g);
        } else if vacuum.last_share > MINIMUM_MOLES_DELTA_TO_MOVE {
            if g != NONE {
                self.groups[g as usize].dismantle_cooldown = 0;
            }
        } else {
            return;
        }
        self.cells[c as usize].ticker = 0;
    }

    /// `c`'s own pass over a space neighbour that is not processing (in no group): tg's group
    /// handling with the space turf as the enemy, which wakes it into the group when they differ.
    fn share_with_idle_space(
        &mut self,
        c: u32,
        space: u32,
        dir: Dir,
        our_coeff: f64,
        ticker: &mut u32,
    ) {
        if !self.cells[c as usize].mix.compare(&VACUUM, true) {
            return;
        }
        let mut g = self.cells[c as usize].group;
        if g == NONE {
            g = self.new_group();
            self.add_turf(g, c);
        }
        self.add_space(g, space);
        let space_coeff = 1.0 / f64::from(self.space_adj_count(space) + 1);
        let mut vacuum = VACUUM.clone();
        let mix = &mut self.cells[c as usize].mix;
        let difference = mix.share(&mut vacuum, our_coeff, space_coeff);
        let last_share = mix.last_share;
        if let Some(d) = difference.filter(|&d| d > 0.0) {
            self.consider_pressure_difference(c, dir, d);
        }
        self.last_share_check(c, last_share, ticker, None);
    }

    /// `LAST_SHARE_CHECK` / `PLANET_SHARE_CHECK`.
    fn last_share_check(&mut self, c: u32, last_share: f64, ticker: &mut u32, enemy: Option<u32>) {
        let g = self.cells[c as usize].group;
        if last_share > MINIMUM_AIR_TO_SUSPEND {
            self.reset_cooldowns(g);
        } else if last_share > MINIMUM_MOLES_DELTA_TO_MOVE {
            if g != NONE {
                self.groups[g as usize].dismantle_cooldown = 0;
            }
        } else {
            return;
        }
        *ticker = 0;
        if let Some(e) = enemy {
            self.cells[e as usize].ticker = 0;
        }
    }

    fn reset_cooldowns(&mut self, g: u32) {
        if g != NONE {
            let group = &mut self.groups[g as usize];
            group.breakdown_cooldown = 0;
            group.dismantle_cooldown = 0;
        }
    }

    /// `consider_pressure_difference`.
    fn consider_pressure_difference(&mut self, c: u32, dir: Dir, difference: f64) {
        let cell = &mut self.cells[c as usize];
        if !cell.in_high_pressure {
            cell.in_high_pressure = true;
            self.high_pressure.push(c);
        }
        if difference > cell.pressure_difference {
            cell.pressure_direction = dir;
            cell.pressure_difference = difference;
        }
    }

    pub(crate) fn update_visuals(&mut self, c: u32) {
        let cell = &mut self.cells[c as usize];
        let steps = cell.mix.overlay_steps();
        if steps != cell.overlay {
            cell.overlay = steps;
            if !cell.visual_dirty {
                cell.visual_dirty = true;
                self.visual_dirty.push(c);
            }
        }
    }

    /// `SSair.add_to_active` for a tile. Space and walls cannot process: like tg for a turf
    /// without air of its own, their air-adjacent cells are woken instead.
    pub fn activate(&mut self, tile: u32, blockchanges: bool) {
        let c = self.cell_of[tile as usize];
        if c != NONE {
            self.add_to_active(c, blockchanges);
            return;
        }
        for dir in Dir::CARDINALS {
            if self.adjacent(tile, dir) {
                let n = self.neighbor(tile, dir).expect("adjacent tiles exist");
                let o = self.cell_of[n as usize];
                if o != NONE {
                    self.add_to_active(o, true);
                }
            }
        }
    }

    /// `SSair.remove_from_active` for a tile (a door closing on it): stops it and breaks up
    /// its group, which re-forms from whatever still differs.
    pub fn deactivate(&mut self, tile: u32) {
        let c = self.cell_of[tile as usize];
        if c != NONE {
            self.remove_from_active(c);
        }
    }

    fn add_to_active(&mut self, c: u32, blockchanges: bool) {
        let cell = &mut self.cells[c as usize];
        cell.ticker = 0;
        let g = cell.group;
        if blockchanges && g != NONE {
            self.garbage_collect(g);
        }
        let cell = &mut self.cells[c as usize];
        if cell.excited {
            return;
        }
        cell.excited = true;
        cell.active_slot = self.active.len() as u32;
        self.active.push(c);
    }

    fn unlist_active(&mut self, c: u32) {
        let fc = self.fire_count;
        let in_pass = self.in_turf_pass;
        let cell = &mut self.cells[c as usize];
        cell.excited = false;
        if in_pass {
            cell.skip_run = fc;
        }
        let slot = cell.active_slot;
        if slot == NONE {
            return;
        }
        cell.active_slot = NONE;
        self.active.swap_remove(slot as usize);
        if let Some(&moved) = self.active.get(slot as usize) {
            self.cells[moved as usize].active_slot = slot;
        }
    }

    fn remove_from_active(&mut self, c: u32) {
        self.unlist_active(c);
        let g = self.cells[c as usize].group;
        if g != NONE {
            self.garbage_collect(g);
        }
    }

    fn sleep_active_turf(&mut self, c: u32) {
        self.unlist_active(c);
    }

    fn new_group(&mut self) -> u32 {
        let id = self.free_groups.pop().unwrap_or_else(|| {
            self.groups.push(Group::default());
            (self.groups.len() - 1) as u32
        });
        let group = &mut self.groups[id as usize];
        group.turfs.clear();
        group.space.clear();
        group.breakdown_cooldown = 0;
        group.dismantle_cooldown = 0;
        group.reactions = 0;
        group.slot = self.live_groups.len() as u32;
        self.live_groups.push(id);
        id
    }

    fn free_group(&mut self, g: u32) {
        let group = &mut self.groups[g as usize];
        group.turfs.clear();
        group.space.clear();
        group.generation = group.generation.wrapping_add(1);
        let slot = group.slot;
        group.slot = NONE;
        self.live_groups.swap_remove(slot as usize);
        if let Some(&moved) = self.live_groups.get(slot as usize) {
            self.groups[moved as usize].slot = slot;
        }
        self.free_groups.push(g);
    }

    fn add_turf(&mut self, g: u32, c: u32) {
        let group = &mut self.groups[g as usize];
        group.turfs.push(c);
        group.dismantle_cooldown = 0;
        self.cells[c as usize].group = g;
    }

    fn add_space(&mut self, g: u32, tile: u32) {
        let group = &mut self.groups[g as usize];
        group.space.push(tile);
        group.dismantle_cooldown = 0;
        self.space_group[tile as usize] = g;
    }

    /// `merge_groups`: the larger group takes the other's turfs.
    fn merge_groups(&mut self, a: u32, b: u32) {
        let len = |g: &Group| g.turfs.len() + g.space.len();
        let (keep, gone) = if len(&self.groups[a as usize]) > len(&self.groups[b as usize]) {
            (a, b)
        } else {
            (b, a)
        };
        let moved = std::mem::take(&mut self.groups[gone as usize].turfs);
        let moved_space = std::mem::take(&mut self.groups[gone as usize].space);
        for &c in &moved {
            self.cells[c as usize].group = keep;
        }
        for &t in &moved_space {
            self.space_group[t as usize] = keep;
        }
        let breakdown = self.groups[gone as usize].breakdown_cooldown;
        let group = &mut self.groups[keep as usize];
        group.turfs.extend_from_slice(&moved);
        group.space.extend_from_slice(&moved_space);
        group.breakdown_cooldown = group.breakdown_cooldown.min(breakdown);
        group.dismantle_cooldown = 0;
        self.free_group(gone);
    }

    /// `garbage_collect`: the turfs leave the group (they stay active) and it ends.
    fn garbage_collect(&mut self, g: u32) {
        let turfs = std::mem::take(&mut self.groups[g as usize].turfs);
        for &c in &turfs {
            self.cells[c as usize].group = NONE;
        }
        self.groups[g as usize].turfs = turfs;
        let space = std::mem::take(&mut self.groups[g as usize].space);
        for &t in &space {
            self.space_group[t as usize] = NONE;
        }
        self.free_group(g);
    }

    /// `dismantle`: every turf goes to sleep and the group ends.
    fn dismantle(&mut self, g: u32) {
        let turfs = std::mem::take(&mut self.groups[g as usize].turfs);
        for &c in &turfs {
            self.unlist_active(c);
            self.cells[c as usize].ticker = 0;
        }
        self.groups[g as usize].turfs = turfs;
        self.garbage_collect(g);
    }

    /// `self_breakdown`: every turf gets the group's average gas at its average temperature
    /// (energy-weighted). `roundstart` = `Some(vent)`: a group touching space (`vent`) or a
    /// planetary turf takes that immutable air instead.
    fn self_breakdown(&mut self, g: u32, roundstart: Option<bool>, poke_turfs: bool) {
        let turfs = std::mem::take(&mut self.groups[g as usize].turfs);
        let mut shared = GasMixture::vacuum();
        let mut immutable = false;
        if roundstart == Some(true) {
            immutable = true;
        } else if roundstart.is_some()
            && let Some(&c) = turfs
                .iter()
                .find(|&&c| self.cells[c as usize].planetary != NO_PLANETARY)
        {
            immutable = true;
            let p = self.cells[c as usize].planetary;
            shared.copy_from(&self.planetary[usize::from(p)].1);
        }
        if !immutable {
            // Space members count as empty 7000 J/K mixtures at 2.7 K.
            let space_members = self.groups[g as usize].space.len();
            let mut energy = TCMB * HEAT_CAPACITY_VACUUM * space_members as f64;
            let mut heat_cap = HEAT_CAPACITY_VACUUM * space_members as f64;
            for &c in &turfs {
                let mix = &self.cells[c as usize].mix;
                let capacity = mix.heat_capacity();
                energy += mix.temperature * capacity;
                heat_cap += capacity;
                for (s, m) in shared.moles.iter_mut().zip(mix.moles) {
                    *s += m;
                }
            }
            shared.temperature = energy / heat_cap;
            let n = (turfs.len() + space_members) as f64;
            for s in &mut shared.moles {
                *s /= n;
            }
            shared.garbage_collect();
        }
        for &c in &turfs {
            let p = self.cells[c as usize].planetary;
            if p != NO_PLANETARY {
                let atmosphere = self.planetary[usize::from(p)].1.clone();
                self.cells[c as usize].mix.copy_from(&atmosphere);
            } else {
                self.cells[c as usize].mix.copy_from(&shared);
            }
            self.update_visuals(c);
            if poke_turfs {
                self.add_to_active(c, false);
                self.cells[c as usize].ticker = EXCITED_GROUP_DISMANTLE_CYCLES;
            }
        }
        let group = &mut self.groups[g as usize];
        group.turfs = turfs;
        group.breakdown_cooldown = 0;
    }

    /// `process_excited_groups`.
    fn process_excited_groups(&mut self) {
        let mut run = std::mem::take(&mut self.group_scratch);
        run.clear();
        run.extend(
            self.live_groups
                .iter()
                .map(|&g| (g, self.groups[g as usize].generation)),
        );
        for &(g, generation) in run.iter().rev() {
            let group = &mut self.groups[g as usize];
            if group.slot == NONE || group.generation != generation {
                continue;
            }
            let volatile = group.reactions & VOLATILE_REACTION != 0;
            group.breakdown_cooldown += 1;
            if !volatile {
                group.dismantle_cooldown += 1;
            }
            let reacting = group.reactions & (REACTING | STOP_REACTIONS) != 0;
            if group.breakdown_cooldown >= EXCITED_GROUP_BREAKDOWN_CYCLES && !volatile {
                self.self_breakdown(g, None, true);
            } else if group.dismantle_cooldown >= EXCITED_GROUP_DISMANTLE_CYCLES && !reacting {
                self.dismantle(g);
            }
            self.groups[g as usize].reactions = 0;
        }
        self.group_scratch = run;
    }

    /// `assume_air` / `atmos_spawn_air`: merges `giver` into an open tile and wakes it.
    pub fn merge_into(&mut self, tile: u32, giver: &GasMixture) -> bool {
        let c = self.cell_of[tile as usize];
        if c == NONE {
            return false;
        }
        self.cells[c as usize].mix.merge(giver);
        self.archive(c);
        self.update_visuals(c);
        self.add_to_active(c, false);
        true
    }

    /// Changes an open tile's mixture in place (scrubbers taking single gases) and wakes the
    /// tile like `remove_air` does; `None` in walls and space.
    pub fn modify<R>(&mut self, tile: u32, f: impl FnOnce(&mut GasMixture) -> R) -> Option<R> {
        let c = self.cell_of[tile as usize];
        if c == NONE {
            return None;
        }
        let out = f(&mut self.cells[c as usize].mix);
        self.cells[c as usize].mix.garbage_collect();
        self.update_visuals(c);
        self.add_to_active(c, false);
        Some(out)
    }

    /// `remove_ratio`: takes that fraction of an open tile's gas.
    pub fn remove_ratio_from(&mut self, tile: u32, ratio: f64) -> Option<GasMixture> {
        let c = self.cell_of[tile as usize];
        if c == NONE {
            return None;
        }
        let removed = self.cells[c as usize].mix.remove_ratio(ratio);
        self.update_visuals(c);
        self.add_to_active(c, false);
        Some(removed)
    }

    /// `remove_air`: takes `amount` moles from an open tile.
    pub fn remove_from(&mut self, tile: u32, amount: f64) -> Option<GasMixture> {
        let c = self.cell_of[tile as usize];
        if c == NONE {
            return None;
        }
        let removed = self.cells[c as usize].mix.remove(amount);
        self.update_visuals(c);
        self.add_to_active(c, false);
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gas::GasId;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-3
    }

    fn mix(text: &str) -> Option<GasMixture> {
        Some(GasMixture::parse(text).unwrap())
    }

    const AIR: &str = "o2=22;n2=82;TEMP=293.15";

    /// Two open tiles side by side, standard air.
    fn pair() -> LevelAir {
        let mut level = LevelAir::new(2, 1);
        level.set_tile(0, TileKind::Open, mix(AIR), None);
        level.set_tile(1, TileKind::Open, mix(AIR), None);
        assert_eq!(level.setup().differing, 0);
        level
    }

    fn fire(level: &mut LevelAir) -> Vec<PressureDelta> {
        let mut out = Vec::new();
        level.fire(&mut out);
        out
    }

    #[test]
    fn stable_air_costs_nothing() {
        let mut level = pair();
        assert_eq!((level.active_count(), level.group_count()), (0, 0));
        assert!(fire(&mut level).is_empty());
        assert_eq!(level.active_count(), 0);
    }

    /// 20 mol of plasma on one tile: shared half way in one fire, then the group breaks down at
    /// fire 5, the turfs fall asleep at fire 6 (ticker 12 > 11) and the group dismantles at
    /// fire 11.
    #[test]
    fn excited_group_lifecycle() {
        let mut level = pair();
        assert!(level.merge_into(0, &GasMixture::parse("plasma=20;TEMP=293.15").unwrap()));
        assert_eq!(level.active_count(), 1);
        let deltas = fire(&mut level);
        // Coefficient 1/2: 10 mol move. Pressure difference: 293.15 * (72 - 52) * 8.31 / 2500.
        let (a, b) = (level.air(0).unwrap(), level.air(1).unwrap());
        assert!(close(a.get(GasId::PLASMA), 10.0) && close(b.get(GasId::PLASMA), 10.0));
        assert_eq!(deltas.len(), 1);
        assert_eq!((deltas[0].tile, deltas[0].dir), (0, Dir::East));
        assert!(
            close(deltas[0].difference, 19.488_612),
            "{}",
            deltas[0].difference
        );
        assert_eq!((level.active_count(), level.group_count()), (2, 1));
        for _ in 2..=5 {
            fire(&mut level);
        }
        assert_eq!(level.active_count(), 2, "breakdown pokes, not sleep");
        fire(&mut level);
        assert_eq!(
            (level.active_count(), level.group_count()),
            (0, 1),
            "fire 6"
        );
        for _ in 7..=10 {
            fire(&mut level);
        }
        assert_eq!(level.group_count(), 1);
        fire(&mut level);
        assert_eq!(level.group_count(), 0, "fire 11");
        let a = level.air(0).unwrap();
        assert!(close(a.get(GasId::PLASMA), 10.0) && close(a.get(GasId::O2), 22.0));
    }

    /// A closed door (west edge blocked) between space and a room tile opens. Fire 1: the space
    /// turf is idle, so the tile's own pass shares with it (coefficients 1/(1+1): 11 O2 and
    /// 41 N2 leave, the tile keeps 293.15 K because its capacity halves with its moles), the
    /// space turf joins the group, and the `run_later` full share empties the tile. Fire 2: the
    /// space turf processes first and sets the tile to 2.7 K.
    #[test]
    fn opening_to_space_vents_the_tile() {
        let mut level = LevelAir::new(2, 1);
        level.set_tile(1, TileKind::Open, mix(AIR), None);
        level.set_edge_block(1, Dir::West as u8);
        assert_eq!(level.setup().differing, 0);
        level.set_edge_block(1, 0);
        level.recalculate_adjacency(1);
        level.activate(1, false);
        let deltas = fire(&mut level);
        let tile = level.air(1).unwrap();
        assert_eq!(tile.total_moles(), 0.0);
        assert!(close(tile.temperature, 293.15));
        // (293.15 * (52 + 52) - 2.7 * (52 - 52)) * 8.31 / 2500 = 101.34 towards space.
        assert_eq!(deltas.len(), 1);
        assert_eq!((deltas[0].tile, deltas[0].dir), (1, Dir::West));
        assert!(close(deltas[0].difference, 101.340_782));
        assert_eq!(level.group_count(), 1, "the space turf joined");
        fire(&mut level);
        assert_eq!(level.air(1).unwrap().temperature, 2.7);
    }

    /// A wall built on one tile of a pair, then dismantled again: the wall has no air and no
    /// adjacency, the new floor gets its own air and shares with its neighbour.
    #[test]
    fn changed_turfs_drop_and_get_air() {
        let mut level = pair();
        level.change_tile(1, TileKind::Blocked, None);
        assert!(level.air(1).is_none());
        assert!(!level.adjacent(0, Dir::East) && !level.adjacent(1, Dir::West));
        let mut visuals = Vec::new();
        level.drain_visuals(|tile, steps| visuals.push((tile, *steps)));
        assert_eq!(
            visuals,
            [(1, [0; GAS_COUNT])],
            "the old cell's overlay clears"
        );
        level.change_tile(1, TileKind::Open, Some(GasMixture::vacuum()));
        assert_eq!(level.air(1).unwrap().total_moles(), 0.0);
        assert!(level.adjacent(0, Dir::East) && level.adjacent(1, Dir::West));
        assert!(level.active_count() > 0, "the new floor wakes up");
        fire(&mut level);
        let (a, b) = (level.air(0).unwrap(), level.air(1).unwrap());
        assert!(close(a.total_moles(), 52.0) && close(b.total_moles(), 52.0));
    }

    /// A space member counts in the breakdown average as an empty 7000 J/K mixture at 2.7 K:
    /// 3 cells + 1 space turf share 3 * 104 mol, so each cell gets 78.
    #[test]
    fn space_in_a_group_drains_its_breakdown() {
        let mut level = LevelAir::new(4, 1);
        for t in 1..4 {
            level.set_tile(t, TileKind::Open, mix(AIR), None);
        }
        level.set_edge_block(1, Dir::West as u8);
        level.setup();
        let g = level.new_group();
        for c in 0..3 {
            level.add_turf(g, c);
        }
        level.add_space(g, 0);
        level.self_breakdown(g, None, false);
        let air = level.air(2).unwrap();
        assert!(close(air.total_moles(), 78.0));
        // (3 * 293.15 * 2080 + 2.7 * 7000) / (3 * 2080 + 7000) = 139.6
        assert!(close(
            air.temperature,
            (3.0 * 293.15 * 2080.0 + 2.7 * 7000.0) / 13240.0
        ));
    }

    #[test]
    fn roundstart_breach_vents_the_connected_room() {
        let mut level = LevelAir::new(3, 1);
        level.set_tile(1, TileKind::Open, mix(AIR), None);
        level.set_tile(2, TileKind::Open, mix(AIR), None);
        let report = level.setup();
        assert_eq!(
            (report.differing, report.connected, report.vented_groups),
            (1, 1, 1)
        );
        assert_eq!(level.air(2).unwrap().total_moles(), 0.0);
        assert_eq!((level.active_count(), level.group_count()), (0, 0));
    }

    #[test]
    fn walls_and_edges_block() {
        let mut level = LevelAir::new(3, 1);
        level.set_tile(0, TileKind::Open, mix(AIR), None);
        level.set_tile(1, TileKind::Blocked, None, None);
        level.set_tile(2, TileKind::Open, mix("TEMP=293.15"), None);
        assert_eq!(level.setup().differing, 0);
        assert!(level.air(1).is_none());
        assert!(!level.adjacent(0, Dir::East));
        // A windoor facing east on tile 0 blocks its east edge only.
        let mut level = pair();
        level.set_edge_block(0, Dir::East as u8);
        level.recalculate_adjacency(0);
        assert!(!level.adjacent(0, Dir::East) && !level.adjacent(1, Dir::West));
        level.set_edge_block(0, Dir::North as u8);
        level.recalculate_adjacency(0);
        assert!(level.adjacent(1, Dir::West));
    }

    #[test]
    fn planetary_relaxes_to_its_atmosphere() {
        let mut level = LevelAir::new(1, 1);
        let atmosphere = GasMixture::parse(AIR).unwrap();
        level.set_tile(0, TileKind::Open, mix(AIR), Some((AIR, &atmosphere)));
        level.setup();
        level.remove_from(0, 50.0);
        for _ in 0..10 {
            fire(&mut level);
        }
        assert!(close(level.air(0).unwrap().total_moles(), 104.0));
    }
}
