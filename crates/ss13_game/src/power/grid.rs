//! Powernets: tg `/datum/powernet` and the cable graph (`cable.dm`, `power.dm`, `powernet.dm`).
//!
//! A cable links to every cable on the four neighbouring tiles that shares one of its
//! `cable_layer` bits (tg `connect_cable`), except across a SMES and its terminal. Cables on the
//! same tile never link to each other: a multilayer hub is one cable with several layer bits.
//! A power machine joins the net of the first cable on its own tile that shares its layer
//! (`get_cable_node`), else the net of a terminal on its tile (`connect_to_network`).
//!
//! The graph is pure data (no ECS) so building and splitting are unit-tested directly.
//! Build: union-find over all cables, O(cables). Changes: placing a cable merges the nets it
//! touches into the largest one (O(smaller nets)), cutting one re-labels its old net by BFS
//! (O(net)), the same bounds as tg's `merge_powernets` / `propagate_network`.

use std::collections::{HashMap, HashSet};

use bevy::ecs::entity::EntityHashMap;
use bevy::prelude::*;
use ss13_shared::{Dir, TilePos};

pub type NetId = u32;

/// tg `/datum/powernet`, energies in joules per machine tick.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Powernet {
    /// Cable ids (see [`Grid`]).
    pub(super) cables: Vec<u32>,
    /// Drawn this tick.
    pub load: f64,
    /// Supplied this tick: becomes `avail` on the next reset.
    pub newavail: f64,
    /// Available this tick.
    pub avail: f64,
    /// `avail - load` of the last tick, after SMES restores took their share.
    pub netexcess: f64,
    /// Load applied between ticks.
    pub delayedload: f64,
}

impl Powernet {
    /// tg `surplus()`.
    pub fn surplus(&self) -> f64 {
        (self.avail - self.load).clamp(0.0, self.avail.max(0.0))
    }

    /// First half of tg `reset()`: whether the SMES on this net restore their unused output.
    pub fn begin_reset(&mut self) -> bool {
        self.netexcess = self.avail - self.load;
        self.netexcess > 100.0
    }

    /// Second half of tg `reset()`, after the SMES restores.
    pub fn finish_reset(&mut self) {
        self.load = self.delayedload;
        self.delayedload = 0.0;
        self.avail = self.newavail;
        self.newavail = 0.0;
    }

    pub fn cables(&self) -> usize {
        self.cables.len()
    }
}

/// What a power machine is to the cable graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    /// A SMES: cables under it do not link towards a terminal next to it.
    Smes,
    /// A terminal: cables under it do not link towards a SMES next to it; machines without a
    /// cable on the tile join its net.
    Terminal,
    Machine,
}

#[derive(Clone, Copy, Debug)]
struct CableData {
    pos: TilePos,
    layer: u8,
    net: NetId,
}

#[derive(Clone, Copy, Debug)]
struct NodeData {
    pos: TilePos,
    layer: u8,
    kind: NodeKind,
    net: Option<NetId>,
}

/// Every cable, net and power machine node.
#[derive(Resource, Default, Debug)]
pub struct Grid {
    cables: Vec<Option<CableData>>,
    free_cables: Vec<u32>,
    cable_of: EntityHashMap<u32>,
    by_tile: HashMap<TilePos, Vec<u32>>,
    nets: Vec<Option<Powernet>>,
    free_nets: Vec<NetId>,
    nodes: EntityHashMap<NodeData>,
    nodes_by_tile: HashMap<TilePos, Vec<Entity>>,
    smes_tiles: HashSet<TilePos>,
    terminal_tiles: HashSet<TilePos>,
}

