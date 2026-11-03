//! Pipe networks after tg `/datum/pipeline` (`code/modules/atmospherics/machinery/datum_pipeline.dm`)
//! and the node logic of `/obj/machinery/atmospherics` (`atmosmachinery.dm`,
//! `components/components_base.dm`, `pipes/*.dm`).
//!
//! Every pipe, manifold, adapter and machine is a [`Device`] in the [`Pipenet`] resource. Devices
//! connect through *nodes*: tg `atmos_init` looks one tile away in each of a device's node
//! directions for a device on a compatible layer and color that faces back
//! ([`Pipenet::connection_check`]). Connected pipes form a [`Pipeline`] with one gas mixture of
//! the summed pipe volume; a machine keeps one mixture per port ([`Device::airs`]), and the
//! pipeline on that port reconciles them all by volume ([`Pipenet::process`], tg
//! `reconcile_air`) whenever a machine moved gas ([`Pipenet::update_parents`]).
//!
//! The game owns the entities and the machine logic; this module owns the graph and the gas.
//! API for machines elsewhere (an engine cooling loop, a supermatter chamber):
//! - build: [`Pipenet::add`] with a [`DeviceSpec`] per map object, then [`Pipenet::build`] once
//!   (tg `setup_atmos_machinery` + `setup_pipenets`). After that `add`/[`Pipenet::remove`]
//!   relink and rebuild the touched pipelines at once, keeping their gas.
//! - per SSair fire: [`Pipenet::process`] (tg `process_pipenets`), then each machine reads and
//!   writes [`Pipenet::air_mut`] of its ports and calls [`Pipenet::update_parents`] when it moved
//!   gas, exactly as tg `process_atmos` does.
//! - [`Pipenet::pipeline_air`] is the whole pipeline's mixture (pumps' `output_pipenet_air`).
//!
//! Cost: building is O(devices); a fire costs O(pipelines marked updated × their member
//! airs), nothing for a pipeline nobody touched (tg stops reconciling it after a quiet fire).

pub mod air;

use std::collections::HashMap;

use bevy::prelude::*;
use ss13_shared::{Dir, TilePos};

pub use air::{Mix, PipeAir, TurfMix};

use crate::gas::GasMixture;
use crate::reactions::{Exposures, Holder, NO_REACTION};

/// `UNARY_PIPE_VOLUME`: litres per pipe direction.
pub const UNARY_PIPE_VOLUME: f64 = 35.0;
/// Litres of a machine port's mixture (`components_base.dm`).
pub const PORT_VOLUME: f64 = 200.0;
pub const PIPING_LAYER_MIN: u8 = 1;
pub const PIPING_LAYER_MAX: u8 = 5;
pub const PIPING_LAYER_DEFAULT: u8 = 3;

/// `pipe_flags` bits (`code/__DEFINES/atmospherics/atmos_piping.dm`).
pub mod flags {
    pub const ALL_LAYER: u16 = 1 << 0;
    pub const ONE_PER_TURF: u16 = 1 << 1;
    pub const DEFAULT_LAYER_ONLY: u16 = 1 << 2;
    pub const CARDINAL_AUTONORMALIZE: u16 = 1 << 3;
    pub const ALL_COLORS: u16 = 1 << 4;
    pub const BRIDGE: u16 = 1 << 5;
    pub const DISTRO_AND_WASTE_LAYERS: u16 = 1 << 6;
    pub const DONT_SHARE_COLOR: u16 = 1 << 7;
}

/// `ATMOS_COLOR_OMNI` (`COLOR_VERY_LIGHT_GRAY`, `#EEEEEE`): connects to every color.
pub const COLOR_OMNI: [u8; 3] = [0xEE, 0xEE, 0xEE];

/// The cardinal bits in tg's `GLOB.cardinals` order (N, S, E, W).
pub const CARDINALS: [u8; 4] = [1, 2, 4, 8];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeviceId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PipelineId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DeviceKind {
    /// `/obj/machinery/atmospherics/pipe`: its gas is its pipeline's. `smart` pipes
    /// (`pipe/smart`) size themselves by connections (35 L each, at least 70).
    Pipe {
        volume: f64,
        smart: bool,
        he: HeatExchange,
    },
    /// `pipe/layer_manifold`: connects front and back on every layer; 200 L.
    LayerManifold,
    /// `/obj/machinery/atmospherics/components`: one mixture per port.
    Component,
}

/// `pipe/heat_exchanging`: which neighbours a pipe connects to (tg `is_connectable` with
/// `HE_type_check`). Heat exchanging pipes connect only to each other; a junction takes plain
/// pipes on the side opposite its `dir` and heat exchanging ones on the other.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HeatExchange {
    #[default]
    None,
    Pipe,
    /// The BYOND dir bit of the side that connects to plain pipes (`REVERSE_DIR(dir)`).
    Junction {
        plain_side: u8,
    },
}

/// tg `custom_reconcilation`: machines that add mixtures to their pipelines' reconcile.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Custom {
    #[default]
    None,
    /// `binary/valve`: open, it joins both its pipelines.
    Valve { open: bool },
    /// `portables_connector`: the connected portable machine's mixture while attached.
    Connector { attached: Option<Box<PipeAir>> },
    /// `unary/airlock_pump`: two nodes toward its `dir`, distro on layer 4 and waste on 2.
    AirlockPump,
    /// `components/tank`: a member of a merge group of adjacent tanks sharing one mixture
    /// (tg `/datum/merger` `stationary_tanks`, [`TankGroup`]).
    Tank { group: u32 },
}

