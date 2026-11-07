//! The research data ([`ResearchDb`], tg `SSresearch.techweb_nodes`, `techweb_designs` and the
//! lookups `initialize_nodes` builds) and the station techweb ([`Techweb`], tg
//! `/datum/techweb/science`): researched, available, visible and hidden nodes, points, tiers, the
//! research queue and the unlock rules of `_techweb.dm`.
//!
//! Experiments ([`super::experiments`]): required experiments gate their nodes, completed discount
//! experiments lower node prices, and a node researched without its discount experiment refunds
//! the discount when the experiment is completed later.
// shortcut: required experiments of kinds the port cannot perform count as done (see
// `super::experiments`); of the six nodes with `required_experiments` only medbay_equip_adv
// (haloperidol scan) is gated, the others (cyber_organs_upgraded, explosives, fishing_equip_adv,
// mech_combat, selection) unlock on points and prerequisites alone.
// shortcut: no node boosts (`boost_techweb_node`, toxins papers) or
// research announcements over the radio (`announce_channels`).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use bevy::prelude::*;
use ss13_mapload::research::{
    ConsoleDef, DesignDef, ExperimentDef, LatheDef, MaterialDef, NodeDef, ResearchData, node_flags,
};

use super::experiments::{Experiment, ExperimentRng};

/// `TECHWEB_POINT_TYPE_GENERIC`.
pub const POINT_GENERIC: &str = "General Research";

/// tg `SSresearch` lookups over [`ResearchData`]. Node and design ids are indices into
/// [`Self::nodes`] and [`Self::designs`].
#[derive(Resource, Debug, Default)]
pub struct ResearchDb {
    pub nodes: Vec<NodeDef>,
    pub designs: Vec<DesignDef>,
    pub materials: Vec<MaterialDef>,
    pub node_index: HashMap<String, usize>,
    pub design_index: HashMap<String, usize>,
    pub material_index: HashMap<String, usize>,
    pub prerequisites: Vec<Vec<usize>>,
    /// `unlocked_nodes`: the nodes that list it as a prerequisite.
    pub unlocks: Vec<Vec<usize>>,
    pub node_designs: Vec<Vec<usize>>,
    /// `design.unlocked_by`.
    pub unlocked_by: Vec<Vec<usize>>,
    /// `techweb_unlock_items`: item type -> nodes its deconstruction unhides.
    pub unlock_items: HashMap<String, Vec<usize>>,
    pub starting: Vec<usize>,
    pub hidden: Vec<usize>,
    pub point_types: Vec<(String, String)>,
    pub server_income: Vec<(String, f64)>,
    pub point_items: HashMap<String, Vec<(String, f64)>>,
    pub techweb_bounty: f64,
    /// Stack type -> (material index, units per stack unit).
    pub stacks: HashMap<String, Vec<(usize, f64)>>,
    pub lathes: HashMap<String, LatheDef>,
    pub consoles: HashMap<String, ConsoleDef>,
    pub departments: Vec<(u32, String)>,
    pub experiments: Vec<ExperimentDef>,
    pub experiment_index: HashMap<String, usize>,
    /// Limb design disk type -> its design types (`blueprints`).
    pub limb_disks: HashMap<String, Vec<String>>,
}

