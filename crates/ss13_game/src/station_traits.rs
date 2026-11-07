//! Station traits (server), after tg `SSstation` (`code/controllers/subsystem/processing/
//! station.dm`) and `/datum/station_trait` (`code/datums/station_traits/`), with BandaStation's
//! weights (`modular_bandastation/balance/code/station_traits.dm`, its blackout at 30%) and its
//! `config/game_options.txt` budgets.
//!
//! - [`roll`]: when the server builds (tg `SSstation/Initialize`, before the map's atoms), a
//!   budget per sign is picked by weight from [`StationTraitsConfig`], then traits of that sign
//!   by weight until the budget is spent (`pick_traits`: a trait costs `cost`, a picked trait
//!   takes its `blacklist` out of the pool). Forced traits replace the roll (tg
//!   `FUTURE_STATION_TRAITS_FILE`).
//! - Effects: map-load ones (maintenance loot through [`ss13_mapload::spawner::MaintLootScale`]),
//!   `New()` ones at `Startup` (the announcer), `on_round_start` ones from [`on_round_start`]
//!   (`crate::round` after the round-start spawns), and hooks the owning systems ask through
//!   [`has`] (latejoin spawns, mail, spiked drinks).
//! - The round-start report lists the traits with `show_in_report` ([`report_section`]).
//!
//! Ported traits are the ones whose mechanism exists here; the pool holds only them (a rolled
//! trait whose effect does not exist would lie in the report).
// shortcut: not in the pool (their systems are not ported): lucky winner, bountiful bounties,
// glowsticks, premium internals, deathrattle departments, cybernetic revolution, luxury and
// cramped pods, advanced medbots, shuttle loans, wise cows, ion storms, shuttle sale, missing
// wallet, assistant gimmicks, hangover, overflow bureaucracy, bot and machine languages, Pun
// Pun's revenge, revolutionary trashing, nebulae, structural weaknesses, every neutral trait but
// the announcers, the job traits; planetary-only ones (bright day, forested, eternal storm) never
// apply to the space maps here. No admin traits panel (forcing is `SS13_STATION_TRAITS`), no
// `revert()`. The lobby sign-up buttons ([`LobbyCandidates`]) exist; the job traits that own
// them (and `pre_jobs_assigned`, which picks among the candidates) are not in the pool yet.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::MapSeed;
use ss13_mapload::spawner::{MaintLootScale, Rng};

use crate::events::controller::pick_weight;
use crate::obj_damage::is_under;

pub const GALACTIC_GRANT: &str = "/datum/station_trait/galactic_grant";
pub const STRONG_SUPPLY_LINES: &str = "/datum/station_trait/strong_supply_lines";
pub const FILLED_MAINT: &str = "/datum/station_trait/filled_maint";
pub const QUICK_SHUTTLE: &str = "/datum/station_trait/quick_shuttle";
pub const ANNOUNCEMENT_INTERN: &str = "/datum/station_trait/announcement_intern";
pub const ANNOUNCEMENT_MEDBOT: &str = "/datum/station_trait/announcement_medbot";
pub const CARP_INFESTATION: &str = "/datum/station_trait/carp_infestation";
pub const DISTANT_SUPPLY_LINES: &str = "/datum/station_trait/distant_supply_lines";
pub const MAIL_BLOCKED: &str = "/datum/station_trait/mail_blocked";
pub const LATE_ARRIVALS: &str = "/datum/station_trait/late_arrivals";
pub const BLACKOUT: &str = "/datum/station_trait/blackout";
pub const EMPTY_MAINT: &str = "/datum/station_trait/empty_maint";
pub const SLOW_SHUTTLE: &str = "/datum/station_trait/slow_shuttle";
pub const RAD_STORMS: &str = "/datum/station_trait/random_event_weight_modifier/rad_storms";
pub const DUST_STORMS: &str = "/datum/station_trait/random_event_weight_modifier/dust_storms";
pub const SPIKED_DRINKS: &str = "/datum/station_trait/spiked_drinks";
pub const VENDING_SHORTAGE: &str = "/datum/station_trait/vending_shortage";
pub const XENOBUREAUCRACY_ERROR: &str = "/datum/station_trait/xenobureaucracy_error";

/// Blacklist entries of traits that are not ported (kept so the tables read as tg's).
const RANDOM_SPAWNS: &str = "/datum/station_trait/random_spawns";
const HANGOVER: &str = "/datum/station_trait/hangover";
const BIRTHDAY: &str = "/datum/station_trait/birthday";

