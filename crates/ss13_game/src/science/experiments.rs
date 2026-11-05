//! Experiments of the station techweb, after tg `code/modules/experisci/experiment/`
//! (`/datum/experiment`, `/datum/experiment/scanning` and its `points`, `machinery_tiered_scan`,
//! `machinery_pinpoint_scan`, `reagent` and `random` subtypes) and the experiment half of
//! `_techweb.dm` (`add_experiment`, `complete_experiment`, `skipped_experiment_types`).
//!
//! The techweb gains an experiment when a node that requires it or is discounted by it becomes
//! reachable (its prerequisite is researched) or when a node with `experiments_to_unlock` is
//! researched. An [`Experiment`] counts the scans of its required types; completing it unlocks
//! the nodes that require it, discounts the ones it discounts, refunds the discount of a node
//! researched without it and pays its `points_reward`. The handlers that perform the scans (the
//! experi-scanner, the destructive scanner) are in [`super::experi_scanner`].
//!
//! Cost: experiments change only on scans and research; a scan looks at one experiment.
// shortcut: experiments of the unsupported kinds (people, plants, materials, mechs, cytology,
// fish, autopsies, ordnance, physical events, exploration: `ExperimentKind::Unsupported`) are never
// added to the techweb, and a node that requires one counts it as done; port each with the system
// its checks read.

use std::collections::BTreeSet;

use bevy::prelude::*;
use ss13_mapload::research::{ExperimentDef, ExperimentKind};

use super::ProgressStage;
use super::techweb::{POINT_GENERIC, ResearchDb, Techweb};
use crate::obj_damage::is_under;

/// `EXPERIMENT_TRAIT_DESTRUCTIVE`: scanning destroys the target (destructive scanner only).
pub const TRAIT_DESTRUCTIVE: u32 = 1 << 0;
/// `EXPERIMENT_TRAIT_TYPECACHE`: each type counts once.
pub const TRAIT_TYPECACHE: u32 = 1 << 1;

/// The experiment randomness (`scanning/random` picks): xorshift64*, seeded per round.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExperimentRng(u64);

impl Default for ExperimentRng {
    fn default() -> Self {
        Self::new(0x9E37_79B9_7F4A_7C15)
    }
}

impl ExperimentRng {
    pub fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// DM `rand(low, high)`, both inclusive.
    pub fn range(&mut self, low: u32, high: u32) -> u32 {
        if high <= low {
            return low;
        }
        low + (self.next_u64() % u64::from(high - low + 1)) as u32
    }
}

/// What has been scanned for one required type (tg `scanned[type]`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scanned {
    /// Non-destructive scans: the scanned atoms (tg weakrefs).
    pub seen: Vec<Entity>,
    /// Destructive scans: how many were destroyed.
    pub destroyed: u32,
    /// `EXPERIMENT_TRAIT_TYPECACHE`: the scanned types.
    pub types: BTreeSet<String>,
}

/// An experiment instance of the techweb (tg `/datum/experiment` in `available_experiments`).
#[derive(Clone, Debug, PartialEq)]
pub struct Experiment {
    /// Index into [`ResearchDb::experiments`].
    pub def: usize,
    /// The type path.
    pub id: String,
    /// `required_atoms` after `scanning/random/New()` rolled them: type -> count (points per
    /// scan for the points kinds).
    pub required: Vec<(String, u32)>,
    pub scanned: Vec<Scanned>,
    /// `scanning/points` `points`.
    pub points: u32,
}

/// The points kinds (`/datum/experiment/scanning/points`).
fn is_points(def: &ExperimentDef) -> bool {
    matches!(
        def.kind,
        ExperimentKind::Points
            | ExperimentKind::MachineryTiered { .. }
            | ExperimentKind::MachineryPinpoint { .. }
    )
}

/// DM `declension_ru(num, single, double, multiple)` (BandaStation `pronouns.dm`).
pub fn declension_ru<'a>(num: u32, single: &'a str, double: &'a str, multiple: &'a str) -> &'a str {
    if num % 10 == 1 && num % 100 != 11 {
        single
    } else if (2..=4).contains(&(num % 10)) && !(12..=14).contains(&(num % 100)) {
        double
    } else {
        multiple
    }
}

