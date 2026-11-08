//! Ordnance experiments, scientific papers and partners, after tg
//! `code/modules/experisci/experiment/types/ordnance.dm` (`/datum/experiment/ordnance` and its
//! `explosive` / `gaseous` subtypes in `experiments.dm`), `research/ordnance/_scipaper.dm`
//! (`/datum/scientific_paper`, `/datum/scientific_partner`), `scipaper_partner.dm`, the paper
//! half of `_techweb.dm` (`add_scientific_paper`, `boost_techweb_node`) and the NT Frontier
//! program (`/datum/computer_file/program/scipaper_program`, tgui `NtosScipaper`).
//!
//! - An ordnance experiment is completed only by publishing a paper on it: the doppler array
//!   and the tank compressor (`super::ordnance_machines`) print their records to a data disk
//!   with the experiments each record qualifies for and its score (light radius or moles).
//! - In NT Frontier a paper takes a file, one of its experiments, a tier the score reaches
//!   (the sigmoid gain is over 10% of the tier's gain) and a partner that accepts the
//!   experiment. Published, it fills that tier of the experiment (once per tier and once per
//!   record), completes the experiment, gives the partner its cooperation gain and the science
//!   budget its funding gain × 125 credits.
//! - Cooperation buys partner boosts: a node's generic cost lowered by the boost (bought for
//!   1% of it in cooperation); a node already researched refunds the boost in points.
//!
//! The experiments' gains, targets and requirements and the partners are the DM tables below.
// shortcut: one techweb (no multitool relinking); the tier/partner tables are copied from DM
// rather than converted; papers do not announce completion to the experiment handlers.

use std::collections::BTreeMap;

use bevy::prelude::*;
use ss13_atmos::GasId;
use ss13_tgui::server::{UiParams, UiReject};

use super::techweb::{POINT_GENERIC, ResearchDb, Techweb};
use crate::modpc::files::{ComputerDisk, OrdnanceRecord};
use crate::modpc::station_ui::*;

pub const ORDNANCE: &str = "/datum/experiment/ordnance";
pub const EXPLOSIVE: &str = "/datum/experiment/ordnance/explosive";
pub const GASEOUS: &str = "/datum/experiment/ordnance/gaseous";
/// `SCIENTIFIC_COOPERATION_PURCHASE_MULTIPLIER`, `SCIPAPER_GAIN_TO_MONEY`.
pub const PURCHASE_MULTIPLIER: f64 = 0.01;
pub const GAIN_TO_MONEY: f64 = 125.0;
/// `MINIMUM_MOLE_COUNT`.
pub const MINIMUM_MOLE_COUNT: f64 = 0.01;
const TIER_1: f64 = 40.0;
const TIER_2: f64 = 80.0;
const TIER_3: f64 = 120.0;
const TIER_4: f64 = 160.0;
const TIER_5: f64 = 200.0;

/// What a record must show for the experiment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Requirement {
    /// `ordnance/explosive`.
    Explosive {
        /// `required_reactions` (tg ids).
        reactions: &'static [&'static str],
        sanitized_reactions: bool,
        require_all: bool,
        sanitized_misc: bool,
        allow_any_source: bool,
    },
    /// `ordnance/gaseous` `required_gas`.
    Gaseous(GasId),
}

/// A proper (`experiment_proper`) `/datum/experiment/ordnance` subtype.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrdnanceExperiment {
    pub path: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    /// `gain` and `target_amount` by tier (index 0 is tier 1).
    pub gain: &'static [f64],
    pub target: &'static [f64],
    pub requirement: Requirement,
}

impl OrdnanceExperiment {
    pub fn explosive(&self) -> bool {
        matches!(self.requirement, Requirement::Explosive { .. })
    }

    /// `min(length(target_amount), length(gain))`.
    pub fn tiers(&self) -> u32 {
        self.gain.len().min(self.target.len()) as u32
    }
}

const fn explosive(
    reactions: &'static [&'static str],
    sanitized_reactions: bool,
    require_all: bool,
    sanitized_misc: bool,
    allow_any_source: bool,
) -> Requirement {
    Requirement::Explosive {
        reactions,
        sanitized_reactions,
        require_all,
        sanitized_misc,
        allow_any_source,
    }
}

const GAS_DESC: &str = "Упакуйте указанный газ в канистру и взорвите его с помощью компрессора \
                        для канистр. Опубликуйте данные в отчёте.";