/// `STATION_TRAIT_COST_FULL`, `STATION_TRAIT_COST_LOW`.
pub const COST_FULL: f64 = 1.0;
pub const COST_LOW: f64 = 0.5;
/// `STATION_TRAIT_COST_MINIMAL`.
pub const COST_MINIMAL: f64 = 0.3;
/// BandaStation's blackout: `prob(30)` per station APC (tg 60).
const BLACKOUT_CHANCE: f32 = 30.0;
/// `/datum/centcom_announcer/medbot`.
const MEDBOT_ANNOUNCER: &str = "/datum/centcom_announcer/medbot";
/// Vendors `STATION_TRAIT_VENDING_SHORTAGE` empties (`snack.dm`, `cola.dm`, `coffee.dm`).
const SHORTAGE_VENDORS: [&str; 3] = [
    "/obj/machinery/vending/snack",
    "/obj/machinery/vending/cola",
    "/obj/machinery/vending/coffee",
];
const COLA_VENDOR: &str = "/obj/machinery/vending/cola";
const ETHANOL: &str = "/datum/reagent/consumable/ethanol";
/// `/obj/machinery/vending/cola` `spiking_booze`.
const SPIKING_BOOZE: [(&str, i64); 12] = [
    ("/datum/reagent/consumable/ethanol/vodka", 5),
    ("/datum/reagent/consumable/ethanol/beer", 5),
    ("/datum/reagent/consumable/ethanol/whiskey", 5),
    ("/datum/reagent/consumable/ethanol/gin", 5),
    ("/datum/reagent/consumable/ethanol/rum", 5),
    ("/datum/reagent/consumable/ethanol/thirteenloko", 3),
    ("/datum/reagent/consumable/ethanol/absinthe", 3),
    ("/datum/reagent/consumable/ethanol/hooch", 3),
    ("/datum/reagent/consumable/ethanol/moonshine", 3),
    ("/datum/reagent/consumable/ethanol/beepsky_smash", 1),
    ("/datum/reagent/consumable/ethanol/gargle_blaster", 1),
    ("/datum/reagent/consumable/ethanol/neurotoxin", 1),
];

/// tg `STATION_TRAIT_POSITIVE` / `NEUTRAL` / `NEGATIVE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraitKind {
    Positive,
    Neutral,
    Negative,
}

/// A `/datum/station_trait` type's vars.
#[derive(Debug, PartialEq)]
pub struct TraitDef {
    pub path: &'static str,
    pub name: &'static str,
    pub kind: TraitKind,
    pub weight: i64,
    pub cost: f64,
    pub show_in_report: bool,
    pub report: &'static str,
    pub blacklist: &'static [&'static str],
}

const fn def(
    path: &'static str,
    name: &'static str,
    kind: TraitKind,
    weight: i64,
    cost: f64,
    report: &'static str,
    blacklist: &'static [&'static str],
) -> TraitDef {
    TraitDef {
        path,
        name,
        kind,
        weight,
        cost,
        show_in_report: true,
        report,
        blacklist,
    }
}

use TraitKind::{Negative, Neutral, Positive};