impl ResearchDb {
    pub fn new(data: ResearchData) -> Self {
        let node_index: HashMap<String, usize> = data
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.clone(), i))
            .collect();
        let design_index: HashMap<String, usize> = data
            .designs
            .iter()
            .enumerate()
            .map(|(i, d)| (d.id.clone(), i))
            .collect();
        let material_index: HashMap<String, usize> = data
            .materials
            .iter()
            .enumerate()
            .map(|(i, m)| (m.id.clone(), i))
            .collect();
        let mut db = Self {
            prerequisites: vec![Vec::new(); data.nodes.len()],
            unlocks: vec![Vec::new(); data.nodes.len()],
            node_designs: vec![Vec::new(); data.nodes.len()],
            unlocked_by: vec![Vec::new(); data.designs.len()],
            ..default()
        };
        for (i, node) in data.nodes.iter().enumerate() {
            if node.flags & node_flags::STARTER != 0 {
                db.starting.push(i);
            }
            if node.flags & node_flags::HIDDEN != 0 {
                db.hidden.push(i);
            }
            for design in &node.designs {
                if let Some(&d) = design_index.get(design) {
                    db.node_designs[i].push(d);
                    db.unlocked_by[d].push(i);
                }
            }
            for item in &node.required_items {
                db.unlock_items.entry(item.clone()).or_default().push(i);
            }
            for prerequisite in &node.prerequisites {
                if let Some(&p) = node_index.get(prerequisite) {
                    db.prerequisites[i].push(p);
                    db.unlocks[p].push(i);
                }
            }
        }
        db.stacks = data
            .stacks
            .into_iter()
            .map(|(stack, mats)| {
                let mats = mats
                    .into_iter()
                    .filter_map(|(m, n)| Some((*material_index.get(&m)?, n)))
                    .collect();
                (stack, mats)
            })
            .collect();
        db.lathes = data
            .lathes
            .into_iter()
            .map(|l| (l.path.clone(), l))
            .collect();
        db.consoles = data
            .consoles
            .into_iter()
            .map(|c| (c.path.clone(), c))
            .collect();
        db.point_items = data.point_items.into_iter().collect();
        db.nodes = data.nodes;
        db.designs = data.designs;
        db.materials = data.materials;
        db.node_index = node_index;
        db.design_index = design_index;
        db.material_index = material_index;
        db.point_types = data.point_types;
        db.server_income = data.server_income;
        db.techweb_bounty = data.techweb_bounty;
        db.departments = data.departments;
        db.experiment_index = data
            .experiments
            .iter()
            .enumerate()
            .map(|(i, e)| (e.id.clone(), i))
            .collect();
        db.experiments = data.experiments;
        db.limb_disks = data.limb_disks.into_iter().collect();
        db
    }

    pub fn node_name(&self, node: usize) -> &str {
        &self.nodes[node].name
    }
}

/// One `research_logs` entry.
#[derive(Clone, Debug, PartialEq)]
pub struct ResearchLog {
    pub node_name: String,
    pub node_cost: f64,
    pub node_researcher: String,
    pub node_research_location: String,
}

/// tg `/datum/techweb` (the station's `/datum/techweb/science`).
#[derive(Resource, Clone, Debug, Default)]
pub struct Techweb {
    pub id: String,
    pub organization: String,
    pub researched: BTreeSet<usize>,
    pub visible: BTreeSet<usize>,
    pub available: BTreeSet<usize>,
    pub hidden: BTreeSet<usize>,
    pub researched_designs: BTreeSet<usize>,
    pub boosted: BTreeSet<usize>,
    pub deconstructed_items: HashSet<String>,
    pub points: BTreeMap<String, f64>,
    /// Per-second income at the last generation, for display.
    pub last_bitcoins: BTreeMap<String, f64>,
    /// 0 researched, 1 available, more: steps away.
    pub tiers: BTreeMap<usize, u32>,
    pub completed_experiments: BTreeSet<String>,
    /// `available_experiments`.
    pub available_experiments: Vec<Experiment>,
    /// `skipped_experiment_types`: discount experiments of nodes researched without them ->
    /// the refund on completion (-1 once refunded).
    pub skipped_experiments: HashMap<String, f64>,
    pub experiment_rng: ExperimentRng,
    /// `research_queue_nodes` in order, with the mob that queued each.
    pub queue: Vec<(usize, Entity)>,
    pub logs: Vec<ResearchLog>,
    pub income_modifier: f64,
    /// Seconds (game time) of the last point generation.
    pub last_income: Option<f64>,
}

/// tg `FLOOR(x, 0.1)`.
fn floor_tenth(x: f64) -> f64 {
    (x * 10.0 + 1e-9).floor() / 10.0
}

impl Techweb {
    /// tg `/datum/techweb/science/New()`: the starting nodes researched for free, hidden nodes
    /// hidden.
    pub fn science(db: &ResearchDb) -> Self {
        Self::science_seeded(db, ExperimentRng::default())
    }

    /// [`Self::science`] with the round's experiment randomness.
    pub fn science_seeded(db: &ResearchDb, experiment_rng: ExperimentRng) -> Self {
        let mut web = Self {
            id: "SCIENCE".into(),
            organization: "Nanotrasen".into(),
            income_modifier: 1.0,
            experiment_rng,
            ..default()
        };
        for &node in &db.starting {
            web.research_node(db, node, true, false);
        }
        web.hidden = db.hidden.iter().copied().collect();
        web
    }

    /// tg `adjust_points`: unknown point types and zero amounts do nothing; never below 0.
    pub fn adjust_points(&mut self, db: &ResearchDb, point_type: &str, amount: f64) {
        if amount == 0.0 || !db.point_types.iter().any(|(t, _)| t == point_type) {
            return;
        }
        let entry = self.points.entry(point_type.to_owned()).or_default();
        *entry = floor_tenth(*entry + amount).max(0.0);
    }