/// `SSresearch.ordnance_experiments`, in declaration order (`experiments.dm`).
pub static EXPERIMENTS: [OrdnanceExperiment; 9] = [
    OrdnanceExperiment {
        path: "/datum/experiment/ordnance/explosive/lowyieldbomb",
        name: "Маломощная взрывчатка",
        description: "Маломощные взрывчатые вещества могут оказаться полезными для наших групп \
                      по защите активов. Зафиксируйте небольшой взрыв с помощью доплеровской \
                      матрицы и опубликуйте данные в отчёте.",
        gain: &[10.0, 15.0, 20.0],
        target: &[5.0, 10.0, 20.0],
        requirement: explosive(&[], false, true, false, true),
    },
    OrdnanceExperiment {
        path: "/datum/experiment/ordnance/explosive/highyieldbomb",
        name: "Высокомощная взрывчатка",
        description: "Некоторые реакции протекают очень энергично и могут быть использованы для \
                      создания более мощных взрывчатых веществ. Зафиксируйте взрыв любой \
                      канистры с помощью доплеровской матрицы и опубликуйте данные в отчёте. \
                      Допускается любая газовая реакция.",
        gain: &[10.0, 50.0, 100.0],
        target: &[50.0, 100.0, 300.0],
        requirement: explosive(&[], false, true, false, false),
    },
    OrdnanceExperiment {
        path: "/datum/experiment/ordnance/explosive/hydrogenbomb",
        name: "Водородная взрывчатка",
        description: "Горение водорода и его производных может быть очень мощным. Зафиксируйте \
                      любой взрыв канистры с помощью доплеровской матрицы и опубликуйте данные \
                      в отчёте. Разрешены только водородные или тритиевые взрывы.",
        gain: &[15.0, 40.0, 60.0],
        target: &[50.0, 75.0, 150.0],
        requirement: explosive(&["h2fire", "tritfire"], true, false, true, false),
    },
    OrdnanceExperiment {
        path: "/datum/experiment/ordnance/explosive/nobliumbomb",
        name: "Ноблиевая взрывчатка",
        description: "Гиперноблиум образуется очень энергично, и его можно использовать для \
                      изготовления взрывчатых веществ. Зафиксируйте взрыв любой канистры с \
                      помощью доплеровской матрицы и опубликуйте данные в отчёте. Разрешена \
                      только конденсация гиперноблия.",
        gain: &[15.0, 60.0, 120.0],
        target: &[50.0, 100.0, 300.0],
        requirement: explosive(&["nobformation"], true, true, true, false),
    },
    OrdnanceExperiment {
        path: "/datum/experiment/ordnance/explosive/pressurebomb",
        name: "Безреакционная взрывчатка",
        description: "Газы с высокой удельной теплоемкостью могут нагревать газы с низкой и \
                      создавать большое давление. Зафиксируйте взрыв любой канистры с помощью \
                      доплеровской матрицы и опубликуйте данные в отчёте. Никаких газовых \
                      реакций не допускается.",
        gain: &[10.0, 50.0, 100.0],
        target: &[20.0, 50.0, 100.0],
        requirement: explosive(&[], true, true, false, false),
    },
    OrdnanceExperiment {
        path: "/datum/experiment/ordnance/gaseous/nitrous_oxide",
        name: "Газовые баллоны с закисью азота",
        description: GAS_DESC,
        gain: &[10.0, 40.0],
        target: &[200.0, 600.0],
        requirement: Requirement::Gaseous(GasId::N2O),
    },
    OrdnanceExperiment {
        path: "/datum/experiment/ordnance/gaseous/plasma",
        name: "Плазменные снаряды",
        description: GAS_DESC,
        gain: &[10.0, 40.0],
        target: &[200.0, 600.0],
        requirement: Requirement::Gaseous(GasId::PLASMA),
    },
    OrdnanceExperiment {
        path: "/datum/experiment/ordnance/gaseous/bz",
        name: "BZ-снаряды",
        description: GAS_DESC,
        gain: &[10.0, 30.0, 60.0],
        target: &[50.0, 125.0, 400.0],
        requirement: Requirement::Gaseous(GasId::BZ),
    },
    OrdnanceExperiment {
        path: "/datum/experiment/ordnance/gaseous/noblium",
        name: "Гиперноблиевые снаряды",
        description: GAS_DESC,
        gain: &[10.0, 40.0, 80.0],
        target: &[15.0, 55.0, 250.0],
        requirement: Requirement::Gaseous(GasId::HYPERNOBLIUM),
    },
];

/// A `/datum/scientific_partner` subtype.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Partner {
    pub path: &'static str,
    pub name: &'static str,
    pub flufftext: &'static str,
    /// Cooperation and funding multipliers.
    pub multipliers: (f64, f64),
    /// `accepted_experiments`; empty: every ordnance experiment.
    pub accepted: &'static [&'static str],
    /// `boostable_nodes`: node path -> generic points.
    pub boosts: &'static [(&'static str, f64)],
}

impl Partner {
    pub fn accepts(&self, experiment: &str) -> bool {
        if self.accepted.is_empty() {
            EXPERIMENTS.iter().any(|e| e.path == experiment)
        } else {
            self.accepted.contains(&experiment)
        }
    }

    pub fn boost(&self, node: &str) -> Option<f64> {
        self.boosts
            .iter()
            .find(|(n, _)| *n == node)
            .map(|(_, b)| *b)
    }
}