/// Stationary tanks touching each other, with their shared `air_contents`.
#[derive(Clone, Debug, PartialEq)]
pub struct TankGroup {
    pub air: PipeAir,
    pub members: Vec<DeviceId>,
}

/// What the game knows about one atmos object when it adds it.
#[derive(Clone, Debug)]
pub struct DeviceSpec {
    pub entity: Entity,
    pub pos: TilePos,
    /// `initialize_directions` after `set_init_directions` (BYOND dir bits).
    pub init_dirs: u8,
    /// Node directions in node order (`get_node_connects`); see [`default_node_dirs`].
    /// Ignored for [`DeviceKind::LayerManifold`].
    pub node_dirs: Vec<u8>,
    pub layer: u8,
    /// `pipe_color` as RGB.
    pub color: [u8; 3],
    /// `pipe_flags`.
    pub flags: u16,
    pub kind: DeviceKind,
    /// Components: litres of each port's mixture, one per node.
    pub port_volumes: Vec<f64>,
    pub custom: Custom,
}

/// tg `get_node_connects`: node `i` takes the first cardinal of `init_dirs` (N, S, E, W order)
/// no earlier node took.
pub fn default_node_dirs(count: usize, init_dirs: u8) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(count);
    for _ in 0..count {
        if let Some(&d) = CARDINALS
            .iter()
            .find(|&&d| init_dirs & d != 0 && !out.contains(&d))
        {
            out.push(d);
        } else {
            out.push(0);
        }
    }
    out
}

/// BYOND `get_dir` between orthogonally adjacent tiles; 0 otherwise.
fn cardinal_dir(from: TilePos, to: TilePos) -> u8 {
    if from.z != to.z {
        return 0;
    }
    match (to.x - from.x, to.y - from.y) {
        (0, 1) => 1,
        (0, -1) => 2,
        (1, 0) => 4,
        (-1, 0) => 8,
        _ => 0,
    }
}

fn reverse(d: u8) -> u8 {
    match d {
        1 => 2,
        2 => 1,
        4 => 8,
        8 => 4,
        _ => 0,
    }
}

fn step(pos: TilePos, d: u8) -> Option<TilePos> {
    Dir::from_bits(d).map(|d| pos.step(d))
}

/// One atmos object in the graph.
#[derive(Clone, Debug)]
pub struct Device {
    pub entity: Entity,
    pub pos: TilePos,
    pub init_dirs: u8,
    pub layer: u8,
    pub color: [u8; 3],
    pub flags: u16,
    pub kind: DeviceKind,
    node_dirs: Vec<u8>,
    /// tg `nodes`: the connected device per node (layer manifolds: every connection).
    pub nodes: Vec<Option<DeviceId>>,
    /// Pipes: litres it adds to its pipeline.
    pub volume: f64,
    /// Pipes: tg `parent`.
    parent: Option<PipelineId>,
    /// Pipes: tg `air_temporary`, its share while it has no pipeline.
    air_temporary: Option<PipeAir>,
    /// Components: tg `airs`, one per node.
    pub airs: Vec<PipeAir>,
    /// Components: tg `parents`, one per node.
    parents: Vec<Option<PipelineId>>,
    pub custom: Custom,
}

impl Device {
    pub fn is_pipe(&self) -> bool {
        !matches!(self.kind, DeviceKind::Component)
    }

    /// BYOND dir bits of the connected nodes (tg `get_dir(src, node)` over `nodes`).
    pub fn connections(&self, net: &Pipenet) -> u8 {
        self.nodes
            .iter()
            .flatten()
            .filter_map(|&n| net.device(n))
            .fold(0, |bits, n| bits | cardinal_dir(self.pos, n.pos))
    }
}

/// tg `/datum/pipeline`.
#[derive(Clone, Debug)]
pub struct Pipeline {
    pub air: PipeAir,
    pub members: Vec<DeviceId>,
    /// Machine ports in this pipeline (tg `other_atmos_machines` + `other_airs`).
    pub others: Vec<(DeviceId, u8)>,
    /// tg `update`: reconcile on the next fire.
    pub update: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum AirRef {
    Line(PipelineId),
    Port(DeviceId, u8),
    Extra(DeviceId),
    Tank(u32),
}

/// Every pipe network. Empty until the game adds devices.
#[derive(Resource, Default)]
pub struct Pipenet {
    devices: Vec<Option<Device>>,
    free_devices: Vec<u32>,
    lines: Vec<Option<Pipeline>>,
    free_lines: Vec<u32>,
    by_tile: HashMap<TilePos, Vec<DeviceId>>,
    built: bool,
    /// tg `rebuild_queue`.
    dirty: Vec<DeviceId>,
    /// Devices whose nodes changed since the game last drained them (icons).
    changed: Vec<DeviceId>,
    /// CPU time of the last [`Self::process`] in the SSair fire.
    pub last_process: std::time::Duration,
    tank_groups: Vec<Option<TankGroup>>,
}

impl Pipenet {
    pub fn device(&self, id: DeviceId) -> Option<&Device> {
        self.devices.get(id.0 as usize).and_then(Option::as_ref)
    }

    fn dev(&self, id: DeviceId) -> &Device {
        self.device(id).expect("live device")
    }

    fn dev_mut(&mut self, id: DeviceId) -> &mut Device {
        self.devices[id.0 as usize].as_mut().expect("live device")
    }

    pub fn device_count(&self) -> usize {
        self.devices.len() - self.free_devices.len()
    }