/// DM `english_list`: "a", "a and b", "a, b and c".
fn english_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// tg `get_points_reward_text`.
fn points_reward_text(reward: &[(String, f64)]) -> String {
    let parts: Vec<String> = reward.iter().map(|(t, n)| format!("{n} {t}")).collect();
    format!("{} points", english_list(&parts))
}

/// tg `PERCENT(x)`: `x * 100` rounded to 0.1, printed as DM prints numbers.
fn percent(x: f64) -> String {
    let p = (x * 1000.0).round() / 10.0;
    if p.fract() == 0.0 {
        format!("{p:.0}")
    } else {
        format!("{p:.1}")
    }
}

impl Experiment {
    /// tg `New()`: `scanning/random` rolls its required types.
    pub fn new(db: &ResearchDb, def: usize, rng: &mut ExperimentRng) -> Self {
        let d = &db.experiments[def];
        let mut required = d.required_atoms.clone();
        if !d.possible_types.is_empty() {
            let mut picked = 0;
            while picked < d.total_requirement {
                let n = rng
                    .range(1, d.total_requirement - picked)
                    .min(d.max_requirement_per_type.max(1));
                let t = &d.possible_types[rng.range(0, d.possible_types.len() as u32 - 1) as usize];
                match required.iter_mut().find(|(p, _)| p == t) {
                    Some(entry) => entry.1 += n,
                    None => required.push((t.clone(), n)),
                }
                picked += n;
            }
        }
        Self {
            def,
            id: d.id.clone(),
            scanned: vec![Scanned::default(); required.len()],
            required,
            points: 0,
        }
    }

    /// tg `is_complete()`.
    pub fn is_complete(&self, def: &ExperimentDef) -> bool {
        if is_points(def) {
            return self.points >= def.required_points;
        }
        let destructive = def.traits & TRAIT_DESTRUCTIVE != 0;
        let typecache = def.traits & TRAIT_TYPECACHE != 0;
        self.required.iter().zip(&self.scanned).all(|(&(_, n), s)| {
            if typecache {
                s.types.len() as u32 == n
            } else if destructive {
                s.destroyed == n
            } else {
                s.seen.len() as u32 == n
            }
        })
    }

    /// tg `experiment_requirements` without `final_contributing_index_checks`: the required
    /// type `target` (of type `path`) would count towards.
    pub fn requirement(&self, def: &ExperimentDef, target: Entity, path: &str) -> Option<usize> {
        let destructive = def.traits & TRAIT_DESTRUCTIVE != 0;
        let typecache = def.traits & TRAIT_TYPECACHE != 0;
        let points = is_points(def);
        self.required.iter().enumerate().find_map(|(i, (t, n))| {
            if !is_under(path, t) {
                return None;
            }
            let s = &self.scanned[i];
            let fits = if points {
                destructive || !s.seen.contains(&target)
            } else if typecache {
                (s.types.len() as u32) < *n && !s.types.contains(path)
            } else if destructive {
                s.destroyed < *n
            } else {
                (s.seen.len() as u32) < *n && !s.seen.contains(&target)
            };
            fits.then_some(i)
        })
    }

    /// tg `perform_experiment_actions` after the checks, and `do_after_experiment`.
    pub fn record(&mut self, def: &ExperimentDef, index: usize, target: Entity, path: &str) {
        let destructive = def.traits & TRAIT_DESTRUCTIVE != 0;
        let s = &mut self.scanned[index];
        if def.traits & TRAIT_TYPECACHE != 0 {
            s.types.insert(path.to_owned());
        } else if destructive {
            s.destroyed += 1;
        } else {
            s.seen.push(target);
        }
        if is_points(def) {
            self.points = def
                .required_points
                .min(self.points + self.required[index].1);
        }
    }

    /// The name of a required type, as the stage texts print it.
    fn name_of<'a>(def: &'a ExperimentDef, path: &'a str) -> &'a str {
        def.names
            .iter()
            .find(|(p, _)| p == path)
            .map_or(path, |(_, n)| n.as_str())
    }