/// `SSresearch.scientific_partners` (`scipaper_partner.dm`).
pub static PARTNERS: [Partner; 5] = [
    Partner {
        path: "/datum/scientific_partner/mining",
        name: "Mining Corps",
        flufftext: "A local group of miners are looking for ways to improve their mining \
                    output. They are interested in smaller scale explosives and plasma research.",
        multipliers: (0.75, 0.75),
        accepted: &[
            "/datum/experiment/ordnance/gaseous/plasma",
            "/datum/experiment/ordnance/explosive/lowyieldbomb",
            "/datum/experiment/ordnance/explosive/highyieldbomb",
        ],
        boosts: &[
            ("/datum/techweb_node/low_pressure_excavation", TIER_2),
            ("/datum/techweb_node/plasma_mining", TIER_3),
            ("/datum/techweb_node/mining_adv", TIER_4),
            ("/datum/techweb_node/night_vision", TIER_4),
            ("/datum/techweb_node/borg_engi", TIER_3),
        ],
    },
    Partner {
        path: "/datum/scientific_partner/baron",
        name: "Ghost Writing",
        flufftext: "A nearby research station ran by a very wealthy captain seems to be \
                    struggling with their scientific output. They might reward us handsomely if \
                    we ghostwrite for them.",
        multipliers: (0.25, 2.0),
        accepted: &[],
        boosts: &[
            ("/datum/techweb_node/consoles", TIER_1),
            ("/datum/techweb_node/gaming", TIER_2),
            ("/datum/techweb_node/bitrunning", TIER_3),
            ("/datum/techweb_node/programmed_server", TIER_3),
        ],
    },
    Partner {
        path: "/datum/scientific_partner/defense",
        name: "Defense Partnership",
        flufftext: "We can work directly for Nanotrasen's [REDACTED] division, potentially \
                    providing us access with advanced offensive and defensive gadgets.",
        multipliers: (1.0, 1.0),
        accepted: &[
            "/datum/experiment/ordnance/explosive/lowyieldbomb",
            "/datum/experiment/ordnance/explosive/highyieldbomb",
            "/datum/experiment/ordnance/explosive/pressurebomb",
            "/datum/experiment/ordnance/explosive/hydrogenbomb",
        ],
        boosts: &[
            ("/datum/techweb_node/riot_supression", TIER_2),
            ("/datum/techweb_node/explosives", TIER_3),
            ("/datum/techweb_node/mech_energy_guns", TIER_4),
            ("/datum/techweb_node/mech_firearms", TIER_5),
            ("/datum/techweb_node/mech_heavy_arms", TIER_5),
        ],
    },
    Partner {
        path: "/datum/scientific_partner/medical",
        name: "Biological Research Division",
        flufftext: "A collegiate of the best medical researchers Nanotrasen employs. They seem \
                    to be interested in the biological effects of some more exotic gases. \
                    Especially stimulants and neurosupressants.",
        multipliers: (1.0, 1.0),
        accepted: &[
            "/datum/experiment/ordnance/gaseous/nitrous_oxide",
            "/datum/experiment/ordnance/gaseous/bz",
        ],
        boosts: &[
            ("/datum/techweb_node/cyber/cyber_organs", TIER_2),
            ("/datum/techweb_node/cyber/cyber_organs_upgraded", TIER_3),
            ("/datum/techweb_node/medbay_equip_adv", TIER_3),
            ("/datum/techweb_node/cytology", TIER_2),
            ("/datum/techweb_node/borg_medical", TIER_3),
            ("/datum/techweb_node/cyber/combat_implants", TIER_4),
        ],
    },
    Partner {
        path: "/datum/scientific_partner/physics",
        name: "NT Physics Quarterly",
        flufftext: "A prestigious physics journal managed by Nanotrasen. The main journal for \
                    publishing cutting-edge physics research conducted by Nanotrasen, given that \
                    they aren't classified.",
        multipliers: (1.0, 1.0),
        accepted: &[
            "/datum/experiment/ordnance/gaseous/bz",
            "/datum/experiment/ordnance/explosive/hydrogenbomb",
            "/datum/experiment/ordnance/gaseous/noblium",
            "/datum/experiment/ordnance/explosive/nobliumbomb",
        ],
        boosts: &[
            ("/datum/techweb_node/parts_adv", TIER_3),
            ("/datum/techweb_node/bluespace_travel", TIER_3),
            ("/datum/techweb_node/anomaly_research", TIER_3),
            ("/datum/techweb_node/telecomms", TIER_5),
            ("/datum/techweb_node/mech_equip_bluespace", TIER_5),
        ],
    },
];

pub fn experiment_index(path: &str) -> Option<usize> {
    EXPERIMENTS.iter().position(|e| e.path == path)
}

pub fn partner_index(path: &str) -> Option<usize> {
    PARTNERS.iter().position(|p| p.path == path)
}

/// An `/datum/experiment/ordnance` type (abstract ones included).
pub fn is_ordnance(path: &str) -> bool {
    path == ORDNANCE || path.starts_with("/datum/experiment/ordnance/")
}

/// The node's `discount_boosts[TECHWEB_POINT_TYPE_GENERIC]`, which the partners set on
/// `New()` (the last partner offering the node wins, as `LAZYSET` does).
pub fn node_boost(node: &str) -> Option<f64> {
    PARTNERS.iter().rev().find_map(|p| p.boost(node))
}

/// `progress()` of an ordnance experiment: published or not.
pub fn progress_text(name: &str) -> String {
    format!("Вы должны опубликовать отчёт по \"{name}\" с помощью приложения NT Frontier")
}

// ---------------------------------------------------------------------------------------------
// Papers

/// `/datum/scientific_paper` (with its `explosive` / `gaseous` record).
#[derive(Clone, Debug, PartialEq)]
pub struct Paper {
    pub title: String,
    pub author: String,
    pub et_alia: bool,
    pub abstract_text: String,
    /// Index into [`EXPERIMENTS`].
    pub experiment: Option<usize>,
    /// `tracked_variable`: the score.
    pub tracked: f64,
    /// `explosion_record` / `compressor_record`.
    pub record: Option<OrdnanceRecord>,
    pub tier: Option<u32>,
    /// Index into [`PARTNERS`].
    pub partner: Option<usize>,
    /// Cooperation and funding gains.
    pub gains: [f64; 2],
}

impl Default for Paper {
    fn default() -> Self {
        Self {
            title: String::new(),
            author: String::new(),
            et_alia: false,
            abstract_text: String::new(),
            experiment: None,
            tracked: 0.0,
            record: None,
            tier: None,
            partner: None,
            gains: [0.0; 2],
        }
    }
}

/// `calculate_gains(tier)`: the sigmoid `2g / (1 + e^(-k(x - x0)))`, `k` such that f(0) = 1.
pub fn gain_at(experiment: &OrdnanceExperiment, tier: u32, tracked: f64) -> f64 {
    let i = tier as usize - 1;
    let (Some(&gain), Some(&target)) = (experiment.gain.get(i), experiment.target.get(i)) else {
        return 0.0;
    };
    let steepness = (gain * 2.0 - 1.0).ln() / target;
    gain * 2.0 / (1.0 + (-steepness * (tracked - target)).exp())
}