impl Grid {
    /// Registers a power machine. Add every SMES and terminal before the cables: they decide
    /// which cables link.
    // shortcut: a SMES or terminal added after the cables does not unlink cables next to it;
    // fix with construction (tg rebuilds through `connect_cable(TRUE)`).
    pub fn add_node(&mut self, entity: Entity, pos: TilePos, layer: u8, kind: NodeKind) {
        match kind {
            NodeKind::Smes => self.smes_tiles.insert(pos),
            NodeKind::Terminal => self.terminal_tiles.insert(pos),
            NodeKind::Machine => false,
        };
        self.nodes.insert(
            entity,
            NodeData {
                pos,
                layer,
                kind,
                net: None,
            },
        );
        self.nodes_by_tile.entry(pos).or_default().push(entity);
        self.resolve_tile(pos);
    }

    /// tg `disconnect_from_network` of a machine node (an emitter cut loose, a SMES or terminal
    /// taken apart).
    // shortcut: cables a removed SMES or terminal kept apart stay unlinked until re-laid.
    pub fn remove_node(&mut self, entity: Entity) {
        let Some(node) = self.nodes.remove(&entity) else {
            return;
        };
        if let Some(list) = self.nodes_by_tile.get_mut(&node.pos) {
            list.retain(|&e| e != entity);
        }
        let kind_left = |kind: NodeKind| {
            self.nodes_by_tile
                .get(&node.pos)
                .is_some_and(|l| l.iter().any(|e| self.nodes[e].kind == kind))
        };
        match node.kind {
            NodeKind::Smes if !kind_left(NodeKind::Smes) => {
                self.smes_tiles.remove(&node.pos);
            }
            NodeKind::Terminal if !kind_left(NodeKind::Terminal) => {
                self.terminal_tiles.remove(&node.pos);
            }
            _ => {}
        }
    }

    /// All cables at once (map load): union-find, then one net per component.
    pub fn build(&mut self, cables: impl IntoIterator<Item = (Entity, TilePos, u8)>) {
        let first = self.cables.len() as u32;
        for (entity, pos, layer) in cables {
            self.insert(entity, pos, layer, NetId::MAX);
        }
        let n = self.cables.len() as u32;
        let mut parent: Vec<u32> = (0..n).collect();
        fn find(parent: &mut [u32], mut i: u32) -> u32 {
            while parent[i as usize] != i {
                parent[i as usize] = parent[parent[i as usize] as usize];
                i = parent[i as usize];
            }
            i
        }
        for i in first..n {
            // North and east cover every pair once (links are symmetric).
            for j in self.links_towards(i, &[Dir::North, Dir::East]) {
                let (a, b) = (find(&mut parent, i), find(&mut parent, j));
                if a != b {
                    parent[a as usize] = b;
                }
            }
        }
        let mut net_of_root: HashMap<u32, NetId> = HashMap::new();
        for i in first..n {
            let root = find(&mut parent, i);
            let net = *net_of_root.entry(root).or_insert_with(|| {
                let id = self.free_nets.pop().unwrap_or(self.nets.len() as NetId);
                if id as usize == self.nets.len() {
                    self.nets.push(None);
                }
                self.nets[id as usize] = Some(Powernet::default());
                id
            });
            self.set_net(i, net);
        }
        let tiles: Vec<TilePos> = self.nodes_by_tile.keys().copied().collect();
        for pos in tiles {
            self.resolve_tile(pos);
        }
    }

    /// A cable placed at runtime: joins the nets it links to, merged into the largest.
    pub fn add_cable(&mut self, entity: Entity, pos: TilePos, layer: u8) -> NetId {
        let id = self.insert(entity, pos, layer, NetId::MAX);
        let mut touched: Vec<NetId> = self
            .links_towards(id, &Dir::CARDINALS)
            .map(|j| self.cable(j).net)
            .collect();
        touched.sort_unstable();
        touched.dedup();
        let target = touched
            .iter()
            .copied()
            .max_by_key(|&n| self.net(n).map_or(0, Powernet::cables))
            .unwrap_or_else(|| self.new_net());
        for &old in touched.iter().filter(|&&n| n != target) {
            let moved = self.nets[old as usize].take().expect("live net").cables;
            self.free_nets.push(old);
            for c in moved {
                self.set_net(c, target);
            }
            for node in self.nodes.values_mut().filter(|n| n.net == Some(old)) {
                node.net = Some(target);
            }
        }
        self.set_net(id, target);
        self.resolve_tile(pos);
        target
    }