/// The ported traits with tg's (BandaStation's) vars, in tg's file order.
pub static TRAITS: [TraitDef; 18] = [
    def(
        GALACTIC_GRANT,
        "Галактический грант",
        Positive,
        5,
        COST_FULL,
        "Ваша станция получила специальный грант. Вашему отделу снабжения выделены \
         дополнительные средства.",
        &[],
    ),
    def(
        STRONG_SUPPLY_LINES,
        "Сильные линии снабжения",
        Positive,
        5,
        COST_FULL,
        "Цены в этой системе занижены. ПОКУПАЙТЕ-ПОКУПАЙТЕ-ПОКУПАЙТЕ!",
        &[DISTANT_SUPPLY_LINES],
    ),
    def(
        FILLED_MAINT,
        "Заполненные технические тоннели",
        Positive,
        5,
        COST_LOW,
        "Наши рабочие случайно забыли часть своих личных вещей в технических тоннелях.",
        &[EMPTY_MAINT],
    ),
    def(
        QUICK_SHUTTLE,
        "Быстрый шаттл",
        Positive,
        5,
        COST_FULL,
        "Благодаря близости к нашей станции снабжения, грузовой шаттл быстрее доставит груз в \
         отдел снабжения.",
        &[SLOW_SHUTTLE],
    ),
    def(
        ANNOUNCEMENT_INTERN,
        "Объявления от интерна",
        Neutral,
        1,
        COST_FULL,
        "Пожалуйста, будьте с ним поласковее.",
        &[ANNOUNCEMENT_MEDBOT, BIRTHDAY],
    ),
    def(
        ANNOUNCEMENT_MEDBOT,
        "Система \"оповещения\"",
        Neutral,
        1,
        COST_FULL,
        "В настоящее время наша система оповещения находится на плановом обслуживании. К \
         счастью, у нас есть резервная система.",
        &[ANNOUNCEMENT_INTERN, BIRTHDAY],
    ),
    def(
        CARP_INFESTATION,
        "Нашествие карпов",
        Negative,
        5,
        COST_FULL,
        "В районе станции обитает опасная фауна.",
        &[],
    ),
    def(
        DISTANT_SUPPLY_LINES,
        "Удаленные линии снабжения",
        Negative,
        3,
        COST_FULL,
        "Из-за удаленности от наших обычных линий снабжения заказы на доставку грузов \
         обходятся дороже.",
        &[STRONG_SUPPLY_LINES],
    ),
    def(
        MAIL_BLOCKED,
        "Забастовка почтовых работников",
        Negative,
        2,
        COST_FULL,
        "В связи с продолжающейся забастовкой, объявленной профсоюзом почтовых работников, \
         почта в эту смену доставляться не будет.",
        &[],
    ),
    def(
        LATE_ARRIVALS,
        "Позднее прибытие",
        Negative,
        2,
        COST_FULL,
        "Извините за это, мы не ожидали, что столкнёмся с этим блюющим гусем, когда будем \
         отправлять вас на новую станцию.",
        &[RANDOM_SPAWNS, HANGOVER],
    ),
    def(
        BLACKOUT,
        "Блэкаут",
        Negative,
        3,
        COST_FULL,
        "Освещение станции, похоже, повреждено. Будьте осторожны, начиная смену сегодня.",
        &[],
    ),
    def(
        EMPTY_MAINT,
        "Вычищенные технические тоннели",
        Negative,
        5,
        COST_LOW,
        "Наши рабочие убрали большую часть мусора в зонах технических тоннелей.",
        &[FILLED_MAINT],
    ),
    def(
        SLOW_SHUTTLE,
        "Медленный шаттл",
        Negative,
        5,
        COST_FULL,
        "Из-за удаленности нашей станции снабжения, время полета грузового шаттла до вашего \
         отдела снабжения будет больше.",
        &[QUICK_SHUTTLE],
    ),
    def(
        RAD_STORMS,
        "Радиационная буря",
        Negative,
        2,
        COST_FULL,
        "Через систему вашей станции проходит радиоактивная буря. Ожидайте повышенную \
         вероятность прохождения радиационных бурь над вашей станцией, а также возможность \
         возникновения нескольких радиационных бурь во время вашей смены.",
        &[],
    ),
    def(
        DUST_STORMS,
        "Пыльная буря",
        Negative,
        2,
        COST_LOW,
        "Пространство вокруг вашей станции затянуто облаками космической пыли. Ожидайте \
         повышенную вероятность повреждения корпуса станции пылевыми бурями.",
        &[],
    ),
    def(
        SPIKED_DRINKS,
        "Напитки с добавками",
        Negative,
        3,
        COST_LOW,
        "Из-за аварии на мегафабрике Robust Softdrinks в некоторых напитках могут содержаться \
         следы этанола или психоактивных химических веществ.",
        &[],
    ),
    def(
        VENDING_SHORTAGE,
        "Vending products shortage",
        Negative,
        3,
        COST_FULL,
        "We haven't had the time to take care of the station's food and drink vending \
         machines. Food and drink products are empty and need to be restocked.",
        &[],
    ),
    // BandaStation `/datum/station_trait/xenobureaucracy_error`: weight 0, only forced (an
    // admin's pick); lifts the species job restrictions (`crate::jobs::species_restricted`).
    def(
        XENOBUREAUCRACY_ERROR,
        "Ксеноглавы",
        Neutral,
        0,
        COST_MINIMAL,
        "Вследствии изменений в кадровой политике НаноТрейзен - ограничения по биологическому \
         виду для командного состава были аннулированы.",
        &[],
    ),
];

/// The definition of the trait type `path`.
pub fn trait_def(path: &str) -> Option<&'static TraitDef> {
    TRAITS.iter().find(|t| t.path == path)
}