/// `return_gist` words of a gain.
fn gain_word(gain: f64) -> &'static str {
    let g = gain.round();
    if g <= 0.0 {
        "None"
    } else if g < 25.0 {
        "Little"
    } else if g < 50.0 {
        "Moderate"
    } else if g < 100.0 {
        "Significant"
    } else {
        "Huge"
    }
}

/// A partner boost that can be bought.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Boost {
    Bought,
    /// `SCIPAPER_ALREADY_BOUGHT`: the node is researched, the boost is refunded in points.
    AlreadyBought,
}

impl Paper {
    pub fn experiment_def(&self) -> Option<&'static OrdnanceExperiment> {
        self.experiment.map(|i| &EXPERIMENTS[i])
    }

    /// `calculate_gains(tier)`; 0 without an experiment or score.
    pub fn gains_at(&self, tier: u32) -> f64 {
        match self.experiment_def() {
            Some(e) if self.tracked != 0.0 => gain_at(e, tier, self.tracked),
            _ => 0.0,
        }
    }

    /// `calculate_tier()`: the tiers whose gain is over 10% of the tier's.
    pub fn allowed_tiers(&self) -> Vec<u32> {
        let Some(e) = self.experiment_def().filter(|_| self.tracked != 0.0) else {
            return Vec::new();
        };
        (1..=e.tiers())
            .filter(|&t| self.gains_at(t) > 0.1 * e.gain[t as usize - 1])
            .collect()
    }

    /// `set_experiment(ex_path, variable, data)`: a record of the experiment's kind with a
    /// score, else everything cleared; the tier and the partner reset.
    pub fn set_experiment(
        &mut self,
        experiment: Option<usize>,
        tracked: f64,
        record: Option<OrdnanceRecord>,
    ) {
        let valid = match (experiment.map(|i| &EXPERIMENTS[i]), record) {
            (Some(e), Some(OrdnanceRecord::Explosion(_))) => e.explosive(),
            (Some(e), Some(OrdnanceRecord::Compressor(_))) => !e.explosive(),
            _ => false,
        } && tracked != 0.0
            && tracked.is_finite();
        if valid {
            self.experiment = experiment;
            self.tracked = tracked;
            self.record = record;
        } else {
            self.experiment = None;
            self.tracked = 0.0;
            self.record = None;
        }
        self.set_tier(None);
        self.set_partner(None);
    }

    /// `set_tier`: only an allowed one.
    pub fn set_tier(&mut self, tier: Option<u32>) {
        self.tier = tier.filter(|t| self.allowed_tiers().contains(t));
        self.set_amount();
    }

    /// `set_partner`: only one accepting the experiment.
    pub fn set_partner(&mut self, partner: Option<usize>) {
        self.partner = partner.filter(|&p| {
            self.experiment_def()
                .is_some_and(|e| PARTNERS.get(p).is_some_and(|p| p.accepts(e.path)))
        });
        self.set_amount();
    }

    /// `set_amount()`.
    fn set_amount(&mut self) {
        self.gains = [0.0; 2];
        let Some(tier) = self.tier else {
            return;
        };
        let gain = self.gains_at(tier);
        let (coop, funding) = self.partner.map_or((1.0, 1.0), |p| PARTNERS[p].multipliers);
        self.gains = [gain * coop, gain * funding];
    }

    /// `allowed_to_publish(techweb)`: complete, a record of the right kind not published
    /// before, and the experiment's tier still open.
    pub fn allowed_to_publish(&self, web: &Techweb) -> bool {
        let (Some(e), Some(record), Some(tier), Some(_)) =
            (self.experiment_def(), self.record, self.tier, self.partner)
        else {
            return false;
        };
        if self.gains.contains(&0.0) {
            return false;
        }
        let kind_ok = match record {
            OrdnanceRecord::Explosion(_) => e.explosive(),
            OrdnanceRecord::Compressor(_) => !e.explosive(),
        };
        if !kind_ok {
            return false;
        }
        let duplicate = web
            .published_papers
            .values()
            .flat_map(|tiers| tiers.values())
            .any(|p| p.record == Some(record));
        if duplicate {
            return false;
        }
        !web.published_papers
            .get(e.path)
            .is_some_and(|tiers| tiers.contains_key(&tier))
    }

    /// `autofill()`.
    fn autofill(&mut self, timestamp: &str) {
        if self.title.is_empty() {
            let name = self.experiment_def().map_or("", |e| e.name);
            self.title = format!("On {name} - {}", self.tier.unwrap_or(0));
        }
        if self.author.is_empty() {
            self.author = "Unknown".to_owned();
            self.et_alia = false;
        }
        if self.abstract_text.is_empty() {
            self.abstract_text = format!("Published on {timestamp}");
        }
    }

    /// `return_gist()`.
    pub fn gist(&self) -> PaperGist {
        PaperGist {
            gains: (
                gain_word(self.gains[0]).to_owned(),
                gain_word(self.gains[1]).to_owned(),
            ),
            title: self.title.clone(),
            author: self.author.clone(),
            et_alia: self.et_alia,
            abstract_text: self.abstract_text.clone(),
            experiment_name: self.experiment_def().map(|e| e.name.to_owned()),
            tier: self.tier,
            partner: self.partner.map(|p| PARTNERS[p].name.to_owned()),
        }
    }
}

