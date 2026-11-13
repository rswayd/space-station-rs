//! The spies' bounties after tg `/datum/spy_bounty_handler` and `/datum/spy_bounty`
//! (`code/modules/antagonists/spy/spy_bounty*.dm`).
//!
//! One handler serves every spy ([`SpyBounties`], made with the first spy uplink). Every
//! `SPY_BOUNTY_REFRESH_TIMER` (12 minutes) it throws the bounties away and rolls new ones by
//! difficulty (4 easy, 2 medium, 2 hard by BandaStation's `game_options.txt`); the hard ones come
//! one per refresh, the rest are medium until then. A bounty type is picked by weight and tries
//! to find a target in the world; a failed try costs an attempt (`clamp(4 × amount, 10, 25)`).
//! Each bounty rewards an uplink item sold to spies (`UPLINK_SPY`) from the loot pool of its
//! difficulty by the item's cost (≤ 5 TC easy, 5-15 medium, ≥ 15 hard), stepping down when a
//! pool is empty, telecrystals when all are. A rewarded item leaves its pool (80%); an item
//! with ammunition keeps that ammunition out of the pools (90%) until it is rewarded.
//!
//! Bounty types: a steal objective target (`objective_item`, by its `difficulty`), a machine of a
//! random list in a station area (`machine/random/*`, the supermatter's emitter, the AI
//! satellite's teleporter), a crew member's ID card or PDA (`targets_person/some_item/id`,
//! `pda`: the item itself, or its owner scanned while incapacitated), a named station bot
//! (`some_bot`). Recently stolen keys are not rolled again (`check_dupe`).
// shortcut: the config entries are BandaStation's values as constants; the person bounties for
// a limb or organ and for the family heirloom are not rolled (no organ theft, no heirloom quirk);
// tg's abstract bounty types that always fail their roll (`targets_person`, `some_item`) are
// left out of the weights.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use ss13_mapload::antag::{StealItemDef, UPLINK_SPY};
use ss13_mapload::fidelity::is_under;
use ss13_mapload::spawner::Rng;
use ss13_mapload::{Areas, DmItem, DmObj, DmPower, MapSeed};
use ss13_shared::TilePos;

use crate::antag::uplink::UplinkCatalog;
use crate::jobs::PlayerJob;
use crate::server::PlayerMob;

/// `SPY_BOUNTY_REFRESH_TIMER 7200` (deciseconds).
pub const REFRESH_SECS: f64 = 720.0;
/// `SPY_BOUNTY_MAX_EASY`, `SPY_BOUNTY_MAX_MEDIUM`, `SPY_BOUNTY_MAX_HARD`.
pub const MAX_BOUNTIES: [u32; 3] = [4, 2, 2];
/// `SPY_BOUNTY_HARD_BOUNTIES_PER_REFRESH`.
pub const HARD_BOUNTIES_PER_REFRESH: f32 = 1.0;
/// `SPY_EASY_REWARD_TC_THRESHOLD`, `SPY_HARD_REWARD_TC_THRESHOLD`.
pub const EASY_REWARD_TC: u32 = 5;
pub const HARD_REWARD_TC: u32 = 15;
/// The reward when every pool is empty.
pub const TELECRYSTAL_ITEM: &str = "/datum/uplink_item/bundles_tc/telecrystal";
/// `ABSTRACT_UPLINK_ITEM`.
const ABSTRACT_ITEM: &str = "/obj/item/loot_table_maker";
/// `show_anyways`: `typesof(/datum/uplink_item/spy_unique/shotgun_ammo)`.
const SHOW_ANYWAYS: &str = "/datum/uplink_item/spy_unique/shotgun_ammo";
/// `blacklisted_item_types` of the objective item bounties.
const BLACKLISTED_STEAL: [&str; 2] = [
    "/datum/objective_item/steal/functionalai",
    "/datum/objective_item/steal/nukedisc",
];
const MAINTENANCE: &str = "/area/station/maintenance";
/// `VALID_TERRITORY`.
const VALID_TERRITORY: u32 = 1 << 0;
const ENGINEERING: &str = "/datum/job_department/engineering";
const MEDICAL: &str = "/datum/job_department/medical";
const ID_CARD: &str = "/obj/item/card/id/advanced";
const PDA: &str = "/obj/item/modular_computer/pda";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
}

impl Difficulty {
    pub const ALL: [Self; 3] = [Self::Easy, Self::Medium, Self::Hard];

