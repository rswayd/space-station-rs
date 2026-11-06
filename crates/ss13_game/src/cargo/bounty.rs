//! Civilian bounties (server), after tg `code/modules/cargo/bounty.dm`, `bounties/item.dm`,
//! `code/game/machinery/civilian_bounty/` and BandaStation's station-wide bounties.
//!
//! - Bounties ([`Bounty`]) are item bounties from `<assets>/cargo.ron` ([`BountyDef`]): a type
//!   matches through the zebra typecache of `wanted_types` ([`wanted`]), a stack counts its
//!   amount. The botany and slime groups get their `New()` here ([`Bounties::make`]).
//! - Per bank account (tg keeps them on `/datum/bank_account`): the current bounty, three to pick
//!   from (`generate_bounty_list`: an assistant one first, then the job's category from the card's
//!   job, `bounty_types`), and the five minute re-roll cooldown shortened by the pad's parts.
//! - The station-wide list (`GLOB.shared_crew_bounties`): `update_list` fills it with one bounty
//!   per job category (20% high priority, x1.5) and adds the specials; every claim counts toward
//!   `civ_bounty_tracker`, and the list grows by one per three claims.
//! - The civilian bounty console ([`CivBountyConsole`], tgui `CivCargoHoldTerminal`): an ID card
//!   used on it goes in; `send` charges the pad for `warmup_time` (3 s), then ships what is on
//!   the pad's tile to the bounties and turns each completed one into a bounty cube on the
//!   console's tile. The pad is the nearest `/obj/machinery/piratepad/civilian` within 4 tiles.
//! - The bounty cube ([`BountyCube`]): sold on the supply shuttle (`/datum/export/bounty_box`,
//!   [`sell_cube`]) for its value plus the speedy delivery bonus (20%); the holders get 30%
//!   (`/datum/component/pricetag`, split by contribution), cargo the rest. Unsent, it nags the
//!   supply channel every 5 minutes (x1.25 each time) and loses the bonus.
// shortcut: reagent, pill, virus and patrol bounties, and item bounties with their own procs
// (toolbox contents, mech scans, fish, atmos canisters, contraband, anomaly cores...) are not
// ported: a category without bounties falls back to the others of its roll, then to the
// assistant ones. No handling tip (export scanner registration), no GPS on cubes, no pad icon
// states or sounds, no multitool linking, no `bounty_voucher`, no civilian bounty beacon, PDAs
// do not lend their ID. The re-roll and bounty lists are not kept across rounds (as in tg).

use std::collections::HashMap;

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::cargo::{BOUNTY_CUBE, BOUNTY_NEW_ROOTS, BountyDef, JobCargoDef, PAPER};
use ss13_mapload::spawner::Rng;
use ss13_mapload::{DmName, DmObj, MapSeed, Turf};
use ss13_net::server::Networked;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, Span};
use ss13_tgui::UiStatus;
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};
use ss13_world::{SpatialIndex, WorldSystems};

use super::economy::{BankAccountId, Economy, bank_card_talk};
use super::export::{ExportReport, Exports};
use super::supply::{CARGO_CRATE_VALUE, SupplyPacks};
use super::*;
use crate::click::{ClickResolved, ResolveClicks};
use crate::inventory::{CarriedBy, Inventory};
use crate::obj_damage::is_under;
use crate::outfit::IdRegistration;
use crate::power::Unpowered;
use crate::radio::MachineRadio;
use crate::storage::StoredIn;

pub const CIV_CONSOLE: &str = "/obj/machinery/computer/piratepad_control/civilian";
pub const CIV_PAD: &str = "/obj/machinery/piratepad/civilian";
/// `BOUNTY_CUT_STANDARD` (`CIV_BOUNTY_SPLIT`): the holders' share.
pub const BOUNTY_CUT_STANDARD: f64 = 0.3;
/// `CIV_BOUNTY_BASELINE`.
pub const CIV_BOUNTY_BASELINE: usize = 5;
/// `HIGH_PRIORITY_BOUNTY_ODDS`.
const HIGH_PRIORITY_ODDS: f32 = 20.0;
/// `COOLDOWN_START(id_account, bounty_timer, 5 MINUTES)`.
pub const BOUNTY_COOLDOWN_SECS: f64 = 300.0;
/// The civilian console's `warmup_time`.
pub const WARMUP_SECS: f64 = 3.0;
/// `locate() in range(4, src)`.
const PAD_RANGE: u32 = 4;
/// `sheet_printer_cooldown`.
const PRINT_COOLDOWN_SECS: f64 = 4.0;
/// The bounty cube's `nag_cooldown`, `nag_cooldown_multiplier`, `speed_bonus`.
pub const NAG_SECS: f64 = 300.0;
const NAG_MULTIPLIER: f64 = 1.25;
pub const SPEED_BONUS: f64 = 0.2;
/// The announcement system's name on the supply channel.
const AAS_NAME: &str = "Automated Announcement System";

/// `CIV_JOB_*` (`code/__DEFINES/economy.dm`).
pub const CIV_JOB_BASIC: u32 = 1;
pub const CIV_JOB_RANDOM: u32 = 15;
/// `MAXIMUM_BOUNTY_JOBS`: the last real category (`CIV_JOB_BITRUN`).
pub const MAXIMUM_BOUNTY_JOBS: u32 = 14;

const ITEM: &str = "/datum/bounty/item";
const SPECIAL: &str = "/datum/bounty/item/special";

/// tg `random_bounty`/`get_random_bounty_type`: per `CIV_JOB_*` (index - 1), the alternatives as
/// (percent, `/datum/bounty/item/<group>`). Missing groups are the not ported bounty kinds.
const CATEGORIES: [&[(f32, &str)]; 14] = [
    &[(100.0, "assistant")],
    &[(100.0, "mech")],
    // + `subtypesof(/datum/bounty/reagent/chef)`.
    &[(100.0, "chef")],
    // 75%: `/datum/bounty/patrol`.
    &[(25.0, "contraband")],
    // Drinks and chemicals: reagent bounties only.
    &[],
    &[],
    // 25%: `subtypesof(/datum/bounty/virus)`.
    &[(75.0, "medical")],
    &[(50.0, "science"), (50.0, "slime")],
    &[(100.0, "engineering")],
    &[(100.0, "mining")],
    &[(100.0, "medical")],
    &[(100.0, "botany")],
    &[(100.0, "atmospherics")],
    &[(100.0, "bitrunning")],
];

/// tg `is_type_in_typecache(path, zebra_typecacheof(wanted_types))`: the last listed type
/// `path` is (or, without subtypes, equals) decides.
pub fn wanted(def: &BountyDef, path: &str) -> bool {
    def.wanted
        .iter()
        .rev()
        .find(|(t, _)| {
            if def.include_subtypes {
                is_under(path, t)
            } else {
                path == t
            }
        })
        .is_some_and(|(_, w)| *w)
}

/// A live `/datum/bounty/item`.
#[derive(Clone, Debug, PartialEq)]
pub struct Bounty {
    /// Index into [`Bounties::defs`].
    pub def: usize,
    pub name: String,
    pub description: String,
    reward: f64,
    pub required_count: u32,
    pub shipped_count: u32,
    pub claimed: bool,
    pub high_priority: bool,
    /// `contribution`: bank account -> items shipped.
    pub contribution: Vec<(u32, f64)>,
}