    /// tg `check_progress()`.
    pub fn progress(&self, def: &ExperimentDef) -> Vec<ProgressStage> {
        if is_points(def) {
            let text = match &def.kind {
                ExperimentKind::MachineryTiered { tier } => format!(
                    "Отсканируйте образцы следующих машин, собранных с использованием деталей \
                     уровня {tier} или выше."
                ),
                ExperimentKind::MachineryPinpoint { part_name, .. } => format!(
                    "Отсканируйте образцы следующих машин, улучшенных с помощью {part_name}, \
                     чтобы набрать достаточное количество очков для завершения эксперимента."
                ),
                _ => "Сканируйте образцы следующих объектов для набора достаточного кол-ва очков \
                      и завершения эксперимента."
                    .to_owned(),
            };
            let mut stages = vec![ProgressStage::Integer {
                text,
                value: self.points,
                total: def.required_points,
            }];
            let complete = self.is_complete(def);
            // `point_val_cache`: types grouped by their points, in first-seen order.
            let mut groups: Vec<(u32, Vec<&str>)> = Vec::new();
            for (t, n) in &self.required {
                let name = Self::name_of(def, t);
                match groups.iter_mut().find(|(p, _)| p == n) {
                    Some((_, names)) => names.push(name),
                    None => groups.push((*n, vec![name])),
                }
            }
            for (n, names) in groups {
                stages.push(ProgressStage::Detail {
                    text: format!(
                        "{n} очк{}: {}",
                        declension_ru(n, "о", "а", "ов"),
                        names.join(", ")
                    ),
                    done: complete,
                });
            }
            return stages;
        }
        let destructive = def.traits & TRAIT_DESTRUCTIVE != 0;
        let typecache = def.traits & TRAIT_TYPECACHE != 0;
        self.required
            .iter()
            .zip(&self.scanned)
            .map(|((t, n), s)| {
                let value = if typecache {
                    s.types.len() as u32
                } else if destructive {
                    s.destroyed
                } else {
                    s.seen.len() as u32
                };
                let text = match &def.kind {
                    ExperimentKind::Reagent {
                        reagent_name,
                        min_purity,
                        ..
                    } => format!(
                        "Просканируйте контейнер с реагентом {reagent_name} с чистотой не менее \
                         {}%.",
                        percent(*min_purity)
                    ),
                    _ if !def.scan_message.is_empty() => def.scan_message.clone(),
                    _ => format!("Скан образцов: {}", Self::name_of(def, t)),
                };
                ProgressStage::Integer {
                    text,
                    value,
                    total: *n,
                }
            })
            .collect()
    }
}

/// The experiment can be performed here (see the module shortcut).
pub fn performable(db: &ResearchDb, id: &str) -> bool {
    db.experiment_index
        .get(id)
        .is_some_and(|&i| db.experiments[i].kind != ExperimentKind::Unsupported)
}

impl Techweb {
    /// tg `add_experiment`: a new instance unless one is available or completed.
    pub fn add_experiment(&mut self, db: &ResearchDb, id: &str) -> bool {
        let Some(&def) = db.experiment_index.get(id) else {
            return false;
        };
        if db.experiments[def].kind == ExperimentKind::Unsupported
            || self.available_experiments.iter().any(|e| e.id == id)
            || self.completed_experiments.contains(id)
        {
            return false;
        }
        let experiment = Experiment::new(db, def, &mut self.experiment_rng);
        self.available_experiments.push(experiment);
        true
    }