    /// `SPY_DIFFICULTY_*`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Easy => "Easy",
            Self::Medium => "Medium",
            Self::Hard => "Hard",
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// What a person bounty wants from its target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PersonItem {
    /// `some_item/id`: an advanced ID card registered to the target.
    Id,
    /// `some_item/pda`: a PDA imprinted with the target's name.
    Pda,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BountyKind {
    ObjectiveItem(StealItemDef),
    /// `target_type` in the area `area_path`, or one of `originals` (the machines of that area
    /// when the bounty was rolled, wherever they went).
    Machine {
        target_type: &'static str,
        area_path: String,
        originals: Vec<Entity>,
    },
    /// `original` is the target's item when the bounty was rolled.
    Person {
        target: Entity,
        item: PersonItem,
        original: Entity,
    },
    Bot {
        bot: Entity,
        bot_type: &'static str,
    },
}

/// `can_claim`: who the employers think cannot do it.
#[derive(Clone, Debug, PartialEq)]
pub enum Claim {
    Anyone,
    /// `excludefromjob` of the objective item: job titles.
    NotJobs(Vec<String>),
    /// Not a member of this department.
    NotDepartment(&'static str),
    /// Not in command (`DEPARTMENT_BITFLAG_COMMAND`).
    NotCommand,
    /// Not the target itself.
    NotTarget(Entity),
}

/// One `/datum/spy_bounty`.
#[derive(Clone, Debug, PartialEq)]
pub struct Bounty {
    pub id: u32,
    pub name: String,
    pub help: String,
    pub difficulty: Difficulty,
    /// `theft_time`, deciseconds.
    pub theft_time_ds: f32,
    pub black_market_prob: f32,
    pub claimed: bool,
    pub loot_difficulty: Difficulty,
    /// The reward's uplink item path and name.
    pub reward: String,
    pub reward_name: String,
    pub kind: BountyKind,
    pub claim: Claim,
}

impl Bounty {
    /// `can_claim(user)` for a spy of job `job` (index into the jobs data).
    pub fn can_claim(
        &self,
        user: Entity,
        job: Option<usize>,
        jobs: Option<&crate::jobs::Jobs>,
    ) -> bool {
        let job_def = job.and_then(|j| jobs?.data.jobs.get(j));
        match &self.claim {
            Claim::Anyone => true,
            Claim::NotJobs(titles) => job_def.is_none_or(|j| !titles.contains(&j.title)),
            Claim::NotCommand => job_def.is_none_or(|j| !j.command),
            Claim::NotDepartment(dept) => !job.is_some_and(|j| {
                jobs.is_some_and(|jobs| {
                    jobs.data
                        .departments
                        .iter()
                        .any(|d| d.path == *dept && d.jobs.contains(&(j as u16)))
                })
            }),
            Claim::NotTarget(target) => *target != user,
        }
    }