impl Bounty {
    /// `get_bounty_reward` (`SSeconomy.bounty_modifier` is 1).
    pub fn reward(&self) -> f64 {
        self.reward * if self.high_priority { 1.5 } else { 1.0 }
    }

    /// `applies_to`.
    pub fn applies_to(&self, def: &BountyDef, path: &str) -> bool {
        wanted(def, path) && self.shipped_count < self.required_count
    }

    /// `ship` of a thing `applies_to` took: `amount` is a stack's amount, else 1.
    pub fn ship(&mut self, amount: u32) {
        self.shipped_count += amount.max(1);
    }

    pub fn can_claim(&self) -> bool {
        self.shipped_count >= self.required_count
    }

    /// `print_required`.
    pub fn print_required(&self) -> String {
        format!("{}/{}", self.shipped_count, self.required_count)
    }

    fn contribute(&mut self, account: u32) {
        match self.contribution.iter_mut().find(|(a, _)| *a == account) {
            Some((_, n)) => *n += 1.0,
            None => self.contribution.push((account, 1.0)),
        }
    }
}

/// A bank account's bounties (tg `civilian_bounty`, `bounties`, `bounty_timer`).
#[derive(Clone, Debug, Default)]
pub struct AccountBounties {
    pub current: Option<Bounty>,
    pub choices: Vec<Bounty>,
    /// Server time the re-roll cooldown ends.
    pub ready_at: f64,
}

/// The bounty definitions, the accounts' bounties and the station-wide list.
#[derive(Resource)]
pub struct Bounties {
    pub defs: Vec<BountyDef>,
    pub jobs: Vec<JobCargoDef>,
    pub accounts: HashMap<u32, AccountBounties>,
    /// `GLOB.shared_crew_bounties`, first shown first.
    pub shared: Vec<Bounty>,
    /// `SSeconomy.civ_bounty_tracker`.
    pub civ_bounty_tracker: u32,
    rng: Rng,
}

impl Bounties {
    pub fn new(defs: Vec<BountyDef>, jobs: Vec<JobCargoDef>, seed: u64) -> Self {
        Self {
            defs,
            jobs,
            accounts: HashMap::new(),
            shared: Vec::new(),
            civ_bounty_tracker: 0,
            rng: Rng::new(seed ^ 0x626f_756e),
        }
    }

    /// `subtypesof(/datum/bounty/item/<group>)` for each group.
    fn pool(&self, groups: &[&str]) -> Vec<usize> {
        (0..self.defs.len())
            .filter(|&i| {
                let id = &self.defs[i].id;
                groups.iter().any(|g| {
                    let root = format!("{ITEM}/{g}");
                    *id != root && is_under(id, &root)
                })
            })
            .collect()
    }

    /// `get_random_bounty_type`: a definition of the category. A roll that lands on bounties
    /// not ported here takes the category's others, then the assistant ones.
    pub fn random_type(&mut self, category: u32) -> Option<usize> {
        let category = if (1..=MAXIMUM_BOUNTY_JOBS).contains(&category) {
            category
        } else {
            self.rng.range(1, i64::from(MAXIMUM_BOUNTY_JOBS)) as u32
        };
        let alternatives: Vec<(f32, Vec<usize>)> = CATEGORIES[category as usize - 1]
            .iter()
            .map(|&(p, group)| (p, self.pool(&[group])))
            .filter(|(_, pool)| !pool.is_empty())
            .collect();
        let total: f32 = alternatives.iter().map(|(p, _)| p).sum();
        let pool = if alternatives.is_empty() {
            self.pool(&["assistant"])
        } else {
            let mut roll = (self.rng.next_u64() % 10_000) as f32 / 100.0 * total / 100.0;
            let mut chosen = alternatives.len() - 1;
            for (i, (p, _)) in alternatives.iter().enumerate() {
                if roll < *p {
                    chosen = i;
                    break;
                }
                roll -= p;
            }
            alternatives[chosen].1.clone()
        };
        if pool.is_empty() {
            return None;
        }
        Some(pool[(self.rng.next_u64() % pool.len() as u64) as usize])
    }

    /// `new chosen_type`: the bounty with its group's `New()`.
    pub fn make(&mut self, def: usize) -> Bounty {
        let d = &self.defs[def];
        let mut bounty = Bounty {
            def,
            name: d.name.clone(),
            description: d.description.clone(),
            reward: d.reward,
            required_count: d.required_count,
            shipped_count: 0,
            claimed: false,
            high_priority: false,
            contribution: Vec::new(),
        };
        if is_under(&d.id, BOUNTY_NEW_ROOTS[0]) {
            bounty.description = format!(
                "Шеф повар Центрального Командования хочет приготовить {}, добавив \"{}\". {}",
                d.foodtype, d.name, d.bonus_desc
            );
            bounty.reward += d.multiplier * (CARGO_CRATE_VALUE * 2.0);
            bounty.required_count = self.rng.range(5, 10) as u32;
        } else if is_under(&d.id, BOUNTY_NEW_ROOTS[1]) {
            bounty.description = format!(
                "Научный руководитель Нанотрейзен охотится за редким и экзотическим {}. За его \
                 нахождение полагается награда.",
                d.name
            );
            bounty.reward += self.rng.range(0, 4) as f64 * 500.0;
        }
        bounty
    }

    /// `random_bounty(category)`.
    pub fn random_bounty(&mut self, category: u32) -> Option<Bounty> {
        let def = self.random_type(category)?;
        Some(self.make(def))
    }

    /// `has_duplicate_bounty`.
    fn duplicate(&self, list: &[Bounty], bounty: &Bounty) -> bool {
        list.iter().any(|b| {
            b.def == bounty.def
                && !(self.defs[b.def].allow_duplicate && self.defs[bounty.def].allow_duplicate)
        })
    }

    /// `/datum/id_trim/generate_bounty_list(3, assistant_failsafe = TRUE)`.
    pub fn generate_bounty_list(&mut self, bounty_type: u32) -> Vec<Bounty> {
        let mut list: Vec<Bounty> = self.random_bounty(CIV_JOB_BASIC).into_iter().collect();
        let mut attempts = 20;
        while list.len() < 3 && attempts > 0 {
            let category = if attempts <= 5 {
                CIV_JOB_BASIC
            } else {
                bounty_type
            };
            attempts -= 1;
            let Some(bounty) = self.random_bounty(category) else {
                continue;
            };
            if !self.duplicate(&list, &bounty) {
                list.push(bounty);
            }
        }
        list
    }