    pub fn pipelines(&self) -> impl Iterator<Item = (PipelineId, &Pipeline)> {
        self.lines
            .iter()
            .enumerate()
            .filter_map(|(i, l)| l.as_ref().map(|l| (PipelineId(i as u32), l)))
    }

    pub fn pipeline(&self, id: PipelineId) -> Option<&Pipeline> {
        self.lines.get(id.0 as usize).and_then(Option::as_ref)
    }

    pub fn devices_on(&self, pos: TilePos) -> &[DeviceId] {
        self.by_tile.get(&pos).map_or(&[], Vec::as_slice)
    }

    pub fn is_built(&self) -> bool {
        self.built
    }

    /// A pipe's pipeline, or a component port's.
    pub fn pipeline_of(&self, id: DeviceId, port: usize) -> Option<PipelineId> {
        let d = self.device(id)?;
        if d.is_pipe() {
            d.parent
        } else {
            d.parents.get(port).copied().flatten()
        }
    }

    /// A pipeline's mixture to change in place (a pipe's `parent.air`, admin and test setups);
    /// it reconciles on the next fire.
    pub fn pipeline_air_mut(&mut self, line: PipelineId) -> Option<&mut PipeAir> {
        let pl = self.lines.get_mut(line.0 as usize)?.as_mut()?;
        pl.update = true;
        Some(&mut pl.air)
    }

    /// The mixture of a component port (tg `airs[port + 1]`).
    pub fn air(&self, id: DeviceId, port: usize) -> Option<&PipeAir> {
        self.device(id)?.airs.get(port)
    }

    pub fn air_mut(&mut self, id: DeviceId, port: usize) -> Option<&mut PipeAir> {
        self.devices
            .get_mut(id.0 as usize)?
            .as_mut()?
            .airs
            .get_mut(port)
    }

    /// Two ports of one machine at once (pumps, filters).
    pub fn airs_mut(&mut self, id: DeviceId) -> Option<&mut [PipeAir]> {
        Some(
            self.devices
                .get_mut(id.0 as usize)?
                .as_mut()?
                .airs
                .as_mut_slice(),
        )
    }

    /// The whole pipeline's mixture on a pipe or a component port (tg `parents[port].air`,
    /// a pipe's `return_air()`); a pipe without a pipeline has its `air_temporary`.
    pub fn pipeline_air(&self, id: DeviceId, port: usize) -> Option<&PipeAir> {
        let d = self.device(id)?;
        match self.pipeline_of(id, port) {
            Some(line) => self.pipeline(line).map(|l| &l.air),
            None => d.air_temporary.as_ref(),
        }
    }

    /// tg `/datum/pipeline/proc/temperature_interact(target, share_volume, thermal_conductivity)`
    /// of a pipe's pipeline with a turf of `turf_temperature` and `turf_heat_capacity`: the
    /// pipeline's air takes conducted heat as if `share_volume` of it touched the turf. Returns
    /// the turf's temperature change (tg `TakeTemperature`), `None` without a pipeline or heat
    /// capacity. The pipeline reconciles on the next fire.
    pub fn temperature_interact(
        &mut self,
        id: DeviceId,
        share_volume: f64,
        thermal_conductivity: f64,
        turf_temperature: f64,
        turf_heat_capacity: f64,
    ) -> Option<f64> {
        let line = self.device(id)?.parent?;
        let pl = self.lines.get_mut(line.0 as usize)?.as_mut()?;
        let total_heat_capacity = pl.air.heat_capacity();
        let partial_heat_capacity = total_heat_capacity * (share_volume / pl.air.volume);
        if turf_heat_capacity <= 0.0 || partial_heat_capacity <= 0.0 {
            return None;
        }
        let delta = turf_temperature - pl.air.gas.temperature;
        let heat = thermal_conductivity
            * delta
            * (partial_heat_capacity
                * (turf_heat_capacity / (partial_heat_capacity + turf_heat_capacity)));
        pl.air.gas.temperature += heat / total_heat_capacity;
        pl.update = true;
        Some(-heat / turf_heat_capacity)
    }

    /// tg `update_parents()`: the machine's pipelines reconcile on the next fire.
    pub fn update_parents(&mut self, id: DeviceId) {
        let Some(d) = self.device(id) else { return };
        let parents = d.parents.clone();
        let mut missing = false;
        for p in parents {
            match p.and_then(|p| self.lines.get_mut(p.0 as usize)?.as_mut()) {
                Some(line) => line.update = true,
                None => missing = true,
            }
        }
        if missing {
            self.dirty.push(id);
        }
    }

    /// Marks the pipelines a device has for reconciliation, without queueing a rebuild for
    /// unconnected nodes (a tank's closed ports).
    pub fn mark_parents(&mut self, id: DeviceId) {
        let Some(d) = self.device(id) else { return };
        for p in d.parents.clone().into_iter().flatten() {
            if let Some(Some(line)) = self.lines.get_mut(p.0 as usize) {
                line.update = true;
            }
        }
    }

    /// Devices whose nodes changed since the last call (icons to redo).
    pub fn take_changed(&mut self) -> Vec<DeviceId> {
        let mut out = std::mem::take(&mut self.changed);
        out.sort_unstable();
        out.dedup();
        out.retain(|&d| self.device(d).is_some());
        out
    }