impl Techweb {
    /// `add_scientific_paper`: publishes the paper (`publish_paper`) and completes its
    /// experiment. Returns the funding in credits for the science budget, `None` when it may
    /// not be published.
    pub fn add_scientific_paper(
        &mut self,
        db: &ResearchDb,
        mut paper: Paper,
        timestamp: &str,
    ) -> Option<i64> {
        if !paper.allowed_to_publish(self) {
            return None;
        }
        paper.autofill(timestamp);
        let e = paper.experiment_def()?;
        let partner = PARTNERS[paper.partner?].path;
        let tier = paper.tier?;
        let [coop, funding] = paper.gains;
        *self
            .scientific_cooperation
            .entry(partner.to_owned())
            .or_default() += coop;
        self.published_papers
            .entry(e.path.to_owned())
            .or_default()
            .insert(tier, paper);
        // "Quickly add and complete it."
        self.add_experiment(db, e.path);
        if self.experiment(e.path).is_some() {
            self.complete_experiment(db, e.path);
        }
        Some((funding * GAIN_TO_MONEY) as i64)
    }

    /// `boost_techweb_node`: the partner discount applies to the node from now on.
    pub fn boost_node(&mut self, db: &ResearchDb, node: usize) {
        self.boosted.insert(node);
        self.unhide_node(db, node);
    }

    /// `allowed_to_boost(techweb, node)` of `partner`.
    pub fn allowed_to_boost(&self, db: &ResearchDb, partner: usize, node: &str) -> Option<Boost> {
        let p = PARTNERS.get(partner)?;
        let amount = p.boost(node)?;
        let &index = db.node_index.get(node)?;
        if self.cooperation(p.path) < amount * PURCHASE_MULTIPLIER {
            return None;
        }
        if self.boosted.contains(&index) && node_boost(node).unwrap_or(0.0) >= amount {
            return None;
        }
        if self.researched.contains(&index) {
            return Some(Boost::AlreadyBought);
        }
        self.available.contains(&index).then_some(Boost::Bought)
    }

    /// `purchase_boost(techweb, node)` of `partner`.
    pub fn purchase_boost(&mut self, db: &ResearchDb, partner: usize, node: &str) -> Option<Boost> {
        let boost = self.allowed_to_boost(db, partner, node)?;
        let p = &PARTNERS[partner];
        let amount = p.boost(node)?;
        let index = db.node_index[node];
        self.boost_node(db, index);
        *self
            .scientific_cooperation
            .entry(p.path.to_owned())
            .or_default() -= amount * PURCHASE_MULTIPLIER;
        if boost == Boost::AlreadyBought {
            self.adjust_points(db, POINT_GENERIC, amount);
        }
        Some(boost)
    }

    pub fn cooperation(&self, partner: &str) -> f64 {
        self.scientific_cooperation
            .get(partner)
            .copied()
            .unwrap_or(0.0)
    }
}

// ---------------------------------------------------------------------------------------------
// NT Frontier

/// The program's state on one computer (`paper_to_be`, `current_tab`, `selected_file`).
#[derive(Clone, Debug, PartialEq)]
pub struct Frontier {
    pub paper: Paper,
    pub tab: u8,
    /// The selected file's uid.
    pub selected_file: Option<u32>,
}