    /// `get_dupe_protection_key`.
    pub fn dupe_key(&self) -> String {
        match &self.kind {
            BountyKind::ObjectiveItem(item) => item.targetitem.clone(),
            BountyKind::Machine { target_type, .. } => (*target_type).to_owned(),
            BountyKind::Person { target, .. } => target.to_bits().to_string(),
            BountyKind::Bot { bot_type, .. } => (*bot_type).to_owned(),
        }
    }
}

/// A random machine bounty's options (`/datum/spy_bounty/machine/random/*`).
const MACHINES_EASY: [&str; 13] = [
    "/obj/machinery/computer/operating",
    "/obj/machinery/computer/order_console/mining",
    "/obj/machinery/computer/records/medical",
    "/obj/machinery/cryo_cell",
    "/obj/machinery/fax",
    "/obj/machinery/hydroponics/constructable",
    "/obj/machinery/medical_kiosk",
    "/obj/machinery/microwave",
    "/obj/machinery/oven",
    "/obj/machinery/recharge_station",
    "/obj/machinery/vending/boozeomat",
    "/obj/machinery/vending/medical",
    "/obj/machinery/vending/wardrobe",
];
const MACHINES_MEDIUM: [&str; 12] = [
    "/obj/machinery/chem_dispenser",
    "/obj/machinery/computer/bank_machine",
    "/obj/machinery/computer/camera_advanced/xenobio",
    "/obj/machinery/computer/cargo",
    "/obj/machinery/computer/crew",
    "/obj/machinery/computer/prisoner/management",
    "/obj/machinery/computer/rdconsole",
    "/obj/machinery/computer/records/security",
    "/obj/machinery/computer/dna_console",
    "/obj/machinery/computer/security",
    "/obj/machinery/dna_scannernew",
    "/obj/machinery/mecha_part_fabricator",
];
const MACHINES_HARD: [&str; 3] = [
    "/obj/machinery/computer/accounting",
    "/obj/machinery/computer/communications",
    "/obj/machinery/modular_computer/preset/id",
];
const MACHINES_AI_SAT: [&str; 2] = [
    "/obj/machinery/teleport",
    "/obj/machinery/computer/teleporter",
];

/// A `/datum/spy_bounty/some_bot` subtype.
#[derive(Debug)]
pub struct BotDef {
    pub bot_type: &'static str,
    pub help: &'static str,
    pub claim: Claim,
}

/// The kinds of bounty with a weight (`bounty_types`).
#[derive(Debug)]
enum Type {
    ObjectiveItem,
    /// Options, the area they must be in, and who may not claim it.
    Machine(&'static [&'static str], Option<&'static str>, Claim),
    Person(PersonItem),
    Bot(BotDef),
}

struct BountyType {
    difficulty: Difficulty,
    weight: u32,
    ty: Type,
}

fn bounty_types() -> Vec<BountyType> {
    use Difficulty::*;
    let t = |difficulty, weight, ty| BountyType {
        difficulty,
        weight,
        ty,
    };
    vec![
        t(Easy, 4, Type::ObjectiveItem),
        t(Medium, 2, Type::ObjectiveItem),
        t(Hard, 1, Type::ObjectiveItem),
        t(Easy, 4, Type::Machine(&MACHINES_EASY, None, Claim::Anyone)),
        t(
            Medium,
            4,
            Type::Machine(&MACHINES_MEDIUM, None, Claim::Anyone),
        ),
        // `machine/engineering_emitter`.
        t(
            Medium,
            1,
            Type::Machine(
                &["/obj/machinery/power/emitter"],
                Some("/area/station/engineering/supermatter"),
                Claim::NotDepartment(ENGINEERING),
            ),
        ),
        t(
            Hard,
            1,
            Type::Machine(&MACHINES_HARD, None, Claim::NotCommand),
        ),
        t(
            Hard,
            1,
            Type::Machine(
                &MACHINES_AI_SAT,
                Some("/area/station/ai/satellite"),
                Claim::NotCommand,
            ),
        ),
        t(Hard, 1, Type::Person(PersonItem::Id)),
        t(Hard, 1, Type::Person(PersonItem::Pda)),
        t(
            Medium,
            1,
            Type::Bot(BotDef {
                bot_type: "/mob/living/basic/bot/secbot/beepsky/officer",
                help: "Похитьте офицера Бипски - обычно встречается при патрулировании станции. \
                       Будьте осторожны, им может не понравиться сканирование.",
                claim: Claim::Anyone,
            }),
        ),
        t(
            Easy,
            1,
            Type::Bot(BotDef {
                bot_type: "/mob/living/basic/bot/secbot/beepsky/ofitser",
                help: "Похитьте тюремного Офитсера - обычно охраняет ГУЛАГ.",
                claim: Claim::Anyone,
            }),
        ),
        t(
            Hard,
            1,
            Type::Bot(BotDef {
                bot_type: "/mob/living/basic/bot/secbot/beepsky/armsky",
                help: "Похитьте Сержанта Армски - обычно охраняет оружейную станции.",
                claim: Claim::Anyone,
            }),
        ),
        t(
            Hard,
            1,
            Type::Bot(BotDef {
                bot_type: "/mob/living/basic/bot/secbot/pingsky",
                help: "Похитьте офицера Пингски - обычно защищает ИИ станции.",
                claim: Claim::Anyone,
            }),
        ),
        t(
            Easy,
            1,
            Type::Bot(BotDef {
                bot_type: "/mob/living/basic/bot/cleanbot/medbay",
                help: "Похитьте доктора Scrubbs - обычно убирает кровь в медотсеке.",
                claim: Claim::NotDepartment(MEDICAL),
            }),
        ),
    ]
}

/// The singleton `/datum/spy_bounty_handler`.
#[derive(Resource)]
pub struct SpyBounties {
    /// Active bounties, easy first.
    pub bounties: Vec<Bounty>,
    /// When the next refresh happens (`refresh_timer`), world seconds.
    pub refresh_at: f64,
    pub num_refreshes: u32,
    /// `all_claimed_bounty_types`: dupe key -> times stolen.
    pub all_claimed: HashMap<String, u32>,
    /// `claimed_bounties_from_last_pool`.
    pub claimed_last_pool: HashSet<String>,
    /// `possible_uplink_items` by difficulty: uplink item paths.
    pub loot: [Vec<String>; 3],
    /// Uplink item path -> (name, `relevant_child_items`).
    items: HashMap<String, (String, Vec<String>)>,
    steal_items: Vec<StealItemDef>,
    pub rng: Rng,
    next_id: u32,
}

impl SpyBounties {
    /// `New()` without the first refresh: the loot pools from the catalog.
    pub fn new(catalog: Option<&UplinkCatalog>, seed: u64) -> Self {
        let mut rng = Rng::new(seed ^ 0x7370_7962_6f75);
        let mut handler = Self {
            bounties: Vec::new(),
            refresh_at: 0.0,
            num_refreshes: 0,
            all_claimed: HashMap::new(),
            claimed_last_pool: HashSet::new(),
            loot: Default::default(),
            items: HashMap::new(),
            steal_items: Vec::new(),
            rng: Rng::new(0),
            next_id: 0,
        };
        let Some(catalog) = catalog else {
            handler.rng = rng;
            return handler;
        };
        handler.steal_items = catalog.data.steal_items.clone();
        for item in &catalog.data.uplink_items {
            handler.items.insert(
                item.path.clone(),
                (item.name.clone(), item.relevant_child_items.clone()),
            );
        }
        let mut limited: HashSet<String> = HashSet::new();
        for item in &catalog.data.uplink_items {
            if limited.contains(&item.path) {
                continue;
            }
            if !handler.try_add_to_loot_pool(catalog, &item.path) {
                continue;
            }
            for child in &item.relevant_child_items {
                if is_under(child, SHOW_ANYWAYS) {
                    continue;
                }
                // "10% chance to have it anyways though."
                if rng.prob(10.0) {
                    continue;
                }
                for pool in &mut handler.loot {
                    pool.retain(|p| p != child);
                }
                limited.insert(child.clone());
            }
        }
        handler.rng = rng;
        handler
    }