    /// A cable removed at runtime: its net splits into one net per remaining component.
    pub fn remove_cable(&mut self, entity: Entity) {
        let Some(id) = self.cable_of.remove(&entity) else {
            return;
        };
        let data = self.cables[id as usize].take().expect("indexed cable");
        self.free_cables.push(id);
        if let Some(list) = self.by_tile.get_mut(&data.pos) {
            list.retain(|&c| c != id);
        }
        let old = self.nets[data.net as usize].take().expect("live net");
        self.free_nets.push(data.net);
        let mut left: HashSet<u32> = old.cables.into_iter().filter(|&c| c != id).collect();
        let mut new_nets = Vec::new();
        while let Some(&start) = left.iter().next() {
            left.remove(&start);
            let net = self.new_net();
            new_nets.push(net);
            let mut stack = vec![start];
            while let Some(c) = stack.pop() {
                self.set_net(c, net);
                let next: Vec<u32> = self
                    .links_towards(c, &Dir::CARDINALS)
                    .filter(|j| left.remove(j))
                    .collect();
                stack.extend(next);
            }
        }
        let stale: Vec<TilePos> = self
            .nodes
            .values()
            .filter(|n| n.net == Some(data.net) || n.pos == data.pos)
            .map(|n| n.pos)
            .collect();
        for pos in stale {
            self.resolve_tile(pos);
        }
    }

    pub fn node_net(&self, entity: Entity) -> Option<NetId> {
        self.nodes.get(&entity)?.net
    }

    pub fn cable_net(&self, entity: Entity) -> Option<NetId> {
        self.cable_of.get(&entity).map(|&id| self.cable(id).net)
    }

    pub fn net(&self, id: NetId) -> Option<&Powernet> {
        self.nets.get(id as usize)?.as_ref()
    }

    pub fn net_mut(&mut self, id: NetId) -> Option<&mut Powernet> {
        self.nets.get_mut(id as usize)?.as_mut()
    }

    pub fn nets(&self) -> impl Iterator<Item = (NetId, &Powernet)> {
        self.nets
            .iter()
            .enumerate()
            .filter_map(|(i, n)| Some((i as NetId, n.as_ref()?)))
    }

    pub fn nets_mut(&mut self) -> impl Iterator<Item = (NetId, &mut Powernet)> {
        self.nets
            .iter_mut()
            .enumerate()
            .filter_map(|(i, n)| Some((i as NetId, n.as_mut()?)))
    }

    pub fn cable_count(&self) -> usize {
        self.cable_of.len()
    }

    fn cable(&self, id: u32) -> &CableData {
        self.cables[id as usize].as_ref().expect("live cable id")
    }

    fn insert(&mut self, entity: Entity, pos: TilePos, layer: u8, net: NetId) -> u32 {
        let data = CableData { pos, layer, net };
        let id = match self.free_cables.pop() {
            Some(id) => {
                self.cables[id as usize] = Some(data);
                id
            }
            None => {
                self.cables.push(Some(data));
                self.cables.len() as u32 - 1
            }
        };
        self.cable_of.insert(entity, id);
        self.by_tile.entry(pos).or_default().push(id);
        id
    }

    fn new_net(&mut self) -> NetId {
        let id = self.free_nets.pop().unwrap_or(self.nets.len() as NetId);
        if id as usize == self.nets.len() {
            self.nets.push(None);
        }
        self.nets[id as usize] = Some(Powernet::default());
        id
    }

    fn set_net(&mut self, cable: u32, net: NetId) {
        self.cables[cable as usize]
            .as_mut()
            .expect("live cable")
            .net = net;
        self.nets[net as usize]
            .as_mut()
            .expect("live net")
            .cables
            .push(cable);
    }