    /// Adds an atmos object. Before [`Self::build`] it only registers; after, it connects at
    /// once (tg `on_construction`) and the touched pipelines are rebuilt with their gas.
    pub fn add(&mut self, spec: DeviceSpec) -> DeviceId {
        let ports = match spec.kind {
            DeviceKind::Component => spec.node_dirs.len(),
            _ => 0,
        };
        let node_count = match spec.kind {
            DeviceKind::LayerManifold => 0,
            _ => spec.node_dirs.len(),
        };
        let volume = match spec.kind {
            DeviceKind::Pipe { volume, smart, .. } => {
                if smart {
                    // `set_init_directions`: 35 L per allowed direction, at least 70.
                    (UNARY_PIPE_VOLUME * f64::from(spec.init_dirs.count_ones())).max(70.0)
                } else if volume > 0.0 {
                    volume
                } else {
                    UNARY_PIPE_VOLUME * node_count as f64
                }
            }
            DeviceKind::LayerManifold => 200.0,
            DeviceKind::Component => 0.0,
        };
        let device = Device {
            entity: spec.entity,
            pos: spec.pos,
            init_dirs: spec.init_dirs,
            layer: spec.layer,
            color: spec.color,
            flags: spec.flags,
            kind: spec.kind,
            nodes: vec![None; node_count],
            node_dirs: spec.node_dirs,
            volume,
            parent: None,
            air_temporary: None,
            airs: (0..ports)
                .map(|i| PipeAir::new(spec.port_volumes.get(i).copied().unwrap_or(PORT_VOLUME)))
                .collect(),
            parents: vec![None; ports],
            custom: spec.custom,
        };
        let id = match self.free_devices.pop() {
            Some(i) => {
                self.devices[i as usize] = Some(device);
                DeviceId(i)
            }
            None => {
                self.devices.push(Some(device));
                DeviceId(self.devices.len() as u32 - 1)
            }
        };
        self.by_tile.entry(spec.pos).or_default().push(id);
        if self.built {
            self.atmos_init(id);
            let nodes: Vec<DeviceId> = self.dev(id).nodes.iter().flatten().copied().collect();
            for n in nodes {
                self.atmos_init(n);
                self.dissolve_around(n);
                self.dirty.push(n);
            }
            self.dirty.push(id);
            self.rebuild();
        }
        id
    }

    /// Removes a device (tg `deconstruct`): its neighbours disconnect, the pipelines it was in
    /// are rebuilt without it. Returns the gas that escapes onto its tile: a pipe's share of its
    /// pipeline, a machine's port mixtures that had nowhere to go, a connector's attached
    /// mixture (detach portables first to keep it).
    pub fn remove(&mut self, id: DeviceId) -> Option<GasMixture> {
        let d = self.device(id)?.clone();
        let mut release = PipeAir::new(1.0);
        if d.is_pipe() {
            if let Some(line) = d.parent {
                self.dissolve(line);
            }
            if let Some(air) = self.dev_mut(id).air_temporary.take() {
                release.merge(&air.gas);
            }
        } else {
            // `relocate_airs`.
            for (i, air) in d.airs.iter().enumerate() {
                match (d.nodes.get(i).copied().flatten(), d.parents[i]) {
                    (Some(_), Some(line)) => {
                        if let Some(l) = self.lines[line.0 as usize].as_mut() {
                            l.air.merge(&air.gas);
                        }
                    }
                    _ => release.merge(&air.gas),
                }
            }
            if let Custom::Connector {
                attached: Some(air),
            } = &d.custom
            {
                release.merge(&air.gas);
            }
            for line in d.parents.iter().flatten() {
                self.dissolve(*line);
            }
        }
        for n in d.nodes.iter().flatten() {
            if let Some(Some(nd)) = self.devices.get_mut(n.0 as usize) {
                for slot in nd.nodes.iter_mut().filter(|s| **s == Some(id)) {
                    *slot = None;
                }
                self.changed.push(*n);
                self.dirty.push(*n);
            }
        }
        if let Some(list) = self.by_tile.get_mut(&d.pos) {
            list.retain(|&x| x != id);
        }
        self.devices[id.0 as usize] = None;
        self.free_devices.push(id.0);
        self.dirty.retain(|&x| x != id);
        if self.built {
            self.rebuild();
        }
        (release.total_moles() > 0.0).then_some(release.gas)
    }

    /// tg `setup_atmos_machinery` + `setup_pipenets`: connects every device and builds every
    /// pipeline. Pipes start empty, as in tg.
    pub fn build(&mut self) {
        let ids: Vec<DeviceId> = (0..self.devices.len() as u32)
            .map(DeviceId)
            .filter(|&id| self.device(id).is_some())
            .collect();
        for &id in &ids {
            self.atmos_init(id);
        }
        for &id in &ids {
            self.refresh_volume(id);
        }
        for &id in &ids {
            self.build_targets(id);
        }
        self.changed.extend(ids);
        self.built = true;
    }

    /// `can_be_node` / `connection_check(target, given_layer)`.
    pub fn connection_check(&self, a: DeviceId, b: DeviceId, given_layer: u8) -> bool {
        let (da, db) = (self.dev(a), self.dev(b));
        let dir = cardinal_dir(da.pos, db.pos);
        if dir == 0 || da.init_dirs & dir == 0 || db.init_dirs & reverse(dir) == 0 {
            return false;
        }
        is_connectable(da, db, given_layer) && is_connectable(db, da, given_layer)
    }