    /// `try_add_to_loot_pool`: a spawnable item sold to spies goes into each pool its cost fits.
    pub fn try_add_to_loot_pool(&mut self, catalog: &UplinkCatalog, path: &str) -> bool {
        let Some(item) = catalog.item(path) else {
            return false;
        };
        if item.item == ABSTRACT_ITEM
            || item.purchasable_from & UPLINK_SPY == 0
            || !crate::antag::uplink::can_spawn(catalog, path)
        {
            return false;
        }
        let mut add = |d: Difficulty| {
            let pool = &mut self.loot[d.index()];
            if !pool.iter().any(|p| p == path) {
                pool.push(path.to_owned());
            }
        };
        if item.cost <= EASY_REWARD_TC {
            add(Difficulty::Easy);
        }
        if item.cost >= EASY_REWARD_TC && item.cost <= HARD_REWARD_TC {
            add(Difficulty::Medium);
        }
        if item.cost >= HARD_REWARD_TC {
            add(Difficulty::Hard);
        }
        true
    }

    /// The child items of a rewarded uplink item.
    pub fn children_of(&self, path: &str) -> Vec<String> {
        self.items
            .get(path)
            .map(|(_, c)| c.clone())
            .unwrap_or_default()
    }

    pub fn bounty(&self, id: u32) -> Option<&Bounty> {
        self.bounties.iter().find(|b| b.id == id)
    }

    pub fn bounty_mut(&mut self, id: u32) -> Option<&mut Bounty> {
        self.bounties.iter_mut().find(|b| b.id == id)
    }

    /// `check_dupe`.
    fn check_dupe(&mut self, key: &str, dupe_prob: f32) -> bool {
        if self.claimed_last_pool.contains(key) {
            return true;
        }
        let times = self.all_claimed.get(key).copied().unwrap_or(0);
        self.rng.prob(dupe_prob * times as f32)
    }

    /// `bounties_to_give` of this refresh: the hard ones come one per refresh.
    pub fn bounties_to_give(&self) -> [u32; 3] {
        let [easy, medium, max_hard] = MAX_BOUNTIES;
        let hard =
            ((self.num_refreshes as f32 * HARD_BOUNTIES_PER_REFRESH).floor() as u32).min(max_hard);
        [easy, medium + max_hard.saturating_sub(hard), hard]
    }

    /// `select_reward`: the pool of the difficulty, stepping down while empty; telecrystals when
    /// none has items. The pick usually leaves its pool.
    fn select_reward(&mut self, difficulty: Difficulty) -> (Difficulty, String) {
        let mut tier = difficulty.index();
        while self.loot[tier].is_empty() && tier > 0 {
            tier -= 1;
        }
        let loot_difficulty = Difficulty::ALL[tier];
        if self.loot[tier].is_empty() {
            return (loot_difficulty, TELECRYSTAL_ITEM.to_owned());
        }
        let i = self.rng.range(0, self.loot[tier].len() as i64 - 1) as usize;
        let reward = self.loot[tier][i].clone();
        if self.rng.prob(80.0) {
            self.loot[tier].remove(i);
        }
        (loot_difficulty, reward)
    }