impl Default for Frontier {
    fn default() -> Self {
        Self {
            paper: Paper::default(),
            tab: 1,
            selected_file: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PaperAct {
    EtAlia,
    Publish,
    /// `rewrite`: title, author, abstract (each when sent).
    Rewrite(Option<String>, Option<String>, Option<String>),
    ChangeTab(u8),
    SelectFile(u32),
    SelectExperiment(String),
    SelectTier(u32),
    SelectPartner(String),
    PurchaseBoost {
        partner: String,
        node: String,
    },
}

/// `MAX_NAME_LEN`-ish bound of a paper's texts (tg takes any length).
const MAX_PAPER_TEXT: usize = 1024;

/// The program's acts; `None` for other actions.
pub fn parse(action: &str, params: &UiParams) -> Option<Result<PaperAct, UiReject>> {
    let text = |key: &str| {
        params
            .text(key)
            .ok()
            .map(|t| crate::records::console::sanitize(&t, MAX_PAPER_TEXT))
    };
    let number = |key: &str| {
        params
            .number(key)
            .ok()
            .filter(|n| *n >= 0.0 && n.fract() == 0.0 && *n <= f64::from(u32::MAX))
            .map(|n| n as u32)
            .ok_or_else(|| UiReject::BadParams(format!("{key}: expected a whole number")))
    };
    let path = |key: &str| {
        params
            .get(key)
            .map(str::to_owned)
            .ok_or_else(|| UiReject::BadParams(format!("{key}: missing")))
    };
    Some(match action {
        PAPER_ET_ALIA => Ok(PaperAct::EtAlia),
        PAPER_PUBLISH => Ok(PaperAct::Publish),
        PAPER_REWRITE => Ok(PaperAct::Rewrite(
            text("title"),
            text("author"),
            text("abstract"),
        )),
        PAPER_CHANGE_TAB => number("new_tab").and_then(|t| {
            if (1..=4).contains(&t) {
                Ok(PaperAct::ChangeTab(t as u8))
            } else {
                Err(UiReject::BadParams("new_tab: 1..4".into()))
            }
        }),
        PAPER_SELECT_FILE => number("selected_uid").map(PaperAct::SelectFile),
        PAPER_SELECT_EXPERIMENT => path("selected_expath").map(PaperAct::SelectExperiment),
        PAPER_SELECT_TIER => number("selected_tier").map(PaperAct::SelectTier),
        PAPER_SELECT_PARTNER => path("selected_partner").map(PaperAct::SelectPartner),
        PAPER_PURCHASE_BOOST => path("boost_seller").and_then(|partner| {
            path("purchased_boost").map(|node| PaperAct::PurchaseBoost { partner, node })
        }),
        _ => return None,
    })
}

fn disk_files(world: &World, pc: Entity) -> Vec<crate::modpc::files::OrdnanceFile> {
    world
        .get::<crate::modpc::pda::ModularComputer>(pc)
        .and_then(|c| c.disk)
        .and_then(|d| world.get::<ComputerDisk>(d))
        .map(|d| d.data.clone())
        .unwrap_or_default()
}

/// `recheck_file_presence`: a selected file no longer on the disk clears the paper.
pub fn recheck_file(world: &World, pc: Entity, state: &mut Frontier) {
    let Some(uid) = state.selected_file else {
        return;
    };
    if !disk_files(world, pc).iter().any(|f| f.uid == uid) {
        state.selected_file = None;
        state.paper.set_experiment(None, 0.0, None);
    }
}

/// `round(relations / SCIENTIFIC_COOPERATION_PURCHASE_MULTIPLIER)` as a word.
pub fn relation_word(cooperation: f64) -> &'static str {
    let r = (cooperation / PURCHASE_MULTIPLIER).round();
    if r <= 0.0 {
        "Nonexistant"
    } else if r < 2500.0 {
        "Negligible"
    } else if r < 5000.0 {
        "Limited"
    } else if r < 10000.0 {
        "Cordial"
    } else if r < 20000.0 {
        "Partners"
    } else {
        "Devoted"
    }
}

/// `ui_static_data` + `ui_data`.
pub fn screen(world: &World, pc: Entity, state: &Frontier) -> ScipaperData {
    let web = world.get_resource::<Techweb>();
    let db = world.get_resource::<ResearchDb>();
    let node_name = |path: &str| {
        db.and_then(|db| db.node_index.get(path).map(|&i| db.node_name(i).to_owned()))
            .unwrap_or_else(|| path.to_owned())
    };
    let experiments = EXPERIMENTS
        .iter()
        .map(|e| {
            let (prefix, suffix) = if e.explosive() {
                ("Range", "tiles")
            } else {
                ("Gas", "moles")
            };
            OrdnanceExperimentInfo {
                path: e.path.to_owned(),
                name: e.name.to_owned(),
                description: e.description.to_owned(),
                target: e.target.to_vec(),
                prefix: prefix.to_owned(),
                suffix: suffix.to_owned(),
            }
        })
        .collect();
    let visible: Vec<usize> = web.map_or_else(Vec::new, |w| {
        w.available.iter().chain(&w.researched).copied().collect()
    });
    let visible_nodes: Vec<String> = db.map_or_else(Vec::new, |db| {
        visible.iter().map(|&i| db.nodes[i].id.clone()).collect()
    });
    let partners = PARTNERS
        .iter()
        .enumerate()
        .map(|(i, p)| PartnerInfo {
            path: p.path.to_owned(),
            name: p.name.to_owned(),
            flufftext: p.flufftext.to_owned(),
            multipliers: p.multipliers,
            boosted_nodes: p
                .boosts
                .iter()
                .map(|(n, d)| (node_name(n), *d, (*n).to_owned()))
                .collect(),
            accepted_experiments: EXPERIMENTS
                .iter()
                .filter(|e| p.accepts(e.path))
                .map(|e| e.name.to_owned())
                .collect(),
            relation: relation_word(web.map_or(0.0, |w| w.cooperation(p.path))).to_owned(),
            purchaseable: match (web, db) {
                (Some(w), Some(db)) => visible_nodes
                    .iter()
                    .filter(|n| w.allowed_to_boost(db, i, n).is_some())
                    .cloned()
                    .collect(),
                _ => Vec::new(),
            },
        })
        .collect();
    let files = disk_files(world, pc);
    let selected = state
        .selected_file
        .and_then(|uid| files.iter().find(|f| f.uid == uid));
    let exp_list = selected.map_or_else(Vec::new, |f| {
        f.possible
            .iter()
            .filter_map(|(path, _)| experiment_index(path))
            .map(|i| {
                (
                    EXPERIMENTS[i].name.to_owned(),
                    EXPERIMENTS[i].path.to_owned(),
                )
            })
            .collect()
    });
    let allowed_partners = state.paper.experiment_def().map_or_else(Vec::new, |e| {
        PARTNERS
            .iter()
            .filter(|p| p.accepts(e.path))
            .map(|p| (p.name.to_owned(), p.path.to_owned()))
            .collect()
    });
    let published = web.map_or_else(Vec::new, |w| {
        w.published_papers
            .values()
            .flat_map(|tiers| tiers.values())
            .map(Paper::gist)
            .collect()
    });
    ScipaperData {
        current_tab: state.tab,
        has_techweb: web.is_some(),
        file_list: files.iter().map(|f| (f.filename.clone(), f.uid)).collect(),
        exp_list,
        allowed_tiers: state.paper.allowed_tiers(),
        allowed_partners,
        selected_file: selected.map(|f| f.filename.clone()),
        paper: state.paper.gist(),
        published,
        experiments,
        partners,
        visible_nodes,
    }
}

/// What an act asks of the computer: a line said and a sound.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PaperOutcome {
    pub say: Option<String>,
    pub ping: bool,
    pub error: bool,
}

/// `ui_act`, every check at the moment of effect.
pub fn apply(world: &mut World, pc: Entity, state: &mut Frontier, act: PaperAct) -> PaperOutcome {
    recheck_file(world, pc, state);
    let mut out = PaperOutcome::default();
    let has_web = world.contains_resource::<Techweb>() && world.contains_resource::<ResearchDb>();
    match act {
        PaperAct::EtAlia => state.paper.et_alia = !state.paper.et_alia,
        PaperAct::Rewrite(title, author, abstract_text) => {
            if let Some(t) = title {
                state.paper.title = t;
            }
            if let Some(a) = author {
                state.paper.author = a;
            }
            if let Some(a) = abstract_text {
                state.paper.abstract_text = a;
            }
        }
        PaperAct::ChangeTab(t) => state.tab = t,
        PaperAct::SelectFile(uid) => {
            state.paper.set_experiment(None, 0.0, None);
            state.selected_file = disk_files(world, pc)
                .iter()
                .any(|f| f.uid == uid)
                .then_some(uid);
        }
        PaperAct::SelectExperiment(path) => {
            let file = state
                .selected_file
                .and_then(|uid| disk_files(world, pc).into_iter().find(|f| f.uid == uid));
            match file {
                Some(f) => {
                    let tracked = f
                        .possible
                        .iter()
                        .find(|(p, _)| *p == path)
                        .map_or(0.0, |(_, v)| *v);
                    state
                        .paper
                        .set_experiment(experiment_index(&path), tracked, Some(f.record));
                }
                None => state.paper.set_experiment(None, 0.0, None),
            }
        }
        PaperAct::SelectTier(t) => state.paper.set_tier(Some(t)),
        PaperAct::SelectPartner(p) => state.paper.set_partner(partner_index(&p)),
        PaperAct::Publish | PaperAct::PurchaseBoost { .. } if !has_web => out.error = true,
        PaperAct::Publish => {
            let timestamp = crate::records::manifest::round_timestamp(world);
            let funding = world.resource_scope(|world, mut web: Mut<Techweb>| {
                let db = world.resource::<ResearchDb>();
                web.add_scientific_paper(db, state.paper.clone(), &timestamp)
            });
            match funding {
                Some(credits) => {
                    if let Some(mut economy) =
                        world.get_resource_mut::<crate::cargo::economy::Economy>()
                        && let Some(account) =
                            economy.department_mut(crate::cargo::economy::ACCOUNT_SCI)
                    {
                        account.adjust_money(credits);
                    }
                    let title = world
                        .get_resource::<Techweb>()
                        .and_then(|w| {
                            let e = state.paper.experiment_def()?;
                            let tier = state.paper.tier?;
                            w.published_papers.get(e.path)?.get(&tier).cloned()
                        })
                        .map(|p| p.title)
                        .unwrap_or_default();
                    out.say = Some(format!("\"{title}\" has been published!"));
                    out.ping = true;
                    *state = Frontier {
                        tab: state.tab,
                        ..Frontier::default()
                    };
                }
                None => out.error = true,
            }
        }
        PaperAct::PurchaseBoost { partner, node } => {
            let bought = partner_index(&partner).and_then(|p| {
                world.resource_scope(|world, mut web: Mut<Techweb>| {
                    let db = world.resource::<ResearchDb>();
                    web.purchase_boost(db, p, &node).map(|b| (b, p))
                })
            });
            match bought {
                Some((boost, p)) => {
                    let refund = if boost == Boost::AlreadyBought {
                        format!(
                            ", refunding {} points",
                            PARTNERS[p].boost(&node).unwrap_or(0.0)
                        )
                    } else {
                        String::new()
                    };
                    out.say = Some(format!("Purchase successful{refund}."));
                    out.ping = true;
                }
                None => out.error = true,
            }
        }
    }
    out
}

/// The published papers of a techweb, experiment path -> tier -> paper.
pub type PublishedPapers = BTreeMap<String, BTreeMap<u32, Paper>>;

#[cfg(test)]
mod tests {
    use ss13_mapload::research::{ExperimentDef, ExperimentKind, NodeDef, ResearchData};