    pub fn adjust_multiple_points(&mut self, db: &ResearchDb, points: &[(String, f64)]) {
        for (t, n) in points {
            self.adjust_points(db, t, *n);
        }
    }

    pub fn points_of(&self, point_type: &str) -> f64 {
        self.points.get(point_type).copied().unwrap_or(0.0)
    }

    /// tg `get_price(host)`: the costs less completed discount experiments.
    pub fn price(&self, db: &ResearchDb, node: usize) -> Vec<(String, f64)> {
        let def = &db.nodes[node];
        let discount: f64 = def
            .discount_experiments
            .iter()
            .filter(|(e, _)| self.completed_experiments.contains(e))
            .map(|(_, n)| n)
            .sum();
        def.costs
            .iter()
            .map(|(t, n)| (t.clone(), n - discount))
            .collect()
    }

    /// tg `is_free`: the price sums to 0.
    pub fn is_free(&self, db: &ResearchDb, node: usize) -> bool {
        self.price(db, node).iter().map(|(_, n)| n).sum::<f64>() == 0.0
    }

    pub fn can_afford(&self, price: &[(String, f64)]) -> bool {
        price.iter().all(|(t, n)| self.points_of(t) >= *n)
    }

    /// tg `have_experiments_for_node`; see the module docs for the experiments shortcut.
    pub fn have_experiments_for_node(&self, db: &ResearchDb, node: usize) -> bool {
        db.nodes[node].required_experiments.iter().all(|e| {
            self.completed_experiments.contains(e) || !super::experiments::performable(db, e)
        })
    }

    /// tg `can_unlock_node`: affordable and experiments done.
    pub fn can_unlock(&self, db: &ResearchDb, node: usize) -> bool {
        self.can_afford(&self.price(db, node)) && self.have_experiments_for_node(db, node)
    }

    /// tg `add_design`: researched, and it unhides the nodes that unlock it.
    pub fn add_design(&mut self, db: &ResearchDb, design: usize) {
        self.researched_designs.insert(design);
        for node in &db.unlocked_by[design] {
            self.hidden.remove(node);
        }
    }

    /// tg `update_node_status`.
    pub fn update_node_status(&mut self, db: &ResearchDb, node: usize) {
        let researched = self.researched.contains(&node);
        let prerequisites = &db.prerequisites[node];
        let done = prerequisites
            .iter()
            .filter(|p| self.researched.contains(p))
            .count();
        let visible = done > 0;
        let available = done == prerequisites.len();
        self.researched.remove(&node);
        self.available.remove(&node);
        self.visible.remove(&node);
        if self.hidden.contains(&node) {
            return;
        }
        if researched {
            self.researched.insert(node);
            for &design in &db.node_designs[node] {
                if !self.researched_designs.contains(&design) {
                    self.add_design(db, design);
                }
            }
        } else if available {
            self.available.insert(node);
        } else if visible {
            self.visible.insert(node);
        }
        self.update_tiers(db, node);
    }

    /// tg `update_tiers`: researched nodes are tier 0, others one more than their furthest
    /// prerequisite; changes spread to the nodes they unlock.
    pub fn update_tiers(&mut self, db: &ResearchDb, base: usize) {
        let mut current = vec![base];
        while !current.is_empty() {
            let mut next = Vec::new();
            for node in current {
                let mut tier = 0;
                if !self.researched.contains(&node) {
                    for p in &db.prerequisites[node] {
                        tier = tier.max(self.tiers.get(p).copied().unwrap_or(0) + 1);
                    }
                }
                if self.tiers.get(&node) != Some(&tier) {
                    self.tiers.insert(node, tier);
                    next.extend(db.unlocks[node].iter().copied());
                }
            }
            current = next;
        }
    }

    /// tg `research_node(node, force, auto_adjust_cost)`. The caller pays the science budget
    /// its bounty (`get_that_dosh`).
    pub fn research_node(
        &mut self,
        db: &ResearchDb,
        node: usize,
        force: bool,
        auto_adjust_cost: bool,
    ) -> bool {
        self.update_node_status(db, node);
        if !force
            && (!self.available.contains(&node)
                || (auto_adjust_cost && !self.can_afford(&self.price(db, node)))
                || !self.have_experiments_for_node(db, node))
        {
            return false;
        }
        if auto_adjust_cost {
            let cost: Vec<(String, f64)> = self
                .price(db, node)
                .into_iter()
                .map(|(t, n)| (t, -n))
                .collect();
            self.adjust_multiple_points(db, &cost);
        }
        self.researched.insert(node);
        // Track any experiments we skipped relating to this.
        for (missed, refund) in &db.nodes[node].discount_experiments {
            if self.completed_experiments.contains(missed)
                || self.skipped_experiments.contains_key(missed)
            {
                continue;
            }
            self.skipped_experiments.insert(missed.clone(), *refund);
        }
        for &unlocked in &db.unlocks[node] {
            self.visible.insert(unlocked);
            let def = &db.nodes[unlocked];
            self.add_experiments(db, &def.required_experiments);
            self.add_experiments(db, def.discount_experiments.iter().map(|(e, _)| e));
            self.update_node_status(db, unlocked);
        }
        self.add_experiments(db, &db.nodes[node].experiments_to_unlock);
        for &design in &db.node_designs[node] {
            self.add_design(db, design);
        }
        self.update_node_status(db, node);
        self.queue.retain(|(n, _)| *n != node);
        true
    }