    fn item_name(&self, path: &str) -> String {
        self.items
            .get(path)
            .map(|(n, _)| n.clone())
            .unwrap_or_else(|| path.rsplit('/').next().unwrap_or(path).to_owned())
    }
}

/// `refresh_bounty_list`: every bounty goes (an unclaimed reward returns to its pool at 40%),
/// new ones are rolled for the world as it is, and the next refresh is set.
pub fn refresh(world: &mut World) {
    let now = crate::tools::now(world);
    let Some(mut handler) = world.remove_resource::<SpyBounties>() else {
        return;
    };
    for bounty in std::mem::take(&mut handler.bounties) {
        if !bounty.claimed && handler.rng.prob(40.0) {
            let pool = &mut handler.loot[bounty.loot_difficulty.index()];
            if !pool.contains(&bounty.reward) {
                pool.push(bounty.reward);
            }
        }
    }
    let to_give = handler.bounties_to_give();
    let types = bounty_types();
    let snapshot = Snapshot::of(world);
    for difficulty in Difficulty::ALL {
        let pool: Vec<&BountyType> = types
            .iter()
            .filter(|t| t.difficulty == difficulty && t.weight > 0)
            .collect();
        let weights: Vec<u32> = pool.iter().map(|t| t.weight).collect();
        let mut amount = to_give[difficulty.index()];
        let mut failed = (amount * 4).clamp(10, 25);
        while amount > 0 && failed > 0 {
            let Some(i) = crate::antag::dynamic::pick_weight(&weights, &mut handler.rng) else {
                break;
            };
            match init_bounty(world, &snapshot, &mut handler, pool[i], difficulty) {
                Some(bounty) => {
                    handler.bounties.push(bounty);
                    amount -= 1;
                }
                None => failed -= 1,
            }
        }
    }
    handler.claimed_last_pool.clear();
    handler.num_refreshes += 1;
    handler.refresh_at = now + REFRESH_SECS;
    info!(
        "spy bounties refreshed ({}): {:?}",
        handler.num_refreshes,
        handler.bounties.iter().map(|b| &b.name).collect::<Vec<_>>()
    );
    world.insert_resource(handler);
}

/// What a refresh reads of the world once.
struct Snapshot {
    /// Items on station or mining levels: (entity, type).
    items: Vec<(Entity, String)>,
    /// Machines on station or mining levels: (entity, type, area entity, area type, flags).
    machines: Vec<(Entity, String, Entity, String, u32)>,
    /// Station bots on station or mining levels: (entity, type).
    bots: Vec<(Entity, String)>,
    /// Crew bodies with a player: (entity, name).
    crew: Vec<(Entity, String)>,
}

impl Snapshot {
    fn of(world: &mut World) -> Self {
        let mut q = world.query::<(Entity, &DmItem)>();
        let all_items: Vec<(Entity, String)> =
            q.iter(world).map(|(e, dm)| (e, dm.path.clone())).collect();
        let mut q = world.query::<(Entity, &DmObj, &TilePos)>();
        let objs: Vec<(Entity, String, TilePos)> = q
            .iter(world)
            .filter(|(_, o, _)| o.path.starts_with("/obj/machinery"))
            .map(|(e, o, p)| (e, o.path.clone(), *p))
            .collect();
        let mut q = world
            .query_filtered::<(Entity, &crate::mobs::BasicMob, &TilePos), With<crate::bots::Bot>>();
        let all_bots: Vec<(Entity, String, TilePos)> = q
            .iter(world)
            .map(|(e, b, p)| (e, b.def.path.clone(), *p))
            .collect();
        let mut q = world.query_filtered::<(Entity, &Name), (With<PlayerJob>, With<PlayerMob>)>();
        let crew = q.iter(world).map(|(e, n)| (e, n.to_string())).collect();
        let world: &World = world;
        // `is_station_level(z) || is_mining_level(z)`.
        let levels = |pos: TilePos| {
            crate::comms_server::on_station_level(world, pos)
                || world
                    .get_resource::<crate::modpc::ntnet::Ntnet>()
                    .is_some_and(|n| n.mining == Some(pos.z))
        };
        let items = all_items
            .into_iter()
            .filter(|(e, _)| crate::explosions::turf_of(world, *e).is_some_and(levels))
            .collect();
        let machines = objs
            .into_iter()
            .filter(|(_, _, p)| levels(*p))
            .filter_map(|(e, path, p)| {
                let area = world.get_resource::<Areas>()?.area_at(p)?;
                let power = world.get::<DmPower>(area)?;
                Some((e, path, area, power.path.clone(), power.area_flags as u32))
            })
            .collect();
        let bots = all_bots
            .into_iter()
            .filter(|(_, _, p)| levels(*p))
            .map(|(e, path, _)| (e, path))
            .collect();
        Self {
            items,
            machines,
            bots,
            crew,
        }
    }
}

/// `new picked_bounty(handler)`: `init_bounty`, then `select_reward`. `None` when the type found
/// no target.
fn init_bounty(
    world: &World,
    snapshot: &Snapshot,
    handler: &mut SpyBounties,
    ty: &BountyType,
    difficulty: Difficulty,
) -> Option<Bounty> {
    let (name, help, kind, claim, theft_time_ds, black_market_prob) = match &ty.ty {
        Type::ObjectiveItem => init_objective_item(snapshot, handler, difficulty)?,
        Type::Machine(options, location, claim) => {
            init_machine(world, snapshot, handler, *options, *location, claim.clone())?
        }
        Type::Person(item) => init_person(world, snapshot, handler, *item)?,
        Type::Bot(def) => init_bot(world, snapshot, handler, def)?,
    };
    let (loot_difficulty, reward) = handler.select_reward(difficulty);
    let reward_name = handler.item_name(&reward);
    handler.next_id += 1;
    Some(Bounty {
        id: handler.next_id,
        name,
        help,
        difficulty,
        theft_time_ds,
        black_market_prob,
        claimed: false,
        loot_difficulty,
        reward,
        reward_name,
        kind,
        claim,
    })
}

type Init = (String, String, BountyKind, Claim, f32, f32);

/// `objective_item/init_bounty`.
fn init_objective_item(
    snapshot: &Snapshot,
    handler: &mut SpyBounties,
    difficulty: Difficulty,
) -> Option<Init> {
    let mut valid: Vec<StealItemDef> = Vec::new();
    for item in handler.steal_items.clone() {
        if handler.check_dupe(&item.targetitem, 33.0) {
            continue;
        }
        if !valid_objective_item(snapshot, &item) {
            continue;
        }
        let d = item.difficulty;
        let fits = match difficulty {
            Difficulty::Easy => d < 3,
            Difficulty::Medium => d > 2 && d < 5,
            Difficulty::Hard => d > 3,
        };
        if fits {
            valid.push(item);
        }
    }
    valid.retain(|item| {
        !handler.bounties.iter().any(|b| {
            matches!(&b.kind, BountyKind::ObjectiveItem(existing) if existing.path == item.path)
        })
    });
    if valid.is_empty() {
        return None;
    }
    let item = valid.swap_remove(handler.rng.range(0, valid.len() as i64 - 1) as usize);
    let name = format!(
        "{}Кража {}",
        if difficulty == Difficulty::Hard {
            "Грандиозная "
        } else {
            ""
        },
        item.name
    );
    let help = if item.steal_hint.is_empty() {
        format!("Украдите любой {}.", item.name)
    } else {
        format!("Украдите любой {}: {}", item.name, item.steal_hint)
    };
    let claim = Claim::NotJobs(item.excludefromjob.clone());
    Some((
        name,
        help,
        BountyKind::ObjectiveItem(item),
        claim,
        20.0,
        50.0,
    ))
}

/// `is_valid_objective_item`.
// shortcut: targets with their own completion rules (`special`) are left out, as for the
// traitor's steal objectives; `TRAIT_ITEM_OBJECTIVE_BLOCKED` is not tracked.
fn valid_objective_item(snapshot: &Snapshot, item: &StealItemDef) -> bool {
    if item.special_equipment || item.special || item.difficulty <= 0 || item.difficulty >= 6 {
        return false;
    }
    if BLACKLISTED_STEAL.iter().any(|b| is_under(&item.path, b)) {
        return false;
    }
    !item.exists_on_map
        || snapshot
            .items
            .iter()
            .any(|(_, p)| is_under(p, &item.targetitem))
}

/// `machine/init_bounty` (with `machine/random`'s pick of the type).
fn init_machine(
    world: &World,
    snapshot: &Snapshot,
    handler: &mut SpyBounties,
    options: &'static [&'static str],
    location: Option<&'static str>,
    claim: Claim,
) -> Option<Init> {
    let target_type: &'static str = if options.len() == 1 {
        options[0]
    } else {
        // `machine/random`: not a type another bounty wants, nor a recent dupe.
        let mut left: Vec<&'static str> = options
            .iter()
            .copied()
            .filter(|o| {
                !handler.bounties.iter().any(|b| {
                    matches!(&b.kind, BountyKind::Machine { target_type, .. } if target_type == o)
                })
            })
            .collect();
        let mut kept = Vec::new();
        for o in left.drain(..) {
            if !handler.check_dupe(o, 33.0) {
                kept.push(o);
            }
        }
        if kept.is_empty() {
            return None;
        }
        kept[handler.rng.range(0, kept.len() as i64 - 1) as usize]
    };
    let taken_areas: Vec<&str> = handler
        .bounties
        .iter()
        .filter_map(|b| match &b.kind {
            BountyKind::Machine { area_path, .. } => Some(area_path.as_str()),
            _ => None,
        })
        .collect();
    let mut possible: Vec<&(Entity, String, Entity, String, u32)> = snapshot
        .machines
        .iter()
        .filter(|(_, path, _, area, flags)| {
            is_under(path, target_type)
                && !is_under(area, MAINTENANCE)
                && !taken_areas.contains(&area.as_str())
                && location.is_none_or(|l| is_under(area, l))
                && flags & VALID_TERRITORY != 0
        })
        .collect();
    if possible.is_empty() {
        return None;
    }
    let picked = possible.swap_remove(handler.rng.range(0, possible.len() as i64 - 1) as usize);
    let (machine, _, area, area_path, _) = picked;
    let mut originals = vec![*machine];
    originals.extend(possible.iter().filter(|m| m.2 == *area).map(|m| m.0));
    let machine_name = crate::security::name_of(world, *machine);
    let area_name = crate::shuttle::area_name(world, *area);
    Some((
        format!("Кража {machine_name}"),
        format!("Украдите {machine_name}, находящаяся в {area_name}."),
        BountyKind::Machine {
            target_type,
            area_path: area_path.clone(),
            originals,
        },
        claim,
        100.0,
        50.0,
    ))
}