    use super::*;

    const LOW: &str = "/datum/experiment/ordnance/explosive/lowyieldbomb";
    const PLASMA: &str = "/datum/experiment/ordnance/gaseous/plasma";
    const EXPLOSIVES: &str = "/datum/techweb_node/explosives";

    fn db() -> ResearchDb {
        let node = |id: &str, prereqs: &[&str], cost: f64| NodeDef {
            id: id.into(),
            name: id.into(),
            prerequisites: prereqs.iter().map(|s| (*s).into()).collect(),
            costs: vec![(POINT_GENERIC.into(), cost)],
            ..default()
        };
        ResearchDb::new(ResearchData {
            nodes: vec![
                NodeDef {
                    flags: ss13_mapload::research::node_flags::STARTER,
                    costs: vec![],
                    ..node("base", &[], 0.0)
                },
                NodeDef {
                    required_experiments: vec![LOW.into()],
                    ..node(EXPLOSIVES, &["base"], 200.0)
                },
            ],
            experiments: vec![
                ExperimentDef {
                    id: LOW.into(),
                    name: "Маломощная взрывчатка".into(),
                    kind: ExperimentKind::Unsupported,
                    ..default()
                },
                ExperimentDef {
                    id: PLASMA.into(),
                    name: "Плазменные снаряды".into(),
                    kind: ExperimentKind::Unsupported,
                    ..default()
                },
            ],
            point_types: vec![(POINT_GENERIC.into(), "Gen. Res.".into())],
            ..default()
        })
    }

    #[test]
    fn gains_follow_the_sigmoid_and_tiers_need_ten_percent() {
        let low = &EXPERIMENTS[experiment_index(LOW).unwrap()];
        // f(0) = 1, f(target) = gain, f(inf) -> 2 * gain.
        assert!((gain_at(low, 1, 0.0) - 1.0).abs() < 1e-9);
        assert!((gain_at(low, 1, 5.0) - 10.0).abs() < 1e-9);
        assert!(gain_at(low, 3, 1000.0) > 39.9);
        let mut paper = Paper::default();
        paper.set_experiment(
            experiment_index(LOW),
            5.0,
            Some(OrdnanceRecord::Explosion(1)),
        );
        // Tier 2 at 5 tiles: 15·2/(1+e^(-k·(5-10))) with k = ln 29 / 10 is about 4.5 > 1.5.
        assert_eq!(paper.allowed_tiers(), vec![1, 2, 3]);
        paper.set_experiment(
            experiment_index(LOW),
            1.0,
            Some(OrdnanceRecord::Explosion(1)),
        );
        assert_eq!(
            paper.allowed_tiers(),
            vec![1],
            "a tiny bang only reaches tier 1"
        );
        assert_eq!(gain_word(0.0), "None");
        assert_eq!(gain_word(30.0), "Moderate");
        assert_eq!(gain_word(150.0), "Huge");
    }