    /// tg `unhide_node`.
    pub fn unhide_node(&mut self, db: &ResearchDb, node: usize) {
        self.hidden.remove(&node);
        self.update_node_status(db, node);
    }

    /// tg `enqueue_node`: one queued node per user; with research director access it goes first
    /// (also when already queued by someone else).
    pub fn enqueue_node(&mut self, node: usize, user: Entity, first: bool) -> bool {
        if self.queue.iter().any(|(n, _)| *n == node) {
            if !first {
                return false;
            }
            self.queue.retain(|(n, _)| *n != node);
        }
        self.queue.retain(|(_, u)| *u != user);
        if first {
            self.queue.insert(0, (node, user));
        } else {
            self.queue.push((node, user));
        }
        true
    }

    /// tg `dequeue_node`: only by who queued it.
    pub fn dequeue_node(&mut self, node: usize, user: Entity) -> bool {
        let before = self.queue.len();
        self.queue.retain(|(n, u)| !(*n == node && *u == user));
        self.queue.len() != before
    }

    /// The `SSresearch.fire()` queue step: the first queued node if affordable, then every
    /// free one. Returns the researched nodes.
    pub fn process_queue(&mut self, db: &ResearchDb) -> Vec<usize> {
        let mut done = Vec::new();
        if let Some(&(first, _)) = self.queue.first()
            && self.research_node(db, first, false, true)
        {
            done.push(first);
        }
        let queued: Vec<usize> = self.queue.iter().map(|(n, _)| *n).collect();
        for node in queued {
            if self.is_free(db, node) && self.research_node(db, node, false, true) {
                done.push(node);
            }
        }
        done
    }

    /// The point step of `SSresearch.fire()` at game time `now`: `income` per second when a
    /// server works, scaled by the time since the last step.
    pub fn generate_points(&mut self, db: &ResearchDb, working: bool, now: f64) {
        let income: Vec<(String, f64)> = if working {
            db.server_income.clone()
        } else {
            Vec::new()
        };
        if let Some(last) = self.last_income {
            self.last_bitcoins = income.iter().cloned().collect();
            let scaled: Vec<(String, f64)> = income
                .into_iter()
                .map(|(t, n)| (t, n * (now - last) * self.income_modifier))
                .collect();
            self.adjust_multiple_points(db, &scaled);
        }
        self.last_income = Some(now);
    }