/// tg's station trait config. Insert before `GameServerPlugin` to override; otherwise
/// BandaStation's `config/game_options.txt` with the environment:
/// - `SS13_FORBID_STATION_TRAITS=1`: none (tg `FORBID_STATION_TRAITS`);
/// - `SS13_STATION_TRAITS="/datum/station_trait/blackout,..."`: exactly these, no roll (tg's
///   future station traits file an admin sets for the next round).
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct StationTraitsConfig {
    pub forbid: bool,
    /// `POSITIVE_STATION_TRAITS` etc.: (budget, weight).
    pub positive: Vec<(f64, i64)>,
    pub neutral: Vec<(f64, i64)>,
    pub negative: Vec<(f64, i64)>,
    pub forced: Vec<String>,
}

impl StationTraitsConfig {
    /// BandaStation's config, no environment.
    pub fn tg() -> Self {
        Self {
            forbid: false,
            positive: vec![(0.0, 8), (1.0, 4), (2.0, 2), (3.0, 1)],
            neutral: vec![(0.0, 10), (1.0, 10), (2.0, 3), (2.5, 1)],
            negative: vec![(0.0, 8), (1.0, 4), (2.0, 2), (3.0, 1)],
            forced: Vec::new(),
        }
    }

    /// Tests (tg skips the roll under `UNIT_TESTS`): no traits unless a test forces them.
    pub fn none() -> Self {
        Self {
            forbid: true,
            ..Self::tg()
        }
    }
}