    /// Cables `id` links to on the neighbours in `dirs`.
    fn links_towards<'a>(&'a self, id: u32, dirs: &'a [Dir]) -> impl Iterator<Item = u32> + 'a {
        let a = *self.cable(id);
        dirs.iter().flat_map(move |&dir| {
            let pos = a.pos.step(dir);
            let blocked = self.split_by_terminal(a.pos, pos);
            self.by_tile
                .get(&pos)
                .into_iter()
                .flatten()
                .copied()
                .filter(move |&j| !blocked && self.cable(j).layer & a.layer != 0)
        })
    }

    /// tg `connect_cable`: no link from under a SMES to a terminal tile and back.
    // shortcut: tg also checks that the terminal belongs to that SMES (`term.master`); at map
    // load masters are not assigned yet either, so tg links the same way there.
    fn split_by_terminal(&self, a: TilePos, b: TilePos) -> bool {
        let smes = |p| self.smes_tiles.contains(p) && !self.terminal_tiles.contains(p);
        let terminal = |p| self.terminal_tiles.contains(p);
        (smes(&a) && terminal(&b)) || (terminal(&a) && smes(&b))
    }

    /// tg `connect_to_network` for every node on `pos`.
    fn resolve_tile(&mut self, pos: TilePos) {
        let Some(nodes) = self.nodes_by_tile.get(&pos) else {
            return;
        };
        let cables = self.by_tile.get(&pos);
        let net_for = |layer: u8| {
            cables?
                .iter()
                .map(|&c| self.cable(c))
                .find(|c| c.layer & layer != 0)
                .map(|c| c.net)
        };
        let mut resolved: Vec<(Entity, Option<NetId>)> = nodes
            .iter()
            .map(|&e| (e, net_for(self.nodes[&e].layer)))
            .collect();
        // Machines without a cable node use a connected terminal on their tile.
        let terminal_net = resolved
            .iter()
            .find(|(e, net)| self.nodes[e].kind == NodeKind::Terminal && net.is_some())
            .and_then(|(_, net)| *net);
        for (e, net) in &mut resolved {
            if net.is_none() && self.nodes[e].kind != NodeKind::Terminal {
                *net = terminal_net;
            }
        }
        for (e, net) in resolved {
            self.nodes.get_mut(&e).expect("indexed node").net = net;
        }
    }