    /// tg `atmos_init`: (re)finds the device's nodes.
    fn atmos_init(&mut self, id: DeviceId) {
        let d = self.dev(id);
        let (pos, layer, dir_front) = (d.pos, d.layer, d.init_dirs);
        if matches!(d.kind, DeviceKind::LayerManifold) {
            // `find_all_connections`: front and back on every layer.
            let front = CARDINALS
                .iter()
                .copied()
                .find(|&c| dir_front & c != 0)
                .unwrap_or(0);
            let mut nodes = Vec::new();
            for l in PIPING_LAYER_MIN..=PIPING_LAYER_MAX {
                for d in [front, reverse(front)] {
                    if let Some(found) = self.find_connecting(id, d, l) {
                        nodes.push(Some(found));
                    }
                }
            }
            self.dev_mut(id).nodes = nodes;
            self.changed.push(id);
            return;
        }
        let dirs = d.node_dirs.clone();
        let airlock_pump = matches!(d.custom, Custom::AirlockPump);
        for (i, dir) in dirs.into_iter().enumerate() {
            let Some(target_pos) = step(pos, dir) else {
                continue;
            };
            // `airlock_pump/atmos_init`: node 1 on layer 4, node 2 on layer 2.
            let layer = if airlock_pump {
                if i == 0 { 4 } else { 2 }
            } else {
                layer
            };
            let found = self
                .devices_on(target_pos)
                .iter()
                .copied()
                .find(|&t| t != id && self.connection_check(id, t, layer));
            if let Some(found) = found {
                self.dev_mut(id).nodes[i] = Some(found);
            }
        }
        self.changed.push(id);
    }

    /// `find_connecting(direction, prompted_layer)`.
    fn find_connecting(&self, id: DeviceId, dir: u8, layer: u8) -> Option<DeviceId> {
        let pos = step(self.dev(id).pos, dir)?;
        self.devices_on(pos).iter().copied().find(|&t| {
            self.dev(t).init_dirs & reverse(dir) != 0 && self.connection_check(id, t, layer)
        })
    }

    /// Smart pipes: `update_pipe_icon` sizes them by their connections.
    fn refresh_volume(&mut self, id: DeviceId) {
        let d = self.dev(id);
        if let DeviceKind::Pipe { smart: true, .. } = d.kind {
            let connected = d.nodes.iter().flatten().count() as f64;
            self.dev_mut(id).volume = (connected * UNARY_PIPE_VOLUME).max(UNARY_PIPE_VOLUME * 2.0);
        }
    }

    /// Dissolves every pipeline the device is in (its gas goes to the member pipes' temporary
    /// mixtures) so the next [`Self::rebuild`] can merge across the changed connection.
    fn dissolve_around(&mut self, id: DeviceId) {
        let d = self.dev(id);
        let lines: Vec<PipelineId> = if d.is_pipe() {
            d.parent.into_iter().collect()
        } else {
            d.parents.iter().flatten().copied().collect()
        };
        for line in lines {
            self.dissolve(line);
        }
    }

    /// tg `qdel(pipeline)`: `temporarily_store_air` into its pipes by volume; its machines keep
    /// their mixtures and lose the pipeline. Everything it held is queued for rebuild.
    fn dissolve(&mut self, line: PipelineId) {
        let Some(pl) = self.lines[line.0 as usize].take() else {
            return;
        };
        self.free_lines.push(line.0);
        for &m in &pl.members {
            let Some(Some(d)) = self.devices.get_mut(m.0 as usize) else {
                continue;
            };
            if d.parent == Some(line) {
                d.parent = None;
                if pl.air.volume > 0.0 {
                    let mut share = PipeAir::new(d.volume);
                    share.copy_from_ratio(&pl.air.gas, d.volume / pl.air.volume);
                    d.air_temporary = Some(share);
                }
            }
            self.dirty.push(m);
        }
        for &(m, port) in &pl.others {
            let Some(Some(d)) = self.devices.get_mut(m.0 as usize) else {
                continue;
            };
            if d.parents.get(port as usize) == Some(&Some(line)) {
                d.parents[port as usize] = None;
            }
            self.dirty.push(m);
        }
    }

    /// tg `process_rebuilds`: every queued device without a pipeline gets one.
    fn rebuild(&mut self) {
        let mut queue = std::mem::take(&mut self.dirty);
        queue.sort_unstable();
        queue.dedup();
        queue.retain(|&d| self.device(d).is_some());
        for &id in &queue {
            self.refresh_volume(id);
        }
        for id in queue {
            self.build_targets(id);
        }
    }

    /// `get_rebuild_target` until it has none: a pipe without a pipeline, then each component
    /// port without one.
    fn build_targets(&mut self, id: DeviceId) {
        let d = self.dev(id);
        if d.is_pipe() {
            if d.parent.is_none() {
                let line = self.new_line();
                self.build_line(id, line);
            }
            return;
        }
        for port in 0..d.parents.len() {
            if self.dev(id).parents[port].is_none() {
                let line = self.new_line();
                self.dev_mut(id).parents[port] = Some(line);
                self.build_line(id, line);
            }
        }
    }

    fn new_line(&mut self) -> PipelineId {
        let line = Pipeline {
            air: PipeAir::new(0.0),
            members: Vec::new(),
            others: Vec::new(),
            update: true,
        };
        match self.free_lines.pop() {
            Some(i) => {
                self.lines[i as usize] = Some(line);
                PipelineId(i)
            }
            None => {
                self.lines.push(Some(line));
                PipelineId(self.lines.len() as u32 - 1)
            }
        }
    }

    fn line_mut(&mut self, line: PipelineId) -> &mut Pipeline {
        self.lines[line.0 as usize].as_mut().expect("live pipeline")
    }