impl Default for StationTraitsConfig {
    fn default() -> Self {
        let forced = std::env::var("SS13_STATION_TRAITS")
            .map(|v| {
                v.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        Self {
            forbid: std::env::var("SS13_FORBID_STATION_TRAITS").is_ok_and(|v| v == "1"),
            forced,
            ..Self::tg()
        }
    }
}

/// tg `SSstation.station_traits`.
#[derive(Resource, Clone, Debug, Default)]
pub struct StationTraits {
    pub active: Vec<&'static TraitDef>,
}

impl StationTraits {
    pub fn has(&self, path: &str) -> bool {
        self.active.iter().any(|t| t.path == path)
    }
}

/// The traits' own rolls (round start effects, spiked drinks).
#[derive(Resource)]
pub struct TraitRng(pub Rng);

/// `/datum/station_trait/job` subtypes (`sign_up_button = TRUE`): path and `button_desc`.
pub const LOBBY_TRAITS: [(&str, &str); 5] = [
    (
        "/datum/station_trait/job/cargorilla",
        "Sign up to become the Cargo Gorilla, a peaceful shepherd of boxes.",
    ),
    (
        "/datum/station_trait/job/bridge_assistant",
        "Sign up to become the Bridge Assistant and watch over the Bridge.",
    ),
    (
        "/datum/station_trait/job/veteran_advisor",
        "Sign up to become a DISABLED but hard boiled Veteran Advisor of Nanotrasen Security \
         Force. Advise HoS and Captain, train Officers, all while fighting your PTSD.",
    ),
    (
        "/datum/station_trait/job/human_ai",
        "Sign up to become the \"AI\".",
    ),
    (
        "/datum/station_trait/job/pun_pun",
        "Ook ook ah ah, sign up to play as the bartender's monkey.",
    ),
];

/// The lobby button text of a trait with a sign-up button, `None` for the others.
pub fn lobby_button_desc(path: &str) -> Option<&'static str> {
    LOBBY_TRAITS
        .iter()
        .find(|(p, _)| *p == path)
        .map(|(_, desc)| *desc)
}

/// tg `/datum/station_trait/job` `lobby_candidates`: the client connections signed up per trait
/// path. Stale entries (disconnected, not ready) are the reader's to drop, as tg's
/// `pre_jobs_assigned` does.
#[derive(Resource, Clone, Debug, Default)]
pub struct LobbyCandidates(pub std::collections::HashMap<&'static str, Vec<Entity>>);

impl LobbyCandidates {
    pub fn signed_up(&self, path: &str, client: Entity) -> bool {
        self.0.get(path).is_some_and(|c| c.contains(&client))
    }

    /// Adds `client`, or removes it when it was signed up; `true` when it is now signed up.
    pub fn toggle(&mut self, path: &'static str, client: Entity) -> bool {
        let list = self.0.entry(path).or_default();
        if let Some(i) = list.iter().position(|&c| c == client) {
            list.remove(i);
            false
        } else {
            list.push(client);
            true
        }
    }
}

/// `HAS_TRAIT(SSstation, ...)` of the trait type `path`.
pub fn has(world: &World, path: &str) -> bool {
    world
        .get_resource::<StationTraits>()
        .is_some_and(|t| t.has(path))
}

/// tg `SetupTraits`: the forced traits, else a budget per sign and `pick_traits` for each.
pub fn roll(config: &StationTraitsConfig, rng: &mut Rng) -> Vec<&'static TraitDef> {
    let mut active: Vec<&'static TraitDef> = Vec::new();
    if config.forbid {
        return active;
    }
    // `selectable_traits_by_types`: every sign in one list, filtered by sign when picking.
    let mut selectable: Vec<&'static TraitDef> = Vec::new();
    if !config.forced.is_empty() {
        for path in &config.forced {
            match trait_def(path) {
                Some(t) => setup_trait(t, &mut active, &mut selectable),
                None => warn!("Invalid station trait path {path} was requested!"),
            }
        }
        return active;
    }
    selectable.extend(TRAITS.iter());
    let budget = |list: &[(f64, i64)], rng: &mut Rng| {
        let indexed: Vec<(usize, i64)> = list.iter().enumerate().map(|(i, b)| (i, b.1)).collect();
        pick_weight(&indexed, rng).map_or(0.0, |i| list[i].0)
    };
    let budgets = [
        budget(&config.positive, rng),
        budget(&config.neutral, rng),
        budget(&config.negative, rng),
    ];
    for (kind, budget) in [Positive, Neutral, Negative].into_iter().zip(budgets) {
        pick_traits(kind, budget, &mut active, &mut selectable, rng);
    }
    active
}

/// tg `pick_traits(trait_sign, budget)`.
fn pick_traits(
    kind: TraitKind,
    mut budget: f64,
    active: &mut Vec<&'static TraitDef>,
    selectable: &mut Vec<&'static TraitDef>,
    rng: &mut Rng,
) {
    while budget > 0.0 {
        // Anything costlier than what is left goes.
        selectable.retain(|t| t.kind != kind || t.cost <= budget);
        let pool: Vec<(usize, i64)> = selectable
            .iter()
            .enumerate()
            .filter(|(_, t)| t.kind == kind)
            .map(|(i, t)| (i, t.weight))
            .collect();
        if pool.is_empty() {
            return;
        }
        // BandaStation's fix: only zero weights left picks nothing.
        let Some(i) = pick_weight(&pool, rng) else {
            return;
        };
        let picked = selectable.remove(i);
        budget -= picked.cost;
        setup_trait(picked, active, selectable);
    }
}

/// tg `setup_trait`: once per type; its blacklist leaves the pool.
fn setup_trait(
    t: &'static TraitDef,
    active: &mut Vec<&'static TraitDef>,
    selectable: &mut Vec<&'static TraitDef>,
) {
    if active.iter().any(|a| a.path == t.path) {
        return;
    }
    active.push(t);
    info!("Station Trait: {} chosen for this round.", t.name);
    selectable.retain(|s| !t.blacklist.contains(&s.path));
}

/// tg `/datum/station_trait/get_report` of the reported traits, under BandaStation's
/// "Обнаруженные отклонения" heading; empty without any.
pub fn report_section(world: &World) -> String {
    let Some(traits) = world.get_resource::<StationTraits>() else {
        return String::new();
    };
    let lines: Vec<String> = traits
        .active
        .iter()
        .filter(|t| t.show_in_report)
        .map(|t| format!("- {} - {}", t.name, t.report))
        .collect();
    if lines.is_empty() {
        return String::new();
    }
    format!("=== Обнаруженные отклонения ===\n\n{}", lines.join("\n"))
}

pub struct StationTraitsPlugin;

impl Plugin for StationTraitsPlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<StationTraitsConfig>() {
            app.init_resource::<StationTraitsConfig>();
        }
        // The same seed the map loads with (`MapLoadPlugin` inserts it only when missing).
        let existing = app.world().get_resource::<MapSeed>().map(|s| s.0);
        let seed = existing.unwrap_or_else(|| {
            let seed = MapSeed::from_env();
            let s = seed.0;
            app.insert_resource(seed);
            s
        });
        let mut rng = Rng::new(seed ^ 0x5354_4154_494f_4e53);
        let config = app.world().resource::<StationTraitsConfig>().clone();
        let active = roll(&config, &mut rng);
        // `get_effective_lootcount`, read when the map's spawners resolve.
        let scale = if active.iter().any(|t| t.path == FILLED_MAINT) {
            1.5
        } else if active.iter().any(|t| t.path == EMPTY_MAINT) {
            0.5
        } else {
            1.0
        };
        app.insert_resource(MaintLootScale(scale))
            .insert_resource(StationTraits { active })
            .insert_resource(TraitRng(rng))
            .init_resource::<LobbyCandidates>()
            .add_systems(Startup, traits_new)
            .add_systems(Update, spike_drinks);
    }
}