    #[cfg(test)]
    fn components(&self) -> usize {
        self.nets().count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(i: u32) -> Entity {
        Entity::from_raw_u32(i + 1).unwrap()
    }

    fn t(x: i32, y: i32) -> TilePos {
        TilePos::new(x, y, 1)
    }

    /// `rows` top-down: `-` layer 2 cable, `r` layer 1, `b` layer 3, `H` hub on all layers.
    fn grid(rows: &[&str]) -> (Grid, Vec<(Entity, TilePos, u8)>) {
        let mut cables = Vec::new();
        let h = rows.len() as i32;
        for (row, line) in rows.iter().enumerate() {
            for (col, ch) in line.chars().enumerate() {
                let layer = match ch {
                    '-' => 2,
                    'r' => 1,
                    'b' => 4,
                    'H' => 7,
                    _ => continue,
                };
                let i = cables.len() as u32;
                cables.push((e(i), t(col as i32 + 1, h - row as i32), layer));
            }
        }
        (Grid::default(), cables)
    }

    #[test]
    fn layers_hubs_and_nodes() {
        let (mut g, cables) = grid(&[
            "---  rrr", //
            "  -    r", //
            "  Hbbbbb", //
        ]);
        let apc = e(100);
        let lonely = e(101);
        g.add_node(apc, t(1, 3), 2, NodeKind::Machine);
        g.add_node(lonely, t(5, 2), 2, NodeKind::Machine);
        g.build(cables.clone());
        // Yellow line + hub + the blue run (through the hub) is one net; red is another.
        assert_eq!(g.components(), 2);
        let net = |x, y| {
            let (entity, ..) = cables.iter().find(|c| c.1 == t(x, y)).unwrap();
            g.cable_net(*entity).unwrap()
        };
        assert_eq!(net(1, 3), net(8, 1));
        assert_ne!(net(6, 3), net(1, 3));
        // The red run reaches (8,2) but red and blue never link.
        assert_ne!(net(8, 2), net(8, 1));
        assert_eq!(g.node_net(apc), Some(net(1, 3)));
        assert_eq!(g.node_net(lonely), None);
    }

    #[test]
    fn smes_and_terminal_do_not_link() {
        // SMES at (2,1) facing its terminal at (3,1): input and output stay apart.
        let (mut g, cables) = grid(&["----"]);
        let smes = e(100);
        let terminal = e(101);
        g.add_node(smes, t(2, 1), 2, NodeKind::Smes);
        g.add_node(terminal, t(3, 1), 2, NodeKind::Terminal);
        g.build(cables);
        assert_eq!(g.components(), 2);
        assert_ne!(g.node_net(smes), g.node_net(terminal));
        assert!(g.node_net(smes).is_some() && g.node_net(terminal).is_some());
    }

    #[test]
    fn machine_uses_terminal_net_without_cable() {
        let (mut g, cables) = grid(&["-- "]);
        let terminal = e(100);
        let machine = e(101);
        // A layer-1 machine on a layer-2 cable: no cable node, but the terminal connects.
        g.add_node(terminal, t(2, 1), 2, NodeKind::Terminal);
        g.add_node(machine, t(2, 1), 1, NodeKind::Machine);
        g.build(cables);
        assert_eq!(g.node_net(machine), g.node_net(terminal));
        assert!(g.node_net(machine).is_some());
    }

    #[test]
    fn cut_and_place_match_a_rebuild() {
        let rows = ["-----", "  -  ", "  -  "];
        let (mut g, cables) = grid(&rows);
        let node = e(100);
        g.add_node(node, t(5, 3), 2, NodeKind::Machine);
        g.build(cables.clone());
        assert_eq!(g.components(), 1);
        // Cut the junction: west arm, east arm and south arm.
        let (junction, pos, layer) = cables.iter().copied().find(|c| c.1 == t(3, 3)).unwrap();
        let before = g.node_net(node);
        g.remove_cable(junction);
        assert_eq!(g.components(), 3);
        assert_eq!(g.cable_count(), cables.len() - 1);
        assert_ne!(g.node_net(node), None);
        let after_cut = g.node_net(node);
        let (east, ..) = cables.iter().find(|c| c.1 == t(4, 3)).unwrap();
        assert_eq!(after_cut, g.cable_net(*east));
        // Place it back: one net again, the node follows the merge.
        let net = g.add_cable(junction, pos, layer);
        assert_eq!(g.components(), 1);
        assert_eq!(g.node_net(node), Some(net));
        assert!(before.is_some());
        let total: usize = g.nets().map(|(_, n)| n.cables()).sum();
        assert_eq!(total, cables.len());
        // A cable placed alone gets its own net; removing it frees the net.
        let stray = e(200);
        g.add_cable(stray, t(9, 9), 2);
        assert_eq!(g.components(), 2);
        g.remove_cable(stray);
        assert_eq!(g.components(), 1);
    }

    #[test]
    fn reset_and_surplus() {
        let mut n = Powernet {
            avail: 1000.0,
            load: 400.0,
            newavail: 50.0,
            delayedload: 7.0,
            ..default()
        };
        assert_eq!(n.surplus(), 600.0);
        assert!(n.begin_reset());
        assert_eq!(n.netexcess, 600.0);
        n.finish_reset();
        assert_eq!((n.avail, n.load, n.newavail), (50.0, 7.0, 0.0));
        n.load = 80.0;
        assert_eq!(n.surplus(), 0.0);
        assert!(!n.begin_reset());
    }
}