/// `targets_person/init_bounty` of the ID and PDA bounties.
fn init_person(
    world: &World,
    snapshot: &Snapshot,
    handler: &mut SpyBounties,
    item: PersonItem,
) -> Option<Init> {
    let mut possible: Vec<(Entity, String)> = Vec::new();
    for (mob, name) in &snapshot.crew {
        if handler.check_dupe(&mob.to_bits().to_string(), 50.0) {
            continue;
        }
        if find_desired(world, *mob, name, item).is_none() {
            continue;
        }
        possible.push((*mob, name.clone()));
    }
    possible.retain(|(mob, _)| {
        !handler
            .bounties
            .iter()
            .any(|b| matches!(&b.kind, BountyKind::Person { target, .. } if target == mob))
    });
    if possible.is_empty() {
        return None;
    }
    let (target, real_name) =
        possible.swap_remove(handler.rng.range(0, possible.len() as i64 - 1) as usize);
    let original = find_desired(world, target, &real_name, item)?;
    let thing = crate::security::name_of(world, original);
    let name = match item {
        PersonItem::Id => format!("ID карта {real_name}"),
        PersonItem::Pda => format!("КПК {real_name}"),
    };
    let help = format!(
        "Украдите {thing} у {real_name}. Вы можете сделать это с помощью грубой силы или \
         просканировав их с помощью аплинка, пока они без сознания."
    );
    Some((
        name,
        help,
        BountyKind::Person {
            target,
            item,
            original,
        },
        Claim::NotTarget(target),
        120.0,
        50.0,
    ))
}