/// The traits' `New()`: the announcers.
fn traits_new(world: &mut World) {
    let announcer = if has(world, ANNOUNCEMENT_INTERN) {
        crate::announce::INTERN_ANNOUNCER
    } else if has(world, ANNOUNCEMENT_MEDBOT) {
        MEDBOT_ANNOUNCER
    } else {
        return;
    };
    if let Some(mut a) = world.get_resource_mut::<crate::announce::Announcer>() {
        a.path = announcer.to_owned();
    }
}

fn rng_range(world: &mut World, lo: i64, hi: i64) -> i64 {
    world.resource_mut::<TraitRng>().0.range(lo, hi)
}

fn rng_prob(world: &mut World, percent: f32) -> bool {
    world.resource_mut::<TraitRng>().0.prob(percent)
}

/// `COMSIG_TICKER_ROUND_STARTING`: every active trait's `on_round_start`.
pub fn on_round_start(world: &mut World) {
    let Some(traits) = world.get_resource::<StationTraits>().cloned() else {
        return;
    };
    for t in traits.active {
        match t.path {
            GALACTIC_GRANT => {
                let amount = rng_range(world, 2000, 5000);
                if let Some(mut economy) =
                    world.get_resource_mut::<crate::cargo::economy::Economy>()
                    && let Some(cargo) = economy.department_mut(crate::cargo::economy::ACCOUNT_CAR)
                {
                    cargo.adjust_money(amount);
                }
            }
            // `SSeconomy.pack_price_modifier`: every pack's `get_cost`.
            STRONG_SUPPLY_LINES => scale_pack_prices(world, 0.8),
            DISTANT_SUPPLY_LINES => scale_pack_prices(world, 1.2),
            // The supply port's `callTime` (tg at its `Initialize`, before anyone can order).
            QUICK_SHUTTLE => scale_supply_call(world, 0.5),
            SLOW_SHUTTLE => scale_supply_call(world, 1.5),
            // `/datum/round_event_control/carp_migration/New()`.
            CARP_INFESTATION => modify_event(world, crate::events::carp_migration::NAME, |c| {
                c.weight *= 3;
                c.max_occurrences *= 2;
                c.earliest_start *= 0.5;
            }),
            RAD_STORMS => modify_event(world, crate::events::radiation_storm::NAME, |c| {
                c.weight = (c.weight as f64 * 1.5).round() as i64;
                c.max_occurrences += 2;
            }),
            DUST_STORMS => modify_event(world, crate::events::meteors::DUST_STORM, |c| {
                c.weight *= 2;
                c.max_occurrences += 3;
            }),
            BLACKOUT => blackout(world),
            VENDING_SHORTAGE => vending_shortage(world),
            _ => {}
        }
    }
}

fn scale_pack_prices(world: &mut World, factor: f64) {
    if let Some(mut packs) = world.get_resource_mut::<crate::cargo::supply::SupplyPacks>() {
        for pack in &mut packs.packs {
            pack.cost *= factor;
        }
    }
}

fn scale_supply_call(world: &mut World, factor: f64) {
    if let Some(mut config) = world.get_resource_mut::<crate::cargo::supply::SupplyConfig>() {
        config.call_secs *= factor;
    }
}

/// `random_event_weight_modifier/on_round_start` and the carp's `New()`.
fn modify_event(
    world: &mut World,
    name: &str,
    change: impl FnOnce(&mut crate::events::EventControl),
) {
    let Some(mut events) = world.get_resource_mut::<crate::events::Events>() else {
        return;
    };
    match events.controls.iter_mut().find(|c| c.name == name) {
        Some(control) => change(control),
        None => warn!("station trait: no {name:?} event to modify"),
    }
}

/// `/datum/station_trait/blackout/on_round_start`: station APCs overload their lighting.
fn blackout(world: &mut World) {
    let levels = crate::events::controller::station_levels(world);
    let mut q = world.query::<(Entity, &ss13_shared::TilePos, &crate::power::Apc)>();
    let mut apcs: Vec<Entity> = q
        .iter(world)
        .filter(|(_, pos, _)| levels.contains(&pos.z))
        .map(|(e, ..)| e)
        .collect();
    apcs.sort();
    for apc in apcs {
        if rng_prob(world, BLACKOUT_CHANCE) {
            crate::events::electrical_storm::overload_lighting(world, apc);
        }
    }
}