    /// tg `add_experiments`.
    pub fn add_experiments<'a>(
        &mut self,
        db: &ResearchDb,
        ids: impl IntoIterator<Item = &'a String>,
    ) {
        for id in ids {
            self.add_experiment(db, id);
        }
    }

    pub fn experiment(&self, id: &str) -> Option<&Experiment> {
        self.available_experiments.iter().find(|e| e.id == id)
    }

    /// tg `complete_experiment`: completed, the refund of a skipped discount and the points
    /// reward. Returns the announcement.
    pub fn complete_experiment(&mut self, db: &ResearchDb, id: &str) -> String {
        self.available_experiments.retain(|e| e.id != id);
        self.completed_experiments.insert(id.to_owned());
        let def = db.experiment_index.get(id).map(|&i| &db.experiments[i]);
        let name = def.map_or(id, |d| d.name.as_str());
        let mut text = format!("{name} has been completed");
        let refund = self.skipped_experiments.get(id).copied().unwrap_or(0.0);
        if refund > 0.0 {
            self.adjust_points(db, POINT_GENERIC, refund);
            text.push_str(&format!(", refunding {refund} points"));
            // Kept to prevent double dipping.
            self.skipped_experiments.insert(id.to_owned(), -1.0);
        }
        if let Some(def) = def
            && !def.points_reward.is_empty()
        {
            self.adjust_multiple_points(db, &def.points_reward);
            text.push_str(&format!(
                ",{} rewarding {}",
                if refund > 0.0 { " and" } else { "" },
                points_reward_text(&def.points_reward)
            ));
        }
        text.push('!');
        info!("research: experiment {id} completed");
        text
    }
}

#[cfg(test)]
mod tests {
    use ss13_mapload::research::{NodeDef, ResearchData};

    use super::*;

    fn exp(id: &str, kind: ExperimentKind, atoms: &[(&str, u32)]) -> ExperimentDef {
        ExperimentDef {
            id: id.into(),
            name: id.into(),
            kind,
            required_atoms: atoms.iter().map(|(t, n)| ((*t).into(), *n)).collect(),
            ..default()
        }
    }

    const SCAN: &str = "/datum/experiment/scanning/wrenches";
    const POINTS: &str = "/datum/experiment/scanning/points/crystal";
    const PEOPLE: &str = "/datum/experiment/scanning/people/android";

    fn db() -> ResearchDb {
        let node = |id: &str, prereqs: &[&str]| NodeDef {
            id: id.into(),
            name: id.into(),
            prerequisites: prereqs.iter().map(|s| (*s).into()).collect(),
            costs: vec![(POINT_GENERIC.into(), 100.0)],
            ..default()
        };
        ResearchDb::new(ResearchData {
            nodes: vec![
                NodeDef {
                    flags: ss13_mapload::research::node_flags::STARTER,
                    costs: vec![],
                    ..node("base", &[])
                },
                NodeDef {
                    required_experiments: vec![SCAN.into(), PEOPLE.into()],
                    ..node("gated", &["base"])
                },
                NodeDef {
                    discount_experiments: vec![(POINTS.into(), 60.0)],
                    ..node("cheap", &["base"])
                },
            ],
            experiments: vec![
                exp(SCAN, ExperimentKind::Scanning, &[("/obj/item/wrench", 2)]),
                ExperimentDef {
                    required_points: 3,
                    points_reward: vec![(POINT_GENERIC.into(), 5.0)],
                    ..exp(
                        POINTS,
                        ExperimentKind::Points,
                        &[
                            ("/obj/item/crystal/artificial", 1),
                            ("/obj/item/crystal/ore", 2),
                        ],
                    )
                },
                exp(PEOPLE, ExperimentKind::Unsupported, &[]),
            ],
            point_types: vec![(POINT_GENERIC.into(), "Gen. Res.".into())],
            ..default()
        })
    }

    fn e(n: u32) -> Entity {
        Entity::from_raw_u32(n).unwrap()
    }

    #[test]
    fn reachable_nodes_bring_their_experiments() {
        let db = db();
        let web = Techweb::science(&db);
        let ids: Vec<&str> = web
            .available_experiments
            .iter()
            .map(|e| e.id.as_str())
            .collect();
        assert_eq!(ids, vec![SCAN, POINTS], "the unsupported one is left out");
    }