    #[test]
    fn a_paper_needs_a_matching_record_tier_and_partner() {
        let mut paper = Paper::default();
        paper.set_experiment(
            experiment_index(PLASMA),
            300.0,
            Some(OrdnanceRecord::Explosion(1)),
        );
        assert_eq!(
            paper.experiment, None,
            "an explosion record is no gas record"
        );
        paper.set_experiment(
            experiment_index(PLASMA),
            300.0,
            Some(OrdnanceRecord::Compressor(7)),
        );
        assert!(paper.experiment.is_some());
        paper.set_partner(partner_index("/datum/scientific_partner/medical"));
        assert_eq!(paper.partner, None, "medicine takes no plasma");
        paper.set_tier(Some(1));
        let mining = partner_index("/datum/scientific_partner/mining");
        paper.set_partner(mining);
        assert_eq!(paper.partner, mining);
        let gain = gain_at(&EXPERIMENTS[experiment_index(PLASMA).unwrap()], 1, 300.0);
        assert!((paper.gains[0] - gain * 0.75).abs() < 1e-9);
        // The ghost writers accept everything.
        assert!(PARTNERS[1].accepts(PLASMA) && PARTNERS[1].accepts(LOW));
    }

    #[test]
    fn publishing_completes_the_experiment_once_per_tier_and_record() {
        let db = db();
        let mut web = Techweb::science(&db);
        let explosives = db.node_index[EXPLOSIVES];
        assert!(
            web.experiment(LOW).is_some(),
            "a reachable node brings its ordnance experiment"
        );
        assert!(!web.have_experiments_for_node(&db, explosives));
        let mut paper = Paper::default();
        paper.set_experiment(
            experiment_index(LOW),
            12.0,
            Some(OrdnanceRecord::Explosion(3)),
        );
        paper.set_tier(Some(2));
        let defense = partner_index("/datum/scientific_partner/defense").unwrap();
        paper.set_partner(Some(defense));
        let funding = web
            .add_scientific_paper(&db, paper.clone(), "00:10:00")
            .unwrap();
        assert!(funding > 0);
        assert!(web.completed_experiments.contains(LOW));
        assert!(web.have_experiments_for_node(&db, explosives));
        let published = &web.published_papers[LOW][&2];
        assert_eq!(published.title, "On Маломощная взрывчатка - 2");
        assert_eq!(published.author, "Unknown");
        assert!(web.cooperation(PARTNERS[defense].path) > 0.0);
        // The same record again, even on another tier: refused.
        paper.set_tier(Some(3));
        paper.set_partner(Some(defense));
        assert_eq!(web.add_scientific_paper(&db, paper.clone(), "x"), None);
        // Another record on the published tier: refused.
        paper.set_experiment(
            experiment_index(LOW),
            12.0,
            Some(OrdnanceRecord::Explosion(4)),
        );
        paper.set_tier(Some(2));
        paper.set_partner(Some(defense));
        assert_eq!(web.add_scientific_paper(&db, paper.clone(), "x"), None);
        paper.set_tier(Some(3));
        paper.set_partner(Some(defense));
        assert!(web.add_scientific_paper(&db, paper, "x").is_some());
    }

    #[test]
    fn cooperation_buys_partner_boosts() {
        let db = db();
        let mut web = Techweb::science(&db);
        let defense = partner_index("/datum/scientific_partner/defense").unwrap();
        let explosives = db.node_index[EXPLOSIVES];
        assert_eq!(
            web.allowed_to_boost(&db, defense, EXPLOSIVES),
            None,
            "no cooperation"
        );
        web.scientific_cooperation
            .insert(PARTNERS[defense].path.to_owned(), 2.0);
        assert_eq!(
            web.purchase_boost(&db, defense, EXPLOSIVES),
            Some(Boost::Bought)
        );
        assert!((web.cooperation(PARTNERS[defense].path) - 0.8).abs() < 1e-9);
        // 200 less the 120 boost.
        assert_eq!(
            web.price(&db, explosives),
            vec![(POINT_GENERIC.into(), 80.0)]
        );
        assert_eq!(
            web.allowed_to_boost(&db, defense, EXPLOSIVES),
            None,
            "bought once"
        );
        assert_eq!(relation_word(0.0), "Nonexistant");
        assert_eq!(relation_word(30.0), "Limited");
        assert_eq!(relation_word(300.0), "Devoted");
    }

    #[test]
    fn frontier_acts_parse_hostile_params() {
        let p = |params: &[(&str, &str)]| -> Vec<(String, String)> {
            params
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect()
        };
        let tab = p(&[("new_tab", "9")]);
        assert!(matches!(
            parse(PAPER_CHANGE_TAB, &UiParams::new(&tab)),
            Some(Err(_))
        ));
        let tier = p(&[("selected_tier", "-1")]);
        assert!(matches!(
            parse(PAPER_SELECT_TIER, &UiParams::new(&tier)),
            Some(Err(_))
        ));
        let title = p(&[("title", "<b>Bomb</b>\u{7}")]);
        assert_eq!(
            parse(PAPER_REWRITE, &UiParams::new(&title)),
            Some(Ok(PaperAct::Rewrite(Some("bBomb/b".into()), None, None)))
        );
        assert_eq!(parse("other", &UiParams::new(&[])), None);
    }
}