/// `find_desired_thing`: the target's own card (registered to its name) or PDA (imprinted
/// with it) among its gear.
// shortcut: the card is matched by its registered name, not the bank account id.
pub fn find_desired(
    world: &World,
    mob: Entity,
    real_name: &str,
    item: PersonItem,
) -> Option<Entity> {
    crate::antag::objectives::all_contents(world, mob)
        .into_iter()
        .find(|&e| match item {
            PersonItem::Id => {
                world
                    .get::<DmItem>(e)
                    .is_some_and(|dm| is_under(&dm.path, ID_CARD))
                    && world
                        .get::<crate::outfit::IdRegistration>(e)
                        .is_some_and(|r| r.registered_name == real_name)
            }
            PersonItem::Pda => world
                .get::<crate::modpc::pda::ModularComputer>(e)
                .is_some_and(|c| {
                    is_under(&c.path, PDA) && c.saved_identification.as_deref() == Some(real_name)
                }),
        })
}

/// The parent type of a path (`parent_type`).
fn parent_of(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(p, _)| p)
}

/// `some_bot/init_bounty`.
fn init_bot(
    world: &World,
    snapshot: &Snapshot,
    handler: &mut SpyBounties,
    def: &BotDef,
) -> Option<Init> {
    // "ensures we don't get two similar bounties."
    if handler.bounties.iter().any(|b| {
        matches!(&b.kind, BountyKind::Bot { bot_type, .. } if is_under(def.bot_type, parent_of(bot_type)))
    }) {
        return None;
    }
    let possible: Vec<Entity> = snapshot
        .bots
        .iter()
        .filter(|(_, p)| is_under(p, def.bot_type))
        .map(|(e, _)| *e)
        .collect();
    if possible.is_empty() {
        return None;
    }
    let bot = possible[handler.rng.range(0, possible.len() as i64 - 1) as usize];
    let name = crate::security::name_of(world, bot);
    Some((
        format!("Похищение {name}"),
        def.help.to_owned(),
        BountyKind::Bot {
            bot,
            bot_type: def.bot_type,
        },
        def.claim.clone(),
        100.0,
        0.0,
    ))
}

/// `is_stealable`: `stealing` satisfies the bounty right now.
pub fn is_stealable(world: &World, bounty: &Bounty, stealing: Entity) -> bool {
    match &bounty.kind {
        // `istype(stealing, desired_item.targetitem)`.
        BountyKind::ObjectiveItem(item) => world
            .get::<DmItem>(stealing)
            .is_some_and(|dm| is_under(&dm.path, &item.targetitem)),
        BountyKind::Machine {
            target_type,
            area_path,
            originals,
        } => {
            world
                .get::<DmObj>(stealing)
                .is_some_and(|o| is_under(&o.path, target_type))
                && (originals.contains(&stealing)
                    || world.get::<TilePos>(stealing).is_some_and(|&p| {
                        world
                            .get_resource::<Areas>()
                            .and_then(|a| a.area_at(p))
                            .and_then(|a| world.get::<DmPower>(a))
                            .is_some_and(|power| is_under(&power.path, area_path))
                    }))
        }
        BountyKind::Person {
            target,
            item,
            original,
        } => {
            if stealing == *original {
                return true;
            }
            if stealing != *target {
                return false;
            }
            // `INCAPACITATED_IGNORING(target, INCAPABLE_RESTRAINTS|INCAPABLE_STASIS)`.
            let incapacitated = world
                .get::<crate::health::Stat>(stealing)
                .is_some_and(|s| *s != crate::health::Stat::Conscious)
                || crate::status_effects::incapacitation(world, stealing).incapacitated;
            let name = world
                .get::<Name>(stealing)
                .map(|n| n.to_string())
                .unwrap_or_default();
            incapacitated && find_desired(world, stealing, &name, *item).is_some()
        }
        BountyKind::Bot { bot, .. } => stealing == *bot,
    }
}

/// Makes the handler with its first refresh when no spy uplink made it yet.
pub fn ensure_handler(world: &mut World) {
    if world.contains_resource::<SpyBounties>() {
        return;
    }
    let seed = world.get_resource::<MapSeed>().map_or(0, |s| s.0);
    let handler = SpyBounties::new(world.get_resource::<UplinkCatalog>(), seed);
    world.insert_resource(handler);
    refresh(world);
}