    /// `update_global_bounty_list`: claimed ones out, then new ones (one per category, never
    /// `global_exempt`) until `update_up_to` plus the unique ones.
    pub fn update_global_bounty_list(&mut self, mut update_up_to: usize, high_priority: bool) {
        self.shared.retain(|b| !b.claimed);
        update_up_to += self
            .shared
            .iter()
            .filter(|b| self.defs[b.def].unique)
            .count();
        let mut picked: Vec<u32> = Vec::new();
        // tg loops until the list is full; with every category tried it never would.
        while self.shared.len() < update_up_to && picked.len() < MAXIMUM_BOUNTY_JOBS as usize {
            let job = self.rng.range(1, i64::from(MAXIMUM_BOUNTY_JOBS)) as u32;
            if picked.contains(&job) {
                continue;
            }
            picked.push(job);
            let Some(mut bounty) = self.random_bounty(job) else {
                continue;
            };
            if self.defs[bounty.def].global_exempt
                || self.shared.iter().any(|b| b.def == bounty.def)
            {
                continue;
            }
            if high_priority && self.rng.prob(HIGH_PRIORITY_ODDS) {
                bounty.high_priority = true;
                bounty.description.push_str(
                    "\nThis bounty is marked as high priority, and will reward 1.5x the normal \
                     payout!",
                );
            }
            self.shared.insert(0, bounty);
        }
    }

    /// `update_list`: `looped_global_update(1, 5 + bonus)` at once, then the specials.
    pub fn first_global_update(&mut self, players: usize) {
        let bonus = (players / 8).min(5);
        self.update_global_bounty_list(CIV_BOUNTY_BASELINE + bonus, true);
        let specials: Vec<usize> = (0..self.defs.len())
            .filter(|&i| is_under(&self.defs[i].id, SPECIAL) && self.defs[i].id != SPECIAL)
            .collect();
        for def in specials {
            let bounty = self.make(def);
            self.shared.push(bounty);
        }
    }

    /// The job category of a card's job title (`our_job?.bounty_types || CIV_JOB_RANDOM`).
    pub fn bounty_types(&self, job: &str) -> u32 {
        self.jobs
            .iter()
            .find(|j| j.title == job)
            .map(|j| j.bounty_types)
            .filter(|&t| t != 0)
            .unwrap_or(CIV_JOB_RANDOM)
    }

    pub fn account(&self, id: u32) -> Option<&AccountBounties> {
        self.accounts.get(&id)
    }
}

/// The civilian bounty console (`/obj/machinery/computer/piratepad_control/civilian`).
#[derive(Component, Clone, Debug, Default)]
pub struct CivBountyConsole {
    /// `inserted_scan_id`.
    pub inserted: Option<Entity>,
    /// `pad_ref`.
    pub pad: Option<Entity>,
    pub status_report: String,
    pub sending: bool,
    /// When the pad fires, and whether it ships to the station bounties.
    send_at: Option<(f64, bool)>,
    next_print: f64,
}

/// A `/obj/machinery/piratepad/civilian`.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct CivBountyPad;

/// A completed bounty (`/obj/item/bounty_cube`).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct BountyCube {
    pub bounty_value: f64,
    pub speed_bonus: f64,
    pub holder_cut: f64,
    /// `bounty_holder_accounts` with their contributions.
    pub payees: Vec<(u32, f64)>,
    pub next_nag: f64,
    pub nag_cooldown: f64,
}

impl BountyCube {
    /// `/datum/export/bounty_box/get_base_cost`, rounded up as `get_cost`.
    pub fn sale_value(&self) -> i64 {
        (self.bounty_value + self.bounty_value * self.speed_bonus - 1e-9).ceil() as i64
    }

    /// `pricetag/on_parent_sold`: each payee's cut (`round(value * holder_cut * share)`) and
    /// what is left for the report.
    pub fn split(&self, value: i64) -> (Vec<(u32, i64)>, i64) {
        let total: f64 = self.payees.iter().map(|(_, n)| n.max(0.0)).sum();
        let mut left = value;
        let mut cuts = Vec::new();
        for &(account, n) in &self.payees {
            let share = if total > 0.0 { n.max(0.0) / total } else { 1.0 };
            let cut = (value as f64 * self.holder_cut * share).round() as i64;
            left = (left - cut).max(0);
            cuts.push((account, cut));
        }
        (cuts, left)
    }
}

pub struct BountyPlugin;

impl Plugin for BountyPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // tg `item_interaction` of the civilian bounty console: an ID card goes in.
        app.add_click_handler(ClickPhase::HeldOnTarget, 4800, |w, c| {
            c.held.is_some_and(|h| {
                crate::cargo::bounty::item_interaction(w, c.client, c.mob, h, c.target)
            })
        });
        app.register_ui::<CivBountyUi>()
            .add_systems(Startup, load_bounties.after(super::supply::load_packs))
            .add_systems(ss13_world::MapInit, setup_consoles)
            .add_systems(
                Update,
                (
                    open_console
                        .after(WorldSystems::Movement)
                        .after(ResolveClicks)
                        .before(UiSystems::Open),
                    console_acts.in_set(UiSystems::Handle),
                    unpowered_windows.in_set(UiSystems::StatusHook),
                    fire_pads,
                    nag_cubes,
                ),
            );
    }
}

fn load_bounties(world: &mut World) {
    if world.contains_resource::<Bounties>() {
        return; // inserted by a test
    }
    let seed = world.get_resource::<MapSeed>().map_or(0, |s| s.0);
    let path = world
        .get_resource::<ss13_mapload::MapLoadPlugin>()
        .map(|m| m.assets.join(ss13_mapload::cargo::CARGO_FILE));
    let data = path
        .as_deref()
        .and_then(|p| ss13_mapload::cargo::CargoData::read(p).ok())
        .unwrap_or_default();
    info!(
        "cargo: {} civilian bounties, {} jobs' bounty categories",
        data.bounties.len(),
        data.jobs.len()
    );
    world.insert_resource(Bounties::new(data.bounties, data.jobs, seed));
}

fn setup_consoles(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &DmObj), With<ss13_world::Mapped>>();
    let found: Vec<(Entity, bool)> = query
        .iter(world)
        .filter_map(|(e, o)| {
            if is_under(&o.path, CIV_CONSOLE) {
                Some((e, true))
            } else if is_under(&o.path, CIV_PAD) {
                Some((e, false))
            } else {
                None
            }
        })
        .collect();
    for &(e, console) in &found {
        if !console {
            world.entity_mut(e).insert(CivBountyPad);
        }
    }
    for (e, console) in found {
        if console {
            setup_console(world, e);
        }
    }
}

/// A civilian console's state: `post_machine_initialize` links the nearest pad.
pub(crate) fn setup_console(world: &mut World, console: Entity) {
    let pad = find_pad(world, console);
    world.entity_mut(console).insert(CivBountyConsole {
        pad,
        status_report: "Готов к доставке.".into(),
        ..default()
    });
}

fn find_pad(world: &mut World, console: Entity) -> Option<Entity> {
    let &pos = world.get::<TilePos>(console)?;
    let mut pads = world.query_filtered::<(Entity, &TilePos), With<CivBountyPad>>();
    pads.iter(world)
        .filter_map(|(e, p)| Some((p.chebyshev(pos)?, e)))
        .filter(|(d, _)| *d <= PAD_RANGE)
        .min()
        .map(|(_, e)| e)
}