    /// `add_machinery_member`: the component's ports on this pipeline join its mixtures.
    fn add_machinery_member(&mut self, line: PipelineId, comp: DeviceId) {
        let ports: Vec<u8> = self
            .dev(comp)
            .parents
            .iter()
            .enumerate()
            .filter(|(_, p)| **p == Some(line))
            .map(|(i, _)| i as u8)
            .collect();
        let pl = self.line_mut(line);
        for port in ports {
            if !pl.others.contains(&(comp, port)) {
                pl.others.push((comp, port));
            }
        }
    }

    /// `build_pipeline_blocking(base)`.
    fn build_line(&mut self, base: DeviceId, line: PipelineId) {
        let mut volume = 0.0;
        if self.dev(base).is_pipe() {
            let d = self.dev_mut(base);
            d.parent = Some(line);
            volume = d.volume;
            let temp = d.air_temporary.take();
            let pl = self.line_mut(line);
            pl.members.push(base);
            if let Some(t) = temp {
                pl.air.gas = t.gas;
            }
        } else {
            self.add_machinery_member(line, base);
        }
        let mut border = vec![base];
        while let Some(b) = border.pop() {
            for found in self.expansion(b, line) {
                if !self.dev(found).is_pipe() {
                    // `set_pipenet(src, borderline)`: the port that faces the borderline.
                    let Some(port) = self.dev(found).nodes.iter().position(|&n| n == Some(b))
                    else {
                        continue;
                    };
                    self.dev_mut(found).parents[port] = Some(line);
                    self.add_machinery_member(line, found);
                    continue;
                }
                if self.line_mut(line).members.contains(&found) {
                    continue;
                }
                if let Some(old) = self.dev(found).parent.filter(|&p| p != line) {
                    // tg logs "added to a pipenet while still having one" and takes it.
                    let v = self.dev(found).volume;
                    if let Some(o) = self.lines[old.0 as usize].as_mut() {
                        o.members.retain(|&m| m != found);
                        o.air.volume -= v;
                    }
                }
                let d = self.dev_mut(found);
                d.parent = Some(line);
                volume += d.volume;
                let temp = d.air_temporary.take();
                let pl = self.line_mut(line);
                pl.members.push(found);
                if let Some(t) = temp {
                    pl.air.merge(&t.gas);
                }
                border.push(found);
            }
        }
        let pl = self.line_mut(line);
        pl.air.volume = volume;
        pl.update = true;
    }

    /// `pipeline_expansion(reference)`.
    fn expansion(&self, id: DeviceId, line: PipelineId) -> Vec<DeviceId> {
        let d = self.dev(id);
        if d.is_pipe() {
            return d.nodes.iter().flatten().copied().collect();
        }
        match d.parents.iter().position(|&p| p == Some(line)) {
            Some(port) => d.nodes.get(port).copied().flatten().into_iter().collect(),
            None => Vec::new(),
        }
    }

    /// tg `process_pipenets`: every pipeline marked updated reconciles its mixtures and reacts;
    /// it stays marked only while it reacts. Devices queued by `update_parents` without a
    /// pipeline get one first (tg `process_rebuilds`).
    pub fn process(&mut self) {
        if !self.dirty.is_empty() {
            self.rebuild();
        }
        for i in (0..self.lines.len()).rev() {
            let line = PipelineId(i as u32);
            if !self.lines[i].as_ref().is_some_and(|l| l.update) {
                continue;
            }
            self.reconcile(line);
            let pl = self.line_mut(line);
            pl.update = if pl.air.volume > 0.0 {
                let mut exposures = Exposures::default();
                pl.air
                    .gas
                    .react(Holder::Other, pl.air.volume, &mut exposures)
                    != NO_REACTION
            } else {
                false
            };
        }
    }