    /// Visible nodes as the console lists them (`tiers` entries not hidden).
    pub fn shown_nodes(&self) -> impl Iterator<Item = (usize, u32)> + '_ {
        self.tiers
            .iter()
            .filter(|(n, _)| !self.hidden.contains(n))
            .map(|(n, t)| (*n, *t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, flags: u32, prereqs: &[&str], designs: &[&str], cost: f64) -> NodeDef {
        NodeDef {
            id: id.into(),
            name: id.into(),
            flags,
            prerequisites: prereqs.iter().map(|s| (*s).into()).collect(),
            designs: designs.iter().map(|s| (*s).into()).collect(),
            costs: if cost > 0.0 {
                vec![(POINT_GENERIC.into(), cost)]
            } else {
                vec![]
            },
            ..default()
        }
    }

    fn db() -> ResearchDb {
        ResearchDb::new(ResearchData {
            nodes: vec![
                node("base", node_flags::STARTER, &[], &["d_base"], 0.0),
                node("a", 0, &["base"], &["d_a"], 40.0),
                node("b", 0, &["base"], &[], 80.0),
                node("ab", 0, &["a", "b"], &["d_ab"], 40.0),
                node("secret", node_flags::HIDDEN, &["base"], &["d_secret"], 40.0),
            ],
            designs: ["d_base", "d_a", "d_ab", "d_secret"]
                .iter()
                .map(|d| DesignDef {
                    id: (*d).into(),
                    ..default()
                })
                .collect(),
            point_types: vec![(POINT_GENERIC.into(), "Gen. Res.".into())],
            server_income: vec![(POINT_GENERIC.into(), 1.0)],
            ..default()
        })
    }

    #[test]
    fn starting_nodes_and_tiers() {
        let db = db();
        let web = Techweb::science(&db);
        assert!(web.researched.contains(&0));
        assert!(web.researched_designs.contains(&0));
        // tg hides nodes after researching the starters, so a hidden one stays in `available`
        // until its status is updated; it is neither shown nor researchable.
        assert!(web.available.is_superset(&BTreeSet::from([1, 2])));
        assert_eq!(web.tiers.get(&0), Some(&0));
        assert_eq!(web.tiers.get(&1), Some(&1));
        assert_eq!(web.tiers.get(&3), Some(&2), "two steps from researched");
        let shown: Vec<usize> = web.shown_nodes().map(|(n, _)| n).collect();
        assert!(!shown.contains(&4), "hidden nodes are not listed");
    }

    #[test]
    fn unlock_needs_points_and_every_prerequisite() {
        let db = db();
        let mut web = Techweb::science(&db);
        assert!(!web.research_node(&db, 1, false, true), "no points");
        web.adjust_points(&db, POINT_GENERIC, 100.0);
        assert!(
            !web.research_node(&db, 3, false, true),
            "prerequisites missing"
        );
        assert!(web.research_node(&db, 1, false, true));
        assert_eq!(web.points_of(POINT_GENERIC), 60.0);
        assert!(web.researched_designs.contains(&1));
        assert!(
            web.visible.contains(&3) && !web.available.contains(&3),
            "one of two prerequisites makes it visible, not available"
        );
        assert!(
            !web.research_node(&db, 1, false, true),
            "already researched"
        );
        assert!(!web.research_node(&db, 2, false, true), "80 > 60");
        web.adjust_points(&db, POINT_GENERIC, 20.0);
        assert!(web.research_node(&db, 2, false, true));
        assert!(web.available.contains(&3));
        assert_eq!(web.tiers.get(&3), Some(&1));
    }

    #[test]
    fn hidden_nodes_unhide_by_design_or_analyzer() {
        let db = db();
        let mut web = Techweb::science(&db);
        web.adjust_points(&db, POINT_GENERIC, 100.0);
        assert!(!web.research_node(&db, 4, false, true), "hidden");
        web.unhide_node(&db, 4);
        assert!(web.available.contains(&4));
        assert!(web.research_node(&db, 4, false, true));
    }

    #[test]
    fn points_accrue_per_second_while_a_server_works() {
        let db = db();
        let mut web = Techweb::science(&db);
        web.generate_points(&db, true, 10.0);
        assert_eq!(
            web.points_of(POINT_GENERIC),
            0.0,
            "the first step only marks the time"
        );
        web.generate_points(&db, true, 12.5);
        assert_eq!(web.points_of(POINT_GENERIC), 2.5);
        assert_eq!(web.last_bitcoins.get(POINT_GENERIC), Some(&1.0));
        web.generate_points(&db, false, 100.0);
        assert_eq!(web.points_of(POINT_GENERIC), 2.5);
        assert!(web.last_bitcoins.is_empty());
        web.adjust_points(&db, POINT_GENERIC, -10.0);
        assert_eq!(web.points_of(POINT_GENERIC), 0.0, "never negative");
        web.adjust_points(&db, "Nonsense", 10.0);
        assert!(!web.points.contains_key("Nonsense"));
    }

    #[test]
    fn queue_researches_when_affordable() {
        let db = db();
        let mut web = Techweb::science(&db);
        let (alice, bob) = (
            Entity::from_raw_u32(1).unwrap(),
            Entity::from_raw_u32(2).unwrap(),
        );
        assert!(web.enqueue_node(2, alice, false));
        assert!(web.enqueue_node(1, alice, false), "replaces alice's entry");
        assert_eq!(web.queue, vec![(1, alice)]);
        assert!(!web.enqueue_node(1, bob, false), "already queued");
        assert!(web.enqueue_node(2, bob, true), "RD access goes first");
        assert_eq!(web.queue[0], (2, bob));
        assert!(!web.dequeue_node(2, alice), "only the queuer dequeues");
        web.adjust_points(&db, POINT_GENERIC, 80.0);
        assert_eq!(web.process_queue(&db), vec![2]);
        assert_eq!(web.queue, vec![(1, alice)]);
        assert!(web.process_queue(&db).is_empty(), "0 points left");
    }
}