/// An empty hand opens the window (`ui_interact`).
fn open_console(
    mut clicks: MessageReader<ClickResolved>,
    consoles: Query<&DmName, (With<CivBountyConsole>, Without<Unpowered>)>,
    inventories: Query<&Inventory>,
    mut open: MessageWriter<OpenUi>,
) {
    for click in clicks.read() {
        let Ok(name) = consoles.get(click.target) else {
            continue;
        };
        if inventories
            .get(click.mob)
            .is_ok_and(|i| i.active_item().is_none())
        {
            open.write(OpenUi::new::<CivBountyUi>(
                click.client,
                click.mob,
                click.target,
                plain(&name.name),
            ));
        }
    }
}

fn plain(name: &str) -> &str {
    name.trim_start_matches("\\improper ")
        .trim_start_matches("\\proper ")
}

fn unpowered_windows(
    mut windows: Query<&mut UiWindow>,
    unpowered: Query<(), (With<CivBountyConsole>, With<Unpowered>)>,
) {
    for mut window in &mut windows {
        if window.interface == CivBountyUi::NAME && unpowered.contains(window.source) {
            window.cap_status(UiStatus::Update);
        }
    }
}

/// `item_interaction` with an ID card: it goes in (`id_insert`), a card already there comes
/// out, the window opens. `true` when `held` was taken.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<CivBountyConsole>(target).is_none()
        || !world
            .get::<ss13_mapload::DmItem>(held)
            .is_some_and(ss13_mapload::DmItem::is_id_card)
    {
        return false;
    }
    let in_hand = world
        .get::<Inventory>(mob)
        .is_some_and(|i| i.hands.contains(&Some(held)));
    if !in_hand {
        return true;
    }
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        for hand in &mut inventory.hands {
            if *hand == Some(held) {
                *hand = None;
            }
        }
    }
    world.entity_mut(held).remove::<CarriedBy>();
    let old = world
        .get::<CivBountyConsole>(target)
        .and_then(|c| c.inserted);
    crate::storage::put_inside(world, held, target);
    if let Some(old) = old {
        eject(world, Some(mob), target, old);
    }
    if let Some(mut console) = world.get_mut::<CivBountyConsole>(target) {
        console.inserted = Some(held);
    }
    let (card, console) = (name_of(world, held), name_of(world, target));
    crate::tools::tell(
        world,
        client,
        Span::Notice,
        format!("Вы вставляете {card} в {console}."),
    );
    if world.get::<Unpowered>(target).is_none() {
        world.write_message(OpenUi::new::<CivBountyUi>(
            client,
            mob,
            target,
            plain(&console),
        ));
    }
    true
}

fn name_of(world: &World, e: Entity) -> String {
    world
        .get::<DmName>(e)
        .map(|n| plain(&n.name).to_owned())
        .unwrap_or_default()
}