/// The refresh timer.
pub(super) fn tick_refresh(world: &mut World) {
    let Some(at) = world.get_resource::<SpyBounties>().map(|h| h.refresh_at) else {
        return;
    };
    if crate::tools::now(world) >= at {
        refresh(world);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handler() -> SpyBounties {
        SpyBounties::new(None, 1)
    }

    #[test]
    fn hard_bounties_come_one_per_refresh() {
        let mut h = handler();
        assert_eq!(h.bounties_to_give(), [4, 4, 0]);
        h.num_refreshes = 1;
        assert_eq!(h.bounties_to_give(), [4, 3, 1]);
        h.num_refreshes = 5;
        assert_eq!(h.bounties_to_give(), [4, 2, 2]);
    }

    #[test]
    fn rewards_step_down_to_a_pool_with_items() {
        let mut h = handler();
        h.loot = [vec!["easy".into()], Vec::new(), Vec::new()];
        let (d, reward) = h.select_reward(Difficulty::Hard);
        assert_eq!((d, reward.as_str()), (Difficulty::Easy, "easy"));
        h.loot = Default::default();
        let (_, reward) = h.select_reward(Difficulty::Medium);
        assert_eq!(reward, TELECRYSTAL_ITEM);
    }

    #[test]
    fn recent_dupes_are_not_rolled() {
        let mut h = handler();
        h.claimed_last_pool.insert("/obj/item/a".into());
        assert!(h.check_dupe("/obj/item/a", 0.0));
        assert!(!h.check_dupe("/obj/item/b", 33.0), "never stolen");
        h.all_claimed.insert("/obj/item/c".into(), 4);
        assert!(h.check_dupe("/obj/item/c", 33.0), "132% after four thefts");
    }

    #[test]
    fn targets_are_checked_by_kind() {
        let mut world = World::new();
        let laser = world
            .spawn(DmItem {
                path: "/obj/item/gun/energy/laser/captain".into(),
                ..default()
            })
            .id();
        let toy = world
            .spawn(DmItem {
                path: "/obj/item/toy".into(),
                ..default()
            })
            .id();
        let bot = world.spawn_empty().id();
        let mut bounty = Bounty {
            id: 1,
            name: String::new(),
            help: String::new(),
            difficulty: Difficulty::Hard,
            theft_time_ds: 20.0,
            black_market_prob: 50.0,
            claimed: false,
            loot_difficulty: Difficulty::Hard,
            reward: String::new(),
            reward_name: String::new(),
            kind: BountyKind::ObjectiveItem(StealItemDef {
                targetitem: "/obj/item/gun/energy/laser/captain".into(),
                ..default()
            }),
            claim: Claim::Anyone,
        };
        assert!(is_stealable(&world, &bounty, laser));
        assert!(!is_stealable(&world, &bounty, toy));
        assert_eq!(bounty.dupe_key(), "/obj/item/gun/energy/laser/captain");
        bounty.kind = BountyKind::Bot {
            bot,
            bot_type: "/mob/living/basic/bot/cleanbot/medbay",
        };
        assert!(is_stealable(&world, &bounty, bot));
        assert!(!is_stealable(&world, &bounty, laser));
        // A person bounty takes its original item, or the owner only while incapacitated.
        let owner = world
            .spawn((crate::health::Stat::Conscious, Name::new("Боб")))
            .id();
        bounty.kind = BountyKind::Person {
            target: owner,
            item: PersonItem::Id,
            original: toy,
        };
        assert!(is_stealable(&world, &bounty, toy));
        assert!(!is_stealable(&world, &bounty, owner), "awake");
    }

    #[test]
    fn claims_follow_the_spy_job() {
        let bounty = |claim| Bounty {
            id: 1,
            name: String::new(),
            help: String::new(),
            difficulty: Difficulty::Easy,
            theft_time_ds: 20.0,
            black_market_prob: 50.0,
            claimed: false,
            loot_difficulty: Difficulty::Easy,
            reward: String::new(),
            reward_name: String::new(),
            kind: BountyKind::Bot {
                bot: Entity::PLACEHOLDER,
                bot_type: "",
            },
            claim,
        };
        let mut data = crate::jobs::fallback_jobs();
        let base = data.jobs[0].clone();
        let mut captain = base.clone();
        captain.title = "Captain".into();
        captain.command = true;
        data.jobs.push(captain);
        let captain = data.jobs.len() - 1;
        data.departments.push(ss13_mapload::jobs::Department {
            path: MEDICAL.into(),
            jobs: vec![captain as u16],
            ..Default::default()
        });
        let jobs = crate::jobs::Jobs::new(data, 0);
        let spy = Entity::from_raw_u32(7).unwrap();
        assert!(!bounty(Claim::NotCommand).can_claim(spy, Some(captain), Some(&jobs)));
        assert!(bounty(Claim::NotCommand).can_claim(spy, Some(0), Some(&jobs)));
        assert!(!bounty(Claim::NotDepartment(MEDICAL)).can_claim(spy, Some(captain), Some(&jobs)));
        assert!(bounty(Claim::NotDepartment(MEDICAL)).can_claim(spy, Some(0), Some(&jobs)));
        assert!(!bounty(Claim::NotJobs(vec!["Captain".into()])).can_claim(
            spy,
            Some(captain),
            Some(&jobs)
        ));
        assert!(!bounty(Claim::NotTarget(spy)).can_claim(spy, None, None));
        assert_eq!(
            parent_of("/mob/living/basic/bot/secbot/pingsky"),
            "/mob/living/basic/bot/secbot"
        );
    }
}