/// The snack, cola and coffee vendors' `Initialize(mapload)` on the station: every record
/// empty, `rand(1, 5)` credits each.
// shortcut: applied at round start to the vendors standing then, not at their map load (the
// same machines: nobody can buy before the round starts).
fn vending_shortage(world: &mut World) {
    let mut q = world.query::<(Entity, &crate::vending::Vendor)>();
    let mut vendors: Vec<Entity> = q
        .iter(world)
        .filter(|(_, v)| v.onstation)
        .map(|(e, _)| e)
        .collect();
    vendors.sort();
    for vendor in vendors {
        let short = crate::construction::path_of(world, vendor)
            .is_some_and(|p| SHORTAGE_VENDORS.iter().any(|v| is_under(&p, v)));
        if !short {
            continue;
        }
        let records = world
            .get::<crate::vending::Vendor>(vendor)
            .map_or(0, |v| v.records.len());
        let mut credits = 0;
        for _ in 0..records {
            credits += rng_range(world, 1, 5);
        }
        if let Some(mut machine) = world.get_mut::<crate::vending::Vendor>(vendor) {
            for record in &mut machine.records {
                record.amount = 0;
            }
            machine.credits_contained += credits;
        }
    }
}

/// `/obj/machinery/vending/cola/on_dispense` with `STATION_TRAIT_SPIKED_DRINKS`: 65% of
/// station cola drinks lose `rand(5, volume / 2)` units to the same of a random booze.
// shortcut: returned items (tg `dispense_returned`) are spiked too: `Vended` does not tell them
// apart.
fn spike_drinks(world: &mut World, mut cursor: Local<MessageCursor<crate::vending::Vended>>) {
    let vended: Vec<(Entity, Entity)> = cursor
        .read(world.resource::<Messages<crate::vending::Vended>>())
        .map(|v| (v.vendor, v.item))
        .collect();
    if vended.is_empty() || !has(world, SPIKED_DRINKS) {
        return;
    }
    let Some(chem) = world
        .get_resource::<crate::chem::Chem>()
        .map(|c| c.0.clone())
    else {
        return;
    };
    for (vendor, item) in vended {
        let onstation = world
            .get::<crate::vending::Vendor>(vendor)
            .is_some_and(|v| v.onstation);
        let cola =
            crate::construction::path_of(world, vendor).is_some_and(|p| is_under(&p, COLA_VENDOR));
        if !onstation || !cola || !rng_prob(world, 65.0) {
            continue;
        }
        let Some((booze_inside, max)) = world.get::<crate::chem::Reagents>(item).map(|r| {
            let booze = r.0.reagents.iter().any(|x| chem.is_type(x.id, ETHANOL));
            (booze, (r.0.maximum_volume * 0.5).floor() as i64)
        }) else {
            continue;
        };
        // Don't fill booze with more booze.
        if booze_inside {
            continue;
        }
        let amount = rng_range(world, 5, max) as f32;
        let booze: Vec<(usize, i64)> = SPIKING_BOOZE
            .iter()
            .enumerate()
            .map(|(i, (_, w))| (i, *w))
            .collect();
        let picked = pick_weight(&booze, &mut world.resource_mut::<TraitRng>().0);
        let Some(id) = picked.and_then(|i| chem.id(SPIKING_BOOZE[i].0)) else {
            continue;
        };
        let Some(mut reagents) = world.get_mut::<crate::chem::Reagents>(item) else {
            continue;
        };
        let removed = reagents.0.remove_all(amount);
        if removed <= 0.0 {
            continue;
        }
        reagents.0.add_reagent(
            &chem,
            id,
            removed,
            ss13_reagents::DEFAULT_REAGENT_TEMPERATURE,
            None,
            None,
            false,
            false,
        );
        reagents.0.handle_reactions(&chem);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(positive: f64, neutral: f64, negative: f64) -> StationTraitsConfig {
        StationTraitsConfig {
            positive: vec![(positive, 1)],
            neutral: vec![(neutral, 1)],
            negative: vec![(negative, 1)],
            ..StationTraitsConfig::tg()
        }
    }

    fn spent(traits: &[&TraitDef], kind: TraitKind) -> f64 {
        traits
            .iter()
            .filter(|t| t.kind == kind)
            .map(|t| t.cost)
            .sum()
    }

    #[test]
    fn budgets_are_spent_within_and_blacklists_hold() {
        for seed in 0..500 {
            let traits = roll(&config(3.0, 2.0, 3.0), &mut Rng::new(seed));
            assert!(spent(&traits, Positive) <= 3.0, "{traits:?}");
            assert!(spent(&traits, Neutral) <= 2.0);
            assert!(spent(&traits, Negative) <= 3.0);
            for t in &traits {
                for other in &traits {
                    assert!(
                        !t.blacklist.contains(&other.path),
                        "{} with {}",
                        t.path,
                        other.path
                    );
                }
                assert_eq!(traits.iter().filter(|o| o.path == t.path).count(), 1);
            }
            // Two neutral traits exist and blacklist each other: one at most.
            assert!(traits.iter().filter(|t| t.kind == Neutral).count() <= 1);
        }
    }

    #[test]
    fn a_full_budget_buys_until_nothing_fits() {
        // Budget 1: one full-cost trait, or low-cost ones until 0.
        for seed in 0..200 {
            let traits = roll(&config(1.0, 0.0, 0.0), &mut Rng::new(seed));
            assert!(!traits.is_empty());
            assert!(spent(&traits, Positive) <= 1.0, "{traits:?}");
            assert!(traits.iter().all(|t| t.kind == Positive));
            // The only low-cost positive is filled maintenance: alone, it leaves 0.5 unspent.
            if traits.len() == 1 && traits[0].path == FILLED_MAINT {
                continue;
            }
            assert_eq!(spent(&traits, Positive), 1.0, "{traits:?}");
        }
        assert!(roll(&config(0.0, 0.0, 0.0), &mut Rng::new(1)).is_empty());
    }

    #[test]
    fn weights_shape_the_picks() {
        let mut counts = std::collections::HashMap::<&str, u32>::new();
        for seed in 0..4000 {
            for t in roll(&config(0.0, 0.0, 1.0), &mut Rng::new(seed)) {
                *counts.entry(t.path).or_default() += 1;
            }
        }
        // Carp (5) against rad storms (2) among full-cost negatives.
        let carp = f64::from(counts[CARP_INFESTATION]);
        let rad = f64::from(counts[RAD_STORMS]);
        assert!((carp / rad - 2.5).abs() < 0.5, "{counts:?}");
    }

    #[test]
    fn forced_and_forbidden() {
        let mut forced = StationTraitsConfig::tg();
        forced.forced = vec![
            BLACKOUT.into(),
            "/datum/station_trait/nope".into(),
            BLACKOUT.into(),
        ];
        let traits = roll(&forced, &mut Rng::new(1));
        assert_eq!(traits.len(), 1);
        assert_eq!(traits[0].path, BLACKOUT);
        assert!(roll(&StationTraitsConfig::none(), &mut Rng::new(1)).is_empty());
    }

    #[test]
    fn every_trait_is_defined_once_and_blacklists_point_at_traits() {
        let known = [RANDOM_SPAWNS, HANGOVER, BIRTHDAY];
        for t in &TRAITS {
            assert_eq!(TRAITS.iter().filter(|o| o.path == t.path).count(), 1);
            for b in t.blacklist {
                assert!(trait_def(b).is_some() || known.contains(b), "{b}");
            }
        }
    }

    #[test]
    fn round_start_changes_prices_shuttle_and_events() {
        let mut world = World::new();
        world.insert_resource(StationTraits {
            active: [
                STRONG_SUPPLY_LINES,
                SLOW_SHUTTLE,
                CARP_INFESTATION,
                DUST_STORMS,
            ]
            .iter()
            .filter_map(|p| trait_def(p))
            .collect(),
        });
        world.insert_resource(TraitRng(Rng::new(1)));
        world.insert_resource(crate::cargo::supply::SupplyPacks::new(vec![
            ss13_mapload::cargo::SupplyPackDef {
                id: "/datum/supply_pack/test".into(),
                cost: 1000.0,
                ..default()
            },
        ]));
        world.insert_resource(crate::cargo::supply::SupplyConfig {
            call_secs: 60.0,
            ignition_secs: 5.5,
            template: None,
        });
        let mut events = crate::events::Events::default();
        events.controls = crate::events::controls();
        world.insert_resource(events);
        on_round_start(&mut world);
        let packs = world.resource::<crate::cargo::supply::SupplyPacks>();
        assert_eq!(packs.packs[0].cost, 800.0);
        let call = world
            .resource::<crate::cargo::supply::SupplyConfig>()
            .call_secs;
        assert_eq!(call, 90.0);
        let events = world.resource::<crate::events::Events>();
        let carp = &events.controls[events.index(crate::events::carp_migration::NAME).unwrap()];
        assert_eq!((carp.weight, carp.max_occurrences), (45, 12));
        assert_eq!(carp.earliest_start, 5.0 * 60.0);
        let dust = &events.controls[events.index(crate::events::meteors::DUST_STORM).unwrap()];
        assert_eq!((dust.weight, dust.max_occurrences), (28, 6));
    }
}