    #[test]
    fn required_experiments_gate_the_node() {
        let db = db();
        let mut web = Techweb::science(&db);
        let gated = db.node_index["gated"];
        web.adjust_points(&db, POINT_GENERIC, 500.0);
        assert!(!web.have_experiments_for_node(&db, gated));
        assert!(!web.research_node(&db, gated, false, true));
        let def = &db.experiments[0];
        let mut x = web.experiment(SCAN).unwrap().clone();
        assert_eq!(x.requirement(def, e(1), "/obj/item/screwdriver"), None);
        assert_eq!(x.requirement(def, e(1), "/obj/item/wrench/combat"), Some(0));
        x.record(def, 0, e(1), "/obj/item/wrench/combat");
        assert_eq!(
            x.requirement(def, e(1), "/obj/item/wrench"),
            None,
            "same one twice"
        );
        assert!(!x.is_complete(def));
        assert_eq!(
            x.progress(def),
            vec![ProgressStage::Integer {
                text: "Скан образцов: /obj/item/wrench".into(),
                value: 1,
                total: 2
            }]
        );
        x.record(def, 0, e(2), "/obj/item/wrench");
        assert!(x.is_complete(def));
        assert_eq!(
            web.complete_experiment(&db, SCAN),
            format!("{SCAN} has been completed!")
        );
        assert!(web.experiment(SCAN).is_none());
        assert!(
            web.have_experiments_for_node(&db, gated),
            "the unsupported requirement counts as done"
        );
        assert!(web.research_node(&db, gated, false, true));
        assert!(
            !web.add_experiment(&db, SCAN),
            "completed experiments stay completed"
        );
    }

    #[test]
    fn discounts_refunds_and_rewards() {
        let db = db();
        let mut web = Techweb::science(&db);
        let cheap = db.node_index["cheap"];
        web.adjust_points(&db, POINT_GENERIC, 100.0);
        assert!(
            web.research_node(&db, cheap, false, true),
            "full price without it"
        );
        assert_eq!(web.points_of(POINT_GENERIC), 0.0);
        assert_eq!(web.skipped_experiments.get(POINTS), Some(&60.0));

        let def = &db.experiments[1];
        let mut x = web.experiment(POINTS).unwrap().clone();
        let ore = "/obj/item/crystal/ore";
        let i = x.requirement(def, e(1), ore).unwrap();
        x.record(def, i, e(1), ore);
        assert_eq!(x.points, 2);
        assert_eq!(x.requirement(def, e(1), ore), None, "each atom counts once");
        let i = x.requirement(def, e(2), ore).unwrap();
        x.record(def, i, e(2), ore);
        assert_eq!(x.points, 3, "capped at required_points");
        assert!(x.is_complete(def));
        let stages = x.progress(def);
        assert_eq!(stages.len(), 3);
        assert_eq!(stages[2].text(), "2 очка: /obj/item/crystal/ore");

        let text = web.complete_experiment(&db, POINTS);
        assert_eq!(
            text,
            format!(
                "{POINTS} has been completed, refunding 60 points, and rewarding 5 General \
                 Research points!"
            )
        );
        assert_eq!(web.points_of(POINT_GENERIC), 65.0);
        assert_eq!(web.skipped_experiments.get(POINTS), Some(&-1.0));
    }

    #[test]
    fn random_experiments_roll_their_total() {
        let mut db = db();
        db.experiments[0].required_atoms.clear();
        db.experiments[0].possible_types = vec!["/obj/a".into(), "/obj/b".into()];
        db.experiments[0].total_requirement = 5;
        db.experiments[0].max_requirement_per_type = 2;
        for seed in 0..20 {
            let mut rng = ExperimentRng::new(seed);
            let x = Experiment::new(&db, 0, &mut rng);
            assert_eq!(x.required.iter().map(|(_, n)| n).sum::<u32>(), 5);
            assert_eq!(x.scanned.len(), x.required.len());
        }
        assert_eq!(declension_ru(1, "о", "а", "ов"), "о");
        assert_eq!(declension_ru(3, "о", "а", "ов"), "а");
        assert_eq!(declension_ru(12, "о", "а", "ов"), "ов");
        assert_eq!(percent(0.98), "98");
        assert_eq!(percent(0.995), "99.5");
    }
}