/// `id_eject`: the card into a free hand of `user` next to the console, else onto its tile.
fn eject(world: &mut World, user: Option<Entity>, console: Entity, card: Entity) {
    if world.get::<StoredIn>(card).is_none_or(|s| s.0 != console) {
        return;
    }
    let pos = world.get::<TilePos>(console).copied();
    let hand = user.and_then(|u| {
        let near = matches!((pos, world.get::<TilePos>(u)), (Some(a), Some(b)) if a.chebyshev(*b).is_some_and(|d| d <= 1));
        let inventory = world.get::<Inventory>(u).filter(|_| near)?;
        [inventory.active, 1 - inventory.active]
            .into_iter()
            .find(|&h| inventory.hands[h].is_none())
            .map(|h| (u, h))
    });
    match (hand, pos) {
        (Some((mob, h)), _) => {
            world
                .entity_mut(card)
                .remove::<StoredIn>()
                .insert(CarriedBy(mob));
            if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
                inventory.hands[h] = Some(card);
            }
        }
        (None, Some(pos)) => crate::storage::take_out(world, card, pos),
        (None, None) => return,
    }
    if let Some(mut state) = world.get_mut::<CivBountyConsole>(console)
        && state.inserted == Some(card)
    {
        state.inserted = None;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CivBountyAct {
    Recalc,
    Send {
        global: bool,
    },
    Stop,
    /// 1-based.
    Pick(usize),
    NewBounties,
    Eject,
    UpdateList,
    Print,
}

type ConsoleParams = (
    Option<Res<'static, Bounties>>,
    Query<'static, 'static, &'static BankAccountId>,
    Query<'static, 'static, (), With<CivBountyPad>>,
);

/// `ui_data`.
pub fn console_data(
    console: &CivBountyConsole,
    account: Option<u32>,
    pad: bool,
    bounties: Option<&Bounties>,
) -> CivBountyData {
    let mut data = CivBountyData {
        pad,
        sending: console.sending,
        status_report: console.status_report.clone(),
        id_inserted: console.inserted.is_some(),
        id_bounty_num: "N/A".into(),
        ..default()
    };
    let Some(bounties) = bounties else {
        return data;
    };
    data.claimed_bounties = bounties.civ_bounty_tracker;
    if let Some(state) = account.and_then(|a| bounties.account(a)) {
        if let Some(current) = &state.current {
            data.id_bounty_info = Some(current.description.clone());
            data.id_bounty_num = current.print_required();
            data.id_bounty_value = current.reward() * BOUNTY_CUT_STANDARD;
        }
        data.picking = !state.choices.is_empty();
        data.choices = state
            .choices
            .iter()
            .map(|b| BountyChoice {
                name: b.name.clone(),
                info: b.description.clone(),
                value: b.reward() * BOUNTY_CUT_STANDARD,
            })
            .collect();
    }
    data.list_bounty = bounties
        .shared
        .iter()
        .map(|b| SharedBounty {
            name: b.name.clone(),
            description: b.description.clone(),
            reward: b.reward(),
            shipped: b.shipped_count,
            maximum: b.required_count,
            claimed: b.claimed,
            priority: b.high_priority,
            unique: bounties.defs[b.def].unique,
        })
        .collect();
    data
}

impl ServerUi for CivBountyUi {
    type Source = &'static CivBountyConsole;
    type UserParam = ConsoleParams;
    type Act = CivBountyAct;

    fn static_data(_: &CivBountyConsole) {}

    fn data(
        console: &CivBountyConsole,
        _: Entity,
        (bounties, accounts, pads): &SystemParamItem<'_, '_, ConsoleParams>,
    ) -> CivBountyData {
        let account = console
            .inserted
            .and_then(|c| accounts.get(c).ok())
            .map(|a| a.0);
        let pad = console.pad.is_some_and(|p| pads.contains(p));
        console_data(console, account, pad, bounties.as_deref())
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<CivBountyAct, UiReject> {
        Ok(match action {
            RECALC => CivBountyAct::Recalc,
            SEND => CivBountyAct::Send {
                global: params
                    .get(GLOBAL)
                    .is_some_and(|v| !matches!(v, "" | "0" | "false")),
            },
            STOP => CivBountyAct::Stop,
            PICK => {
                let n = params.number(VALUE)?;
                if !(1.0..=3.0).contains(&n) || n.fract() != 0.0 {
                    return Err(UiReject::BadParams("value: expected 1..3".into()));
                }
                CivBountyAct::Pick(n as usize)
            }
            BOUNTY => CivBountyAct::NewBounties,
            EJECT => CivBountyAct::Eject,
            UPDATE_LIST => CivBountyAct::UpdateList,
            PRINT => CivBountyAct::Print,
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

/// The inserted card if it is still in the console, with its account.
fn inserted(world: &World, console: Entity) -> Option<(Entity, Option<u32>)> {
    let card = world.get::<CivBountyConsole>(console)?.inserted?;
    if world.get::<StoredIn>(card).is_none_or(|s| s.0 != console) {
        return None;
    }
    Some((card, world.get::<BankAccountId>(card).map(|a| a.0)))
}

fn set_status(world: &mut World, console: Entity, text: impl Into<String>) {
    if let Some(mut state) = world.get_mut::<CivBountyConsole>(console) {
        state.status_report = text.into();
    }
}

/// `ui_act`, one exclusive pass; every check runs here.
fn console_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<CivBountyUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, CivBountyAct)> = acts
        .read(world.resource::<Messages<UiActed<CivBountyUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    let now = world.resource::<Time>().elapsed_secs_f64();
    for (client, actor, console, act) in pending {
        if world.get::<Unpowered>(console).is_some() || !world.contains_resource::<Bounties>() {
            continue;
        }
        let Some(state) = world.get::<CivBountyConsole>(console).cloned() else {
            continue;
        };
        // A pad gone since map load is looked up again.
        let pad = state
            .pad
            .filter(|&p| world.get::<CivBountyPad>(p).is_some())
            .or_else(|| find_pad(world, console));
        if let Some(mut s) = world.get_mut::<CivBountyConsole>(console) {
            s.pad = pad;
        }
        // tg: `if(!pad) return` before every act.
        let Some(pad) = pad else {
            continue;
        };
        let card = inserted(world, console);
        if card.is_none()
            && let Some(mut s) = world.get_mut::<CivBountyConsole>(console)
        {
            s.inserted = None;
        }
        match act {
            CivBountyAct::Recalc => recalc(world, console, pad, card),
            CivBountyAct::Send { global } => start_sending(world, console, pad, global, now),
            CivBountyAct::Stop => stop_sending(world, console, None),
            CivBountyAct::Pick(n) => {
                let Some(account) = card.and_then(|(_, a)| a) else {
                    continue;
                };
                let mut bounties = world.resource_mut::<Bounties>();
                let state = bounties.accounts.entry(account).or_default();
                if n <= state.choices.len() {
                    let choice = state.choices.swap_remove(n - 1);
                    state.current = Some(choice);
                    state.choices.clear();
                }
            }
            CivBountyAct::NewBounties => add_bounties(world, client, console, pad, card, now),
            CivBountyAct::Eject => {
                if let Some((card, _)) = card {
                    eject(world, Some(actor), console, card);
                } else {
                    crate::tools::tell(world, client, Span::Warning, "Этот слот пуст!");
                }
            }
            CivBountyAct::UpdateList => {
                // tg rolls (and adds the specials again) on every press; the button is only
                // shown while the list is empty.
                if world.resource::<Bounties>().shared.is_empty() {
                    let players = world.resource::<crate::server::Players>().len();
                    world
                        .resource_mut::<Bounties>()
                        .first_global_update(players);
                }
            }
            CivBountyAct::Print => {
                if now >= state.next_print {
                    print_sheet(world, console);
                    if let Some(mut s) = world.get_mut::<CivBountyConsole>(console) {
                        s.next_print = now + PRINT_COOLDOWN_SECS;
                    }
                } else {
                    crate::tools::tell(world, client, Span::Warning, "printer spooling!");
                }
            }
        }
    }
}

/// What lies on the pad's tile that may be shipped: not the pad or the floor, not abstract.
fn shippable(world: &World, pad: Entity) -> Vec<(Entity, String, u32)> {
    let Some(&pos) = world.get::<TilePos>(pad) else {
        return Vec::new();
    };
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| e != pad && world.get::<Turf>(e).is_none())
        .filter_map(|e| {
            let path = super::export::path_of(world, e)?;
            let amount = world
                .get::<crate::tools::Stack>(e)
                .map(|s| s.amount)
                .or_else(|| world.get::<ss13_mapload::DmStack>(e).map(|s| s.amount))
                .unwrap_or(1);
            Some((e, path, amount))
        })
        .collect()
}

/// `recalc`.
fn recalc(world: &mut World, console: Entity, pad: Entity, card: Option<(Entity, Option<u32>)>) {
    if world
        .get::<CivBountyConsole>(console)
        .is_none_or(|c| c.sending)
    {
        return;
    }
    let Some((_, account)) = card else {
        set_status(world, console, "Пожалуйста, вставьте сначала ID карту.");
        return;
    };
    let bounties = world.resource::<Bounties>();
    let Some(current) = account
        .and_then(|a| bounties.account(a))
        .and_then(|s| s.current.as_ref())
    else {
        set_status(world, console, "Пожалуйста, возьмите сначала новый заказ.");
        return;
    };
    let def = &bounties.defs[current.def];
    let applies = shippable(world, pad)
        .iter()
        .any(|(_, path, _)| current.applies_to(def, path));
    let text = if applies {
        "Гражданский заказ: Цель применима."
    } else {
        "Гражданский заказ: Цель неприменима."
    };
    set_status(world, console, text);
}

/// `start_sending`.
fn start_sending(world: &mut World, console: Entity, pad: Entity, global: bool, now: f64) {
    let Some(mut state) = world.get_mut::<CivBountyConsole>(console) else {
        return;
    };
    if state.sending {
        return;
    }
    state.sending = true;
    state.status_report = "Отправка... ".into();
    state.send_at = Some((now + WARMUP_SECS, global));
    pad_message(world, pad, "начинает заряжаться.");
}

/// `stop_sending`.
fn stop_sending(world: &mut World, console: Entity, report: Option<&str>) {
    let Some(mut state) = world.get_mut::<CivBountyConsole>(console) else {
        return;
    };
    if !state.sending {
        return;
    }
    state.sending = false;
    state.send_at = None;
    state.status_report = report.unwrap_or("Готов к доставке.").to_owned();
}

fn pad_message(world: &mut World, pad: Entity, what: &str) {
    let Some(&pos) = world.get::<TilePos>(pad) else {
        return;
    };
    let name = name_of(world, pad);
    let mut chars = name.chars();
    let name: String = chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default();
    let chat = crate::pull::visible_message(
        world,
        pos,
        ChatChannel::Local,
        (Span::Notice, format!("{name} {what}")),
        &[],
    );
    for message in chat {
        world.write_message(message);
    }
}

/// The consoles whose warmup ended ship (`sending_timer`).
fn fire_pads(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut query = world.query::<(Entity, &CivBountyConsole)>();
    let due: Vec<(Entity, bool)> = query
        .iter(world)
        .filter_map(|(e, c)| {
            let (at, global) = c.send_at?;
            (now >= at).then_some((e, global))
        })
        .collect();
    for (console, global) in due {
        if let Some(mut state) = world.get_mut::<CivBountyConsole>(console) {
            state.send_at = None;
        }
        send(world, console, global);
    }
}

/// `civilian/send`: what is on the pad goes to the bounties; completed ones become cubes.
pub fn send(world: &mut World, console: Entity, global: bool) {
    set_status(world, console, "");
    let sending = world
        .get::<CivBountyConsole>(console)
        .is_some_and(|c| c.sending);
    let pad = world
        .get::<CivBountyConsole>(console)
        .and_then(|c| c.pad)
        .filter(|&p| world.get::<CivBountyPad>(p).is_some());
    let (Some(pad), true) = (pad, sending) else {
        stop_sending(world, console, None);
        return;
    };
    let card = inserted(world, console);
    let Some((card, Some(account))) = card else {
        stop_sending(world, console, None);
        return;
    };
    let empty = {
        let bounties = world.resource::<Bounties>();
        if global {
            bounties.shared.is_empty()
        } else {
            bounties
                .account(account)
                .and_then(|s| s.current.as_ref())
                .is_none()
        }
    };
    if empty {
        stop_sending(world, console, None);
        return;
    }
    let things = shippable(world, pad);
    let mut shipped: Vec<Entity> = Vec::new();
    let completed: Vec<Bounty> = {
        let mut res = world.resource_mut::<Bounties>();
        let bounties = &mut *res;
        let defs = &bounties.defs;
        // The bounties checked: the station list, or the card's own.
        let mut stack: Vec<&mut Bounty> = if global {
            bounties.shared.iter_mut().collect()
        } else {
            bounties
                .accounts
                .get_mut(&account)
                .and_then(|s| s.current.as_mut())
                .into_iter()
                .collect()
        };
        for (thing, path, amount) in things {
            // tg ships one thing to every bounty it applies to; it is deleted after the first.
            if let Some(bounty) = stack.iter_mut().find(|b| b.applies_to(&defs[b.def], &path)) {
                bounty.contribute(account);
                bounty.ship(amount);
                shipped.push(thing);
            }
        }
        stack
            .iter_mut()
            .filter(|b| b.can_claim() && !b.claimed)
            .map(|b| {
                b.claimed = true;
                (**b).clone()
            })
            .collect()
    };
    for thing in &shipped {
        if let Ok(e) = world.get_entity_mut(*thing) {
            e.despawn();
        }
    }
    let mut report = if shipped.is_empty() {
        "Цель заказа не обнаружена. Отмена. ".to_owned()
    } else {
        format!("Цель заказа найдена x{}. ", shipped.len())
    };
    let mut talk: Vec<(u32, String)> = Vec::new();
    {
        let mut bounties = world.resource_mut::<Bounties>();
        for bounty in &completed {
            if global {
                bounties.civ_bounty_tracker += 1;
                for &(helper, _) in &bounty.contribution {
                    let Some(state) = bounties.accounts.get_mut(&helper) else {
                        continue;
                    };
                    if state.current.as_ref().is_some_and(|b| b.def == bounty.def) {
                        state.current = None;
                        state.ready_at = 0.0;
                        talk.push((
                            helper,
                            format!(
                                "Your {} bounty has been completed for matching the completed \
                                 station bounty!",
                                bounty.name
                            ),
                        ));
                    }
                }
            } else if let Some(state) = bounties.accounts.get_mut(&account) {
                // `reset_bounty`.
                state.current = None;
                state.ready_at = 0.0;
            }
        }
        if global {
            let up_to = CIV_BOUNTY_BASELINE + (bounties.civ_bounty_tracker / 3) as usize;
            bounties.update_global_bounty_list(up_to, false);
        }
    }
    for (account, text) in talk {
        bank_card_talk(world, account, &text);
    }
    for bounty in &completed {
        make_cube(world, console, card, bounty);
    }
    let n = completed.len();
    if n > 0 {
        let ending = match n % 100 {
            11..=14 => "ов",
            _ => match n % 10 {
                1 => "",
                2..=4 => "а",
                _ => "ов",
            },
        };
        let done = if n % 10 == 1 && n % 100 != 11 {
            ""
        } else {
            "о"
        };
        let (your, cube) = if n > 1 { ("и", "ы") } else { ("ю", "у") };
        report.push_str(&format!(
            "x{n} заказ{ending} выполнен{done}! Пожалуйста, передайте сво{your} \
             куб-наград{cube} в отдел снабжения для перевода средств на ваш счёт. "
        ));
    }
    if let Some(mut state) = world.get_mut::<CivBountyConsole>(console) {
        state.status_report = report;
        state.sending = false;
    }
    pad_message(world, pad, "активируется!");
}

/// `new /obj/item/bounty_cube(drop_location())` + `set_up`.
fn make_cube(world: &mut World, console: Entity, card: Entity, bounty: &Bounty) {
    let Some(&pos) = world.get::<TilePos>(console) else {
        return;
    };
    let Some(cube) = world
        .try_resource_scope(|world, packs: Mut<SupplyPacks>| {
            Some(packs.templates.get(BOUNTY_CUBE)?.spawn(world, Some(pos)))
        })
        .flatten()
    else {
        warn!("no {BOUNTY_CUBE} prototype (rerun `ss13-mapload convert`)");
        return;
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    let value = bounty.reward();
    let registration = world.get::<IdRegistration>(card).cloned();
    let (holder, job) = registration.map_or_else(
        || ("Unknown".to_owned(), "Unknown".to_owned()),
        |r| (r.registered_name, r.assignment),
    );
    let (base, desc) = world
        .get::<DmName>(cube)
        .map(|n| (plain(&n.name).to_owned(), n.desc.clone()))
        .unwrap_or_default();
    let value_text = super::economy::dm_number(value);
    world.entity_mut(cube).insert((
        Networked,
        DmName {
            name: format!("{value_text} {MONEY_SYMBOL} {base}"),
            desc: format!(
                "{desc} Торговая бирка показывает, что этот куб-награда для {holder} ({job}) за \
                 выполнение заказа \"{}\" .",
                bounty.name
            ),
        },
        BountyCube {
            bounty_value: value,
            speed_bonus: SPEED_BONUS,
            holder_cut: BOUNTY_CUT_STANDARD,
            payees: bounty.contribution.clone(),
            next_nag: now + NAG_SECS,
            nag_cooldown: NAG_SECS,
        },
    ));
    crate::storage::finish_item(world, cube, 0);
    let area = area_name(world, pos);
    let minutes = (NAG_SECS / 60.0) as u64;
    world.write_message(MachineRadio {
        freq: ss13_chat::radio::FREQ_SUPPLY,
        z: pos.z,
        name: AAS_NAME.into(),
        text: format!(
            "A {value_text} cr bounty cube has been created in {area} by {holder} ({job}). \
             Speedy delivery bonus lost in {minutes:02}:00."
        ),
    });
}

fn area_name(world: &World, pos: TilePos) -> String {
    world
        .get_resource::<ss13_mapload::Areas>()
        .and_then(|a| a.area_at(pos))
        .map(|a| crate::shuttle::area_name(world, a))
        .unwrap_or_default()
}

/// `print_sheet`: the unclaimed station bounties on a paper on the console's tile.
// shortcut: no paper text yet; the sheet's text is its description.
fn print_sheet(world: &mut World, console: Entity) {
    let Some(&pos) = world.get::<TilePos>(console) else {
        return;
    };
    let mut text = String::from("Nanotrasen Cargo Bounties");
    for bounty in world.resource::<Bounties>().shared.iter() {
        if bounty.claimed {
            continue;
        }
        let reward = bounty.reward();
        text.push_str(&format!(
            "\n\n{}\n- Quantity requested: {}\n- Reward: {} cr.\n- Cut: {} cr.",
            bounty.name,
            bounty.print_required(),
            super::economy::dm_number(reward),
            (BOUNTY_CUT_STANDARD * reward).round()
        ));
    }
    let Some(paper) = world
        .try_resource_scope(|world, packs: Mut<SupplyPacks>| {
            Some(packs.templates.get(PAPER)?.spawn(world, Some(pos)))
        })
        .flatten()
    else {
        return;
    };
    world.entity_mut(paper).insert((
        Networked,
        DmName {
            name: "paper - Bounties".into(),
            desc: text,
        },
    ));
}

/// `/datum/export/bounty_box` with the cube's pricetag: the payees' cuts paid, the rest into
/// `report` under the bounty export. `false` when `cube` is not a bounty cube.
pub fn sell_cube(
    world: &mut World,
    exports: &mut Exports,
    cube: Entity,
    report: &mut ExportReport,
) -> bool {
    let Some(state) = world.get::<BountyCube>(cube).cloned() else {
        return false;
    };
    let value = state.sale_value();
    let (cuts, left) = state.split(value);
    let name = name_of(world, cube);
    for (account, cut) in cuts {
        let paid = world
            .resource_mut::<Economy>()
            .account_mut(account)
            .is_some_and(|a| a.adjust_money(cut));
        if paid {
            let text = format!("Sale of {name} recorded. {cut}{MONEY_SYMBOL} added to account.");
            bank_card_talk(world, account, &text);
        }
    }
    exports.report_bounty(left, &name, report);
    true
}

/// tg `bounty_cube/process`: an unsent cube (not at CentCom or in transit) nags the supply
/// channel and its holders, and loses its speedy delivery bonus.
fn nag_cubes(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut query = world.query::<(Entity, &BountyCube)>();
    let due: Vec<Entity> = query
        .iter(world)
        .filter(|(_, c)| now >= c.next_nag)
        .map(|(e, _)| e)
        .collect();
    if due.is_empty() {
        return;
    }
    let away_levels: Vec<u16> = world
        .get_resource::<super::supply::SupplyShuttle>()
        .map(|s| {
            let transit = world.get::<TilePos>(s.transit).map(|p| p.z);
            std::iter::once(s.centcom_z).chain(transit).collect()
        })
        .unwrap_or_default();
    for cube in due {
        let Some(pos) = crate::explosions::turf_of(world, cube) else {
            continue;
        };
        if away_levels.contains(&pos.z) {
            continue;
        }
        let Some(state) = world.get::<BountyCube>(cube).cloned() else {
            continue;
        };
        let area = area_name(world, pos);
        let value = super::economy::dm_number(state.bounty_value);
        let message = if state.speed_bonus > 0.0 {
            format!(
                "The {value} cr bounty cube is unsent in {area}. Speedy delivery bonus of {} \
                 credits lost.",
                super::economy::dm_number(state.bounty_value * state.speed_bonus)
            )
        } else {
            format!("The {value} cr bounty cube is unsent in {area}.")
        };
        world.write_message(MachineRadio {
            freq: ss13_chat::radio::FREQ_SUPPLY,
            z: pos.z,
            name: AAS_NAME.into(),
            text: message.clone(),
        });
        for &(account, _) in &state.payees {
            bank_card_talk(world, account, &message);
        }
        if let Some(mut cube) = world.get_mut::<BountyCube>(cube) {
            cube.speed_bonus = 0.0;
            cube.nag_cooldown *= NAG_MULTIPLIER;
            cube.next_nag = now + cube.nag_cooldown;
        }
    }
}

/// `add_bounties`: three new bounties to pick for the card's account, once per cooldown.
fn add_bounties(
    world: &mut World,
    client: Entity,
    console: Entity,
    pad: Entity,
    card: Option<(Entity, Option<u32>)>,
    now: f64,
) {
    let Some((card, Some(account))) = card else {
        return;
    };
    let cooldown = world
        .resource::<Bounties>()
        .account(account)
        .filter(|s| (s.current.is_some() || !s.choices.is_empty()) && now < s.ready_at)
        .map(|s| (s.ready_at - now).ceil() as u64);
    if let Some(left) = cooldown {
        let text = format!("попробуйте еще раз через {}:{:02}!", left / 60, left % 60);
        crate::tools::tell(world, client, Span::Warning, text);
        return;
    }
    let job = world
        .get::<IdRegistration>(card)
        .map(|r| {
            if r.job.is_empty() {
                r.assignment.clone()
            } else {
                r.job.clone()
            }
        })
        .filter(|j| !j.is_empty());
    let Some(job) = job else {
        crate::cargo::console::machine_say(
            world,
            console,
            "Запрашиваемая ID карта не имеет зарегистрированной должности!",
        );
        return;
    };
    // `RefreshParts`: 30 s less per tier above the first of the laser and the scanner.
    let ratings = crate::stock_parts::ratings(world, pad);
    let tiers = i64::from(ratings.sum(crate::stock_parts::PartKind::MicroLaser))
        + i64::from(ratings.sum(crate::stock_parts::PartKind::ScanningModule))
        - 2;
    let reduction = tiers as f64 * 30.0;
    let mut bounties = world.resource_mut::<Bounties>();
    let category = bounties.bounty_types(&job);
    let list = bounties.generate_bounty_list(category);
    let state = bounties.accounts.entry(account).or_default();
    state.choices = list;
    state.ready_at = now + BOUNTY_COOLDOWN_SECS - reduction;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(id: &str, wanted: &[(&str, bool)]) -> BountyDef {
        BountyDef {
            id: id.into(),
            name: id.rsplit('/').next().unwrap().into(),
            reward: 1000.0,
            required_count: 2,
            wanted: wanted.iter().map(|(t, w)| ((*t).into(), *w)).collect(),
            include_subtypes: true,
            ..default()
        }
    }

    fn bounties() -> Bounties {
        Bounties::new(
            vec![
                def(
                    "/datum/bounty/item/assistant/soap",
                    &[("/obj/item/soap", true)],
                ),
                def(
                    "/datum/bounty/item/assistant/spear",
                    &[("/obj/item/spear", true)],
                ),
                def(
                    "/datum/bounty/item/assistant/mop",
                    &[("/obj/item/mop", true)],
                ),
                def(
                    "/datum/bounty/item/engineering/gas",
                    &[("/obj/item/tank", true)],
                ),
                BountyDef {
                    multiplier: 1.0,
                    foodtype: "рагу".into(),
                    ..def(
                        "/datum/bounty/item/botany/apple",
                        &[("/obj/item/apple", true)],
                    )
                },
                def("/datum/bounty/item/special/doc", &[("/obj/item/doc", true)]),
            ],
            vec![JobCargoDef {
                title: "Station Engineer".into(),
                bounty_types: 9,
                ..default()
            }],
            7,
        )
    }

    #[test]
    fn zebra_typecache() {
        let mut d = def(
            "/datum/bounty/item/assistant/soap",
            &[("/obj/item/soap", true), ("/obj/item/soap/fake", false)],
        );
        assert!(wanted(&d, "/obj/item/soap"));
        assert!(wanted(&d, "/obj/item/soap/deluxe"));
        assert!(!wanted(&d, "/obj/item/soap/fake"));
        assert!(!wanted(&d, "/obj/item/soap/fake/very"));
        assert!(!wanted(&d, "/obj/item/soapy"));
        d.include_subtypes = false;
        assert!(wanted(&d, "/obj/item/soap"));
        assert!(!wanted(&d, "/obj/item/soap/deluxe"));
    }

    #[test]
    fn shipping_counts_stacks_and_stops_at_the_requirement() {
        let mut b = bounties();
        let mut bounty = b.make(0);
        let d = b.defs[0].clone();
        assert!(bounty.applies_to(&d, "/obj/item/soap"));
        bounty.ship(1);
        assert!(!bounty.can_claim());
        bounty.ship(5);
        assert!(bounty.can_claim());
        assert!(!bounty.applies_to(&d, "/obj/item/soap"), "full");
        assert_eq!(bounty.print_required(), "6/2");
        bounty.high_priority = true;
        assert_eq!(bounty.reward(), 1500.0);
    }

    #[test]
    fn categories_fall_back_to_assistant_bounties() {
        let mut b = bounties();
        for _ in 0..50 {
            let eng = b.random_type(9).unwrap();
            assert_eq!(b.defs[eng].id, "/datum/bounty/item/engineering/gas");
            // Chemistry has only reagent bounties: assistant ones instead.
            let chem = b.random_type(6).unwrap();
            assert!(b.defs[chem].id.starts_with("/datum/bounty/item/assistant/"));
            // `subtypesof(/datum/bounty/item/botany)`; the specials are never rolled.
            let any = b.random_type(CIV_JOB_RANDOM).unwrap();
            assert!(!b.defs[any].id.starts_with(SPECIAL));
        }
    }

    #[test]
    fn botany_new_sets_count_reward_and_text() {
        let mut b = bounties();
        let bounty = b.make(4);
        assert!((5..=10).contains(&bounty.required_count));
        assert_eq!(bounty.reward(), 1000.0 + 400.0);
        assert!(bounty.description.contains("рагу") && bounty.description.contains("apple"));
    }

    #[test]
    fn bounty_list_has_an_assistant_one_and_no_duplicates() {
        let mut b = bounties();
        for _ in 0..20 {
            let list = b.generate_bounty_list(b.bounty_types("Station Engineer"));
            // The engineering bounty, then assistant ones in the last five attempts.
            assert!((2..=3).contains(&list.len()), "{}", list.len());
            assert!(b.defs[list[0].def].id.contains("/assistant/"));
            assert!(
                list.iter()
                    .any(|x| b.defs[x.def].id.contains("/engineering/"))
            );
            let mut defs: Vec<usize> = list.iter().map(|x| x.def).collect();
            defs.sort_unstable();
            defs.dedup();
            assert_eq!(defs.len(), list.len(), "duplicates are rerolled");
        }
        assert_eq!(b.bounty_types("Clown"), CIV_JOB_RANDOM);
    }

    #[test]
    fn global_list_fills_once_per_category_and_adds_specials() {
        let mut b = bounties();
        b.first_global_update(0);
        let specials = b
            .shared
            .iter()
            .filter(|x| b.defs[x.def].id.starts_with(SPECIAL))
            .count();
        assert_eq!(specials, 1);
        // Five categories, but only four distinct non-special bounties exist.
        assert!(b.shared.len() <= CIV_BOUNTY_BASELINE + 1);
        let mut defs: Vec<usize> = b.shared.iter().map(|x| x.def).collect();
        defs.sort_unstable();
        defs.dedup();
        assert_eq!(defs.len(), b.shared.len(), "no bounty twice");
        b.shared[0].claimed = true;
        b.update_global_bounty_list(1, false);
        assert!(b.shared.iter().all(|x| !x.claimed));
    }

    #[test]
    fn cube_value_and_split() {
        let cube = BountyCube {
            bounty_value: 1000.0,
            speed_bonus: SPEED_BONUS,
            holder_cut: BOUNTY_CUT_STANDARD,
            payees: vec![(1, 3.0), (2, 1.0)],
            next_nag: 0.0,
            nag_cooldown: NAG_SECS,
        };
        assert_eq!(cube.sale_value(), 1200);
        let (cuts, left) = cube.split(1200);
        assert_eq!(cuts, vec![(1, 270), (2, 90)]);
        assert_eq!(left, 840);
    }

    #[test]
    fn selling_a_cube_pays_the_holders_and_reports_the_rest() {
        let mut world = World::new();
        let mut economy = Economy::new(1);
        let holder = economy.open_account("Bob", 0, "CIV");
        world.insert_resource(economy);
        let cube = world
            .spawn((
                DmName {
                    name: "1000 ¢ bounty cube".into(),
                    desc: String::new(),
                },
                BountyCube {
                    bounty_value: 1000.0,
                    speed_bonus: SPEED_BONUS,
                    holder_cut: BOUNTY_CUT_STANDARD,
                    payees: vec![(holder, 1.0)],
                    next_nag: 0.0,
                    nag_cooldown: NAG_SECS,
                },
            ))
            .id();
        let plain = world.spawn_empty().id();
        let mut exports = Exports::new(Vec::new());
        let mut report = ExportReport::default();
        assert!(!sell_cube(&mut world, &mut exports, plain, &mut report));
        assert!(sell_cube(&mut world, &mut exports, cube, &mut report));
        assert_eq!(
            world.resource::<Economy>().account(holder).unwrap().balance,
            360,
            "30% of 1200"
        );
        assert_eq!(report.total(), 840);
        let lines = exports.printout(&report);
        assert_eq!(lines, ["+840¢: Received 1 completed bounty cube."]);
    }

    #[test]
    fn data_shows_the_account_and_the_station_list() {
        let mut b = bounties();
        let current = b.make(0);
        let choice = b.make(1);
        b.accounts.insert(
            5,
            AccountBounties {
                current: Some(current),
                choices: vec![choice],
                ready_at: 0.0,
            },
        );
        b.first_global_update(0);
        let console = CivBountyConsole {
            inserted: Some(Entity::PLACEHOLDER),
            ..default()
        };
        let data = console_data(&console, Some(5), true, Some(&b));
        assert!(data.pad && data.id_inserted && data.picking);
        assert_eq!(data.id_bounty_num, "0/2");
        assert_eq!(data.id_bounty_value, 300.0);
        assert_eq!(data.choices.len(), 1);
        assert_eq!(data.list_bounty.len(), b.shared.len());
        let none = console_data(&CivBountyConsole::default(), None, false, Some(&b));
        assert!(!none.id_inserted && none.id_bounty_info.is_none());
        assert_eq!(none.id_bounty_num, "N/A");
    }
}