    /// tg `reconcile_air`: every mixture of the pipeline (and of pipelines open valves join,
    /// and of attached portables) gets the volume share of their sum.
    pub fn reconcile(&mut self, start: PipelineId) {
        let mut lines = vec![start];
        let mut airs: Vec<AirRef> = Vec::new();
        let mut i = 0;
        while i < lines.len() {
            let l = lines[i];
            i += 1;
            let Some(pl) = self.pipeline(l) else { continue };
            airs.extend(pl.others.iter().map(|&(d, p)| AirRef::Port(d, p)));
            airs.push(AirRef::Line(l));
            for &(d, _) in &pl.others {
                match &self.dev(d).custom {
                    Custom::Valve { open: true } => {
                        for p in self.dev(d).parents.iter().flatten() {
                            if !lines.contains(p) {
                                lines.push(*p);
                            }
                        }
                    }
                    Custom::Connector { attached: Some(_) } => airs.push(AirRef::Extra(d)),
                    // `return_pipenets_for_reconcilation`: every member's pipelines too.
                    &Custom::Tank { group } => {
                        airs.push(AirRef::Tank(group));
                        let members = self
                            .tank_group(group)
                            .map_or_else(Vec::new, |g| g.members.clone());
                        for m in members {
                            for p in self.dev(m).parents.iter().flatten() {
                                if !lines.contains(p) {
                                    lines.push(*p);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        airs.sort_unstable();
        airs.dedup();
        let mut total = GasMixture::vacuum();
        let (mut energy, mut capacity, mut volume) = (0.0, 0.0, 0.0);
        for &r in &airs {
            let Some(a) = self.air_ref(r) else { continue };
            volume += a.volume;
            let c = a.heat_capacity();
            for (t, m) in total.moles.iter_mut().zip(a.gas.moles) {
                *t += m;
            }
            capacity += c;
            energy += a.gas.temperature * c;
        }
        if volume == 0.0 {
            return;
        }
        total.temperature = if capacity != 0.0 {
            energy / capacity
        } else {
            0.0
        };
        total.garbage_collect();
        for r in airs {
            if let Some(a) = self.air_ref_mut(r) {
                let share = a.volume / volume;
                a.copy_from_ratio(&total, share);
            }
        }
    }

    fn air_ref(&self, r: AirRef) -> Option<&PipeAir> {
        match r {
            AirRef::Line(l) => self.pipeline(l).map(|l| &l.air),
            AirRef::Port(d, p) => self.device(d)?.airs.get(p as usize),
            AirRef::Extra(d) => match &self.device(d)?.custom {
                Custom::Connector { attached } => attached.as_deref(),
                _ => None,
            },
            AirRef::Tank(g) => self.tank_group(g).map(|g| &g.air),
        }
    }

    fn air_ref_mut(&mut self, r: AirRef) -> Option<&mut PipeAir> {
        match r {
            AirRef::Line(l) => self
                .lines
                .get_mut(l.0 as usize)?
                .as_mut()
                .map(|l| &mut l.air),
            AirRef::Port(d, p) => self
                .devices
                .get_mut(d.0 as usize)?
                .as_mut()?
                .airs
                .get_mut(p as usize),
            AirRef::Extra(d) => match &mut self.devices.get_mut(d.0 as usize)?.as_mut()?.custom {
                Custom::Connector { attached } => attached.as_deref_mut(),
                _ => None,
            },
            AirRef::Tank(g) => self.tank_group_mut(g).map(|g| &mut g.air),
        }
    }

    /// `binary/valve/set_open`: an opening valve reconciles its pipelines at once.
    pub fn set_valve_open(&mut self, id: DeviceId, open: bool) {
        let Some(Some(d)) = self.devices.get_mut(id.0 as usize) else {
            return;
        };
        let Custom::Valve { open: current } = &mut d.custom else {
            return;
        };
        if *current == open {
            return;
        }
        *current = open;
        if open {
            self.update_parents(id);
            if let Some(line) = self.dev(id).parents.first().copied().flatten() {
                self.reconcile(line);
            }
        }
    }

    /// `portable_atmospherics/connect`: the portable's mixture joins the connector's pipeline
    /// (reconciled at once). Fails when something is attached already.
    pub fn attach(&mut self, connector: DeviceId, air: PipeAir) -> Result<(), Box<PipeAir>> {
        let air = Box::new(air);
        let Some(Some(d)) = self.devices.get_mut(connector.0 as usize) else {
            return Err(air);
        };
        let Custom::Connector { attached } = &mut d.custom else {
            return Err(air);
        };
        if attached.is_some() {
            return Err(air);
        }
        *attached = Some(air);
        if let Some(line) = self.dev(connector).parents.first().copied().flatten() {
            self.reconcile(line);
        }
        Ok(())
    }

    /// `disconnect`: the portable takes its mixture back.
    pub fn detach(&mut self, connector: DeviceId) -> Option<PipeAir> {
        match &mut self.devices.get_mut(connector.0 as usize)?.as_mut()?.custom {
            Custom::Connector { attached } => attached.take().map(|a| *a),
            _ => None,
        }
    }

    /// The attached portable's mixture.
    pub fn attached_mut(&mut self, connector: DeviceId) -> Option<&mut PipeAir> {
        match &mut self.devices.get_mut(connector.0 as usize)?.as_mut()?.custom {
            Custom::Connector { attached } => attached.as_deref_mut(),
            _ => None,
        }
    }

    pub fn attached(&self, connector: DeviceId) -> Option<&PipeAir> {
        match &self.device(connector)?.custom {
            Custom::Connector { attached } => attached.as_deref(),
            _ => None,
        }
    }

    /// Makes `members` one tank group with `air` (their merged `air_contents`).
    pub fn add_tank_group(&mut self, air: PipeAir, members: Vec<DeviceId>) -> u32 {
        let group = self.tank_groups.len() as u32;
        for &m in &members {
            if let Some(Some(d)) = self.devices.get_mut(m.0 as usize) {
                d.custom = Custom::Tank { group };
            }
        }
        self.tank_groups.push(Some(TankGroup { air, members }));
        group
    }

    pub fn tank_group(&self, group: u32) -> Option<&TankGroup> {
        self.tank_groups.get(group as usize)?.as_ref()
    }

    pub fn tank_group_mut(&mut self, group: u32) -> Option<&mut TankGroup> {
        self.tank_groups.get_mut(group as usize)?.as_mut()
    }

    /// A new tank `id` joins the groups of the tanks next to it (`GetMergeGroup`,
    /// `merger_refresh_complete`): they become one group holding all their air and `air` (its
    /// own `air_contents`); with no neighbours it is a group of its own. Returns the group;
    /// the groups in `neighbours` other than it are gone.
    pub fn join_tank_groups(&mut self, id: DeviceId, air: PipeAir, neighbours: &[u32]) -> u32 {
        let mut groups: Vec<u32> = Vec::new();
        for &g in neighbours {
            if self.tank_group(g).is_some() && !groups.contains(&g) {
                groups.push(g);
            }
        }
        let Some((&into, rest)) = groups.split_first() else {
            return self.add_tank_group(air, vec![id]);
        };
        let mut joining = vec![(air, vec![id])];
        for &g in rest {
            if let Some(TankGroup { air, members }) = self.tank_groups[g as usize].take() {
                joining.push((air, members));
            }
        }
        for (air, members) in joining {
            for &m in &members {
                if let Some(Some(d)) = self.devices.get_mut(m.0 as usize) {
                    d.custom = Custom::Tank { group: into };
                }
            }
            if let Some(g) = self.tank_group_mut(into) {
                g.air.merge(&air.gas);
                g.air.volume += air.volume;
                g.members.extend(members);
            }
        }
        into
    }

    /// A member of `volume` litres leaves its group (`merger_refresh_complete` for a leaver):
    /// it takes `1 / members` of the shared gas, which is returned, and its volume. The rest
    /// stays with the others.
    pub fn leave_tank_group(&mut self, id: DeviceId, volume: f64) -> Option<GasMixture> {
        let group = self.tank_group_of(id)?;
        let g = self.tank_group_mut(group)?;
        let shares = g.members.len().max(1) as f64;
        let share = g.air.remove_ratio(1.0 / shares);
        g.air.volume = (g.air.volume - volume).max(0.0);
        g.members.retain(|&m| m != id);
        if g.members.is_empty() {
            self.tank_groups[group as usize] = None;
        }
        Some(share)
    }

    /// The tank group a device is a member of.
    pub fn tank_group_of(&self, id: DeviceId) -> Option<u32> {
        match self.device(id)?.custom {
            Custom::Tank { group } => Some(group),
            _ => None,
        }
    }

    /// `reconnect_nodes` of a component whose direction, layer or color changed: it leaves its
    /// pipelines and connects again as `init_dirs`, `node_dirs`, `layer` and `color` say,
    /// keeping its port gas. Returns its new id.
    pub fn reconnect(
        &mut self,
        id: DeviceId,
        init_dirs: u8,
        node_dirs: Vec<u8>,
        layer: u8,
        color: [u8; 3],
    ) -> Option<DeviceId> {
        let d = self.device(id)?.clone();
        // Emptied first: `remove` would hand the port gas to the pipelines it leaves.
        for air in &mut self.dev_mut(id).airs {
            *air = PipeAir::new(air.volume);
        }
        // What it would release is now only a connector's attached mixture, which `custom`
        // carries over.
        self.remove(id);
        let new = self.add(DeviceSpec {
            entity: d.entity,
            pos: d.pos,
            init_dirs,
            node_dirs,
            layer,
            color,
            flags: d.flags,
            kind: d.kind,
            port_volumes: d.airs.iter().map(|a| a.volume).collect(),
            custom: d.custom,
        });
        if let Some(airs) = self.airs_mut(new) {
            for (slot, air) in airs.iter_mut().zip(d.airs) {
                *slot = air;
            }
        }
        self.mark_parents(new);
        Some(new)
    }

    /// `toggle_side_port`: a tank member opens or closes ports (`init_dirs`), keeping its
    /// group and port gas; returns its new id.
    pub fn retarget_tank(&mut self, id: DeviceId, mut spec: DeviceSpec) -> Option<DeviceId> {
        let group = self.tank_group_of(id)?;
        // Port gas with nowhere to go comes back below.
        let released = self.remove(id);
        spec.custom = Custom::Tank { group };
        let new = self.add(spec);
        if let Some(g) = self.tank_group_mut(group) {
            for m in g.members.iter_mut().filter(|m| **m == id) {
                *m = new;
            }
            if let Some(gas) = released {
                g.air.merge(&gas);
            }
        }
        self.mark_parents(new);
        Some(new)
    }

    /// Total moles in every pipeline, port and attached mixture (conservation checks).
    pub fn total_moles(&self) -> f64 {
        let lines: f64 = self.pipelines().map(|(_, l)| l.air.total_moles()).sum();
        let devices: f64 = self
            .devices
            .iter()
            .flatten()
            .map(|d| {
                d.airs.iter().map(PipeAir::total_moles).sum::<f64>()
                    + d.air_temporary.as_ref().map_or(0.0, PipeAir::total_moles)
                    + match &d.custom {
                        Custom::Connector { attached: Some(a) } => a.total_moles(),
                        _ => 0.0,
                    }
            })
            .sum();
        let tanks: f64 = self
            .tank_groups
            .iter()
            .flatten()
            .map(|g| g.air.total_moles())
            .sum();
        lines + devices + tanks
    }
}

/// `is_connectable(target, given_layer)` called on `src`.
fn is_connectable(src: &Device, target: &Device, given_layer: u8) -> bool {
    if target.pos == src.pos {
        return false;
    }
    // `heat_exchanging/is_connectable`: `istype(target, heat_exchanging) == HE_type_check`.
    let wants_he = match he_of(src) {
        HeatExchange::None => None,
        HeatExchange::Pipe => Some(true),
        HeatExchange::Junction { plain_side } => {
            Some(cardinal_dir(src.pos, target.pos) != plain_side)
        }
    };
    if wants_he.is_some_and(|want| want != (he_of(target) != HeatExchange::None)) {
        return false;
    }
    if target.flags & flags::DISTRO_AND_WASTE_LAYERS != 0 {
        if given_layer % 2 == 1 {
            return false;
        }
    } else if target.layer != given_layer && target.flags & flags::ALL_LAYER == 0 {
        return false;
    }
    !(target.color != src.color
        && (target.flags | src.flags) & flags::ALL_COLORS == 0
        && target.color != COLOR_OMNI
        && src.color != COLOR_OMNI)
}

fn he_of(d: &Device) -> HeatExchange {
    match d.kind {
        DeviceKind::Pipe { he, .. } => he,
        _ => HeatExchange::None,
    }
}

#[cfg(test)]
mod tests;
