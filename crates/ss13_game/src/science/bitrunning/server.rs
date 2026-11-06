//! The quantum server, its console and the byteforge (server), after tg
//! `code/modules/bitrunning/server/*.dm`, `objects/quantum_console.dm`, `objects/byteforge.dm`,
//! `objects/loot_crate.dm` and `objects/landmarks.dm`.
//!
//! - The console (tgui `QuantumConsole`) works the quantum server on a tile next to it: the
//!   domains by difficulty with what its scanner tier and points let it see, loading one by key
//!   or at random (`cold_boot_map`), stopping it (`begin_shutdown`: with avatars connected, a
//!   20 s `do_after` after warning them), the connected avatars' vitals.
//! - Loading ([`cold_boot`]): the server is ready, nothing is loaded, nobody is connected, the
//!   points cover the cost. The domain's map loads mid-round onto a level of its own
//!   (`ss13_mapload::load_template_with`); before it spawns, each `/obj/modular_map_root/safehouse`
//!   gets its safehouse pasted (`_maps/safehouses/<key>.dmm`) and the bitrunning landmarks are
//!   taken out and remembered (exits, goal tiles, cache spawns); a permanent exit gets its
//!   hololadder, a random cache spawn the encrypted cache. The map pass then brings it up like
//!   the station; the domain's megafauna drop an encrypted cache when they die.
//! - An encrypted cache that comes onto a goal tile ([`deliveries`]) is decrypted
//!   (`generate_loot`): the server earns the domain's reward points, grades the run
//!   (`grade_completion`) and has a nearby byteforge (4 tiles) put out a decrypted crate with
//!   ores scaled by the rewards bonus (`calculate_rewards`), the domain's `completion_loot` and
//!   the completion certificate. The domain is complete.
//! - Stopping (`reset`): every avatar is disconnected (with damage), the domain is scrubbed
//!   15 s later (everything on its levels but turfs and ghosts is deleted; players there ghost),
//!   the server cools down `2 min × capacitor coefficient`. Power loss disconnects everyone.
//! - Parts: capacitors cut the cooldown (`1.15 - 0.15 × tier`), the scanning module's tier shows
//!   more of the domains, servos add to the rewards bonus and cut the disconnect damage.
// shortcut: no glitches or cyber police (`setup_glitch`, threat stays a score only), no emag,
// no broadcasting to entertainment monitors, no modular mob segments, no curiosities
// (secondary loot), no point-gated crates (`loot_signal`), no crate randomizer, no ghost-role
// missions (`mission_min_candidates`), no tech disk reward, no domain-specific `setup_domain`;
// a megafauna keeps its own drops and health; the byteforge puts the crate out at once (no 1 s
// flicker); messages go to the user's chat instead of balloon alerts; no sounds; a domain loads
// onto a new level each time and its scrubbed level stays empty (turfs are kept: atmos keeps
// its tiles), one level per boot, at most one boot per cooldown.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_assets::dmm::{DmValue, DmmMap};
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmCloset, DmMob, DmName, DmObj, IconIndex, MapLoadPlugin};
use ss13_net::server::Networked;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::UiInterface as _;
use ss13_tgui::UiStatus;
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};
use ss13_world::{SpatialIndex, WorldSystems};

use super::avatar::{self, AvatarConnection};
use super::domains::{self, Domain, NO_NOHIT_BONUS, TEST_ONLY};
use super::*;
use crate::click::{ClickResolved, ResolveClicks};
use crate::closets::Closet;
use crate::health::{Health, Stat};
use crate::inventory::Inventory;
use crate::power::Unpowered;
use crate::server::PlayerMob;
use crate::stock_parts::PartKind;
use crate::storage::StoredIn;
use crate::tools::{Stack, tell};

pub const SERVER_ROOT: &str = "/obj/machinery/quantum_server";
pub const CONSOLE_ROOT: &str = "/obj/machinery/computer/quantum_console";
pub const BYTEFORGE_ROOT: &str = "/obj/machinery/byteforge";
pub const ENCRYPTED_CACHE: &str = "/obj/structure/closet/crate/secure/bitrunning/encrypted";
pub const DECRYPTED_CACHE: &str = "/obj/structure/closet/crate/secure/bitrunning/decrypted";
pub const HOLOLADDER: &str = "/obj/structure/hololadder";
const PAPER: &str = "/obj/item/paper";
pub const DOMAINS_DIR: &str = "_maps/virtual_domains";
pub const SAFEHOUSES_DIR: &str = "_maps/safehouses";
const LANDMARK_ROOT: &str = "/obj/effect/landmark/bitrunning";
const MODULAR_ROOT: &str = "/obj/modular_map_root";
const SAFEHOUSE_ROOT: &str = "/obj/modular_map_root/safehouse";
const MEGAFAUNA_ROOTS: [&str; 2] = [
    "/mob/living/simple_animal/hostile/megafauna",
    "/mob/living/basic/boss",
];
/// No ID has it: `encrypted/can_unlock` is `FALSE`.
const NO_ACCESS: &str = "bitrunning_encrypted";

/// `server_cooldown_time`.
pub const COOLDOWN_SECS: f64 = 120.0;
/// `reset`'s `addtimer(scrub_vdom, 15 SECONDS)`.
pub const SCRUB_DELAY_SECS: f64 = 15.0;
/// `begin_shutdown`'s `do_after(user, 20 SECONDS)`, deciseconds.
const SHUTDOWN_DS: f32 = 200.0;
/// `MAX_DISTANCE` of netpods and byteforges from their server.
pub const MAX_DISTANCE: u32 = 4;
/// `multiplayer_bonus`, `nohit_bonus`.
const MULTIPLAYER_BONUS: f32 = 1.1;
const NOHIT_BONUS: f32 = 0.8;

/// `ORE_MULTIPLIER_*` of the decrypted cache, in `PopulateContents` order with the reward points
/// each needs (`reward_points > n`).
const ORES: [(&str, f32, i32); 9] = [
    ("/obj/item/stack/ore/iron", 3.0, 0),
    ("/obj/item/stack/ore/glass", 2.0, 0),
    ("/obj/item/stack/ore/silver", 0.7, 1),
    ("/obj/item/stack/ore/titanium", 0.5, 1),
    ("/obj/item/stack/ore/plasma", 1.0, 2),
    ("/obj/item/stack/ore/gold", 0.6, 2),
    ("/obj/item/stack/ore/uranium", 0.4, 2),
    ("/obj/item/stack/ore/diamond", 0.3, 3),
    ("/obj/item/stack/ore/bluespace_crystal", 0.2, 3),
];

/// The bitrunning landmarks a domain's loader looks for (`load_map_items`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// `hololadder_spawn`: where an avatar comes in (and its ladder).
    Exit,
    /// `permanent_exit`: an exit with a ladder from the start.
    PermanentExit,
    /// `cache_goal_turf`.
    Goal,
    /// `cache_spawn`.
    CacheSpawn,
}

fn mark_of(path: &str) -> Option<Mark> {
    Some(match path.strip_prefix(LANDMARK_ROOT)? {
        "/hololadder_spawn" => Mark::Exit,
        "/permanent_exit" => Mark::PermanentExit,
        "/cache_goal_turf" => Mark::Goal,
        "/cache_spawn" => Mark::CacheSpawn,
        _ => return None,
    })
}

/// What [`prepare_domain`] found on a parsed domain map, in map coordinates.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Prepared {
    /// The map's z-levels.
    pub levels: u32,
    pub marks: Vec<(Mark, u32, u32, u32)>,
    /// Safehouse keys that were pasted.
    pub safehouses: Vec<String>,
}

/// `strings/modular_maps/safehouse.toml`: the room key's module.
pub fn safehouse_file(key: &str) -> String {
    if key == "test_only" {
        "test_only_safehouse.dmm".to_owned()
    } else {
        format!("{key}.dmm")
    }
}

/// Before the domain spawns: each safehouse root's room pasted with its bottom-left corner on
/// the root (`/obj/modular_map_root/load_map`), then the landmarks noted and taken out with the
/// roots (tg `qdel`s them once read).
pub fn prepare_domain(source: &Path, map: &mut DmmMap) -> Prepared {
    let (sx, sy, sz) = map.size;
    let mut roots = Vec::new();
    for z in 1..=sz {
        for y in 1..=sy {
            for x in 1..=sx {
                let Some(prefab) = map.get(x, y, z) else {
                    continue;
                };
                for o in &prefab.objects {
                    if o.path == SAFEHOUSE_ROOT
                        && let Some(DmValue::Str(key)) = o.var("key")
                    {
                        roots.push((x, y, z, key.clone()));
                    }
                }
            }
        }
    }
    let mut out = Prepared {
        levels: sz,
        ..default()
    };
    for (x, y, z, key) in roots {
        let path = source.join(SAFEHOUSES_DIR).join(safehouse_file(&key));
        match DmmMap::from_file(&path) {
            Ok(house) => {
                ss13_mapload::ruins::paste(map, &house, (x, y, z));
                out.safehouses.push(key);
            }
            Err(e) => warn!("safehouse {}: {e}", path.display()),
        }
    }
    for z in 1..=sz {
        for y in 1..=sy {
            for x in 1..=sx {
                let Some(prefab) = map.get(x, y, z) else {
                    continue;
                };
                for o in &prefab.objects {
                    if let Some(mark) = mark_of(&o.path) {
                        out.marks.push((mark, x, y, z));
                    }
                }
            }
        }
    }
    for prefab in &mut map.prefabs {
        prefab
            .objects
            .retain(|o| !is_under(&o.path, LANDMARK_ROOT) && !is_under(&o.path, MODULAR_ROOT));
    }
    out
}

/// A run's grade (`BITRUNNING_GRADE_*`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Grade {
    #[default]
    None,
    D,
    C,
    B,
    A,
    S,
}

impl Grade {
    pub fn letter(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::D => "D",
            Self::C => "C",
            Self::B => "B",
            Self::A => "A",
            Self::S => "S",
        }
    }

    /// `GLOB.bitrunning_grades`: the Font Awesome icon.
    pub fn symbol(self) -> &'static str {
        match self {
            Self::None => "minus",
            Self::D => "d",
            Self::C => "c",
            Self::B => "b",
            Self::A => "a",
            Self::S => "s",
        }
    }

    /// `grade_completion`'s `switch(score)`.
    pub fn of_score(score: i32) -> Self {
        match score {
            1..=4 => Self::D,
            5..=7 => Self::C,
            8..=10 => Self::B,
            11..=13 => Self::A,
            _ => Self::S,
        }
    }
}

/// `grade_completion`'s time score.
pub fn time_score(secs: f64) -> i32 {
    match secs {
        s if s <= 60.0 => 10,
        s if s <= 120.0 => 5,
        s if s <= 300.0 => 3,
        s if s <= 600.0 => 2,
        _ => 1,
    }
}

/// `grade_completion` without the threat it adds: the score.
pub fn completion_score(domain: &Domain, threats: i32, secs: f64) -> i32 {
    threats * 5 + domain.reward_points + time_score(secs) * (i32::from(domain.difficulty) + 1)
}

/// `/datum/lazy_template/virtual_domain/best_grade` of every domain this round.
#[derive(Resource, Debug, Default)]
pub struct DomainGrades(pub HashMap<&'static str, Grade>);

/// A loaded domain (tg `generated_domain` and the server's per-domain vars).
#[derive(Clone, Debug, PartialEq)]
pub struct LoadedDomain {
    pub domain: &'static Domain,
    pub levels: Vec<u16>,
    /// `start_time`, game seconds.
    pub start_time: f64,
    /// `exit_turfs`.
    pub exit_turfs: Vec<TilePos>,
    pub goal_turfs: Vec<TilePos>,
}

/// `/obj/machinery/quantum_server`.
#[derive(Component, Clone, Debug)]
pub struct QuantumServer {
    pub points: i32,
    pub is_ready: bool,
    pub domain: Option<LoadedDomain>,
    /// `domain_randomized`.
    pub randomized: bool,
    /// `domain_complete`.
    pub complete: bool,
    pub retries_spent: u32,
    /// Builds with completions (glitch odds in tg).
    pub threat: i32,
    /// The connected avatars (`avatar_connection_refs`).
    pub connections: Vec<Entity>,
    /// When the pending `scrub_vdom` runs.
    pub scrub_at: Option<f64>,
    /// When the pending `cool_off` runs.
    pub ready_at: Option<f64>,
    /// `scanner_tier` (`RefreshParts`), kept for the console.
    pub scanner_tier: u32,
}

impl Default for QuantumServer {
    fn default() -> Self {
        Self {
            points: 0,
            is_ready: true,
            domain: None,
            randomized: false,
            complete: false,
            retries_spent: 0,
            threat: 0,
            connections: Vec::new(),
            scrub_at: None,
            ready_at: None,
            scanner_tier: 1,
        }
    }
}

/// `RefreshParts` of the quantum server.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ServerParts {
    pub capacitor_coefficient: f32,
    pub scanner_tier: u32,
    pub servo_bonus: f32,
}

pub fn server_parts(world: &World, server: Entity) -> ServerParts {
    let r = crate::stock_parts::ratings(world, server);
    ServerParts {
        capacitor_coefficient: 1.15 - r.first(PartKind::Capacitor).unwrap_or(1) as f32 * 0.15,
        scanner_tier: r.first(PartKind::ScanningModule).unwrap_or(1),
        servo_bonus: r.sum(PartKind::Servo) as f32 * 0.1,
    }
}

/// `/obj/machinery/computer/quantum_console`: its server (`find_server`).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct QuantumConsole {
    pub server: Option<Entity>,
}

/// `/obj/machinery/byteforge`.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Byteforge;

/// `/obj/structure/closet/crate/secure/bitrunning/encrypted`: the domain's goal.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct EncryptedCache;

/// A domain's megafauna (`make_virtual_megafauna`): drops an encrypted cache on death.
#[derive(Component, Clone, Copy, Debug)]
pub struct VirtualBoss;

/// The bitrunning `do_after`s (`crate::construction::Job::Bitrunning`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BitrunJob {
    /// `enter_matrix`'s 2 s in the netpod.
    Connect { pod: Entity },
    /// `hololadder/disconnect`'s travel time.
    Ladder,
    /// `begin_shutdown`'s 20 s at the console.
    Shutdown { server: Entity },
    /// `default_pry_open` of an occupied netpod.
    PryPod,
}

pub struct BitrunningServerPlugin;

impl Plugin for BitrunningServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DomainGrades>()
            .register_ui::<QuantumConsoleUi>()
            .add_systems(ss13_world::MapInit, setup_machines)
            .add_systems(
                Update,
                (
                    open_console
                        .after(WorldSystems::Movement)
                        .after(ResolveClicks)
                        .before(UiSystems::Open),
                    console_acts.in_set(UiSystems::Handle),
                    unpowered_windows.in_set(UiSystems::StatusHook),
                    (deliveries, boss_drops, server_timers)
                        .chain()
                        .after(WorldSystems::Movement)
                        .run_if(any_with_component::<QuantumServer>),
                ),
            );
        avatar::build(app);
    }
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn path_of(world: &World, e: Entity) -> Option<String> {
    crate::construction::path_of(world, e).or_else(|| world.get::<DmMob>(e).map(|m| m.path.clone()))
}

/// The machines' state at map load; consoles find their server next to them.
fn setup_machines(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &DmObj), With<ss13_world::Mapped>>();
    let found: Vec<(Entity, String)> = query
        .iter(world)
        .filter(|(_, o)| {
            [
                SERVER_ROOT,
                CONSOLE_ROOT,
                BYTEFORGE_ROOT,
                avatar::NETPOD_ROOT,
            ]
            .iter()
            .any(|root| is_under(&o.path, root))
        })
        .map(|(e, o)| (e, o.path.clone()))
        .collect();
    for (e, path) in &found {
        if is_under(path, SERVER_ROOT) {
            world.entity_mut(*e).insert(QuantumServer::default());
        } else if is_under(path, BYTEFORGE_ROOT) {
            world.entity_mut(*e).insert(Byteforge);
        } else if is_under(path, avatar::NETPOD_ROOT) {
            avatar::setup_netpod(world, *e);
        }
    }
    for (e, path) in found {
        if is_under(&path, CONSOLE_ROOT) {
            let server = console_server(world, e);
            world.entity_mut(e).insert(QuantumConsole { server });
        }
    }
}

/// The console's `find_server`: a quantum server on a cardinal neighbour.
fn console_server(world: &World, console: Entity) -> Option<Entity> {
    let &pos = world.get::<TilePos>(console)?;
    let index = world.resource::<SpatialIndex>();
    [
        ss13_shared::Dir::North,
        ss13_shared::Dir::South,
        ss13_shared::Dir::East,
        ss13_shared::Dir::West,
    ]
    .into_iter()
    .flat_map(|d| index.on_tile(pos.step(d)).iter().copied())
    .find(|&e| world.get::<QuantumServer>(e).is_some())
}

/// A `T` machine within `radius` of `pos` (`locate() in oview`), nearest entity first.
pub(crate) fn nearby<T: Component>(world: &World, pos: TilePos, radius: u32) -> Option<Entity> {
    let mut found: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(pos, radius)
        .filter(|&e| world.get::<T>(e).is_some())
        .collect();
    found.sort();
    found.first().copied()
}

/// `is_operational`: powered and not broken.
pub fn operational(world: &World, e: Entity) -> bool {
    crate::medical::pods::operational(world, e)
}

fn plain(name: &str) -> &str {
    crate::vending::plain_name(name)
}

fn open_console(world: &mut World, mut clicks: Local<MessageCursor<ClickResolved>>) {
    let pending: Vec<ClickResolved> = clicks
        .read(world.resource::<Messages<ClickResolved>>())
        .copied()
        .collect();
    for click in pending {
        let Some(console) = world.get::<QuantumConsole>(click.target).copied() else {
            continue;
        };
        if !operational(world, click.target)
            || world
                .get::<Inventory>(click.mob)
                .is_none_or(|i| i.active_item().is_some())
        {
            continue;
        }
        if console
            .server
            .is_none_or(|s| world.get::<QuantumServer>(s).is_none())
        {
            let server = console_server(world, click.target);
            world
                .entity_mut(click.target)
                .insert(QuantumConsole { server });
        }
        let title = world
            .get::<DmName>(click.target)
            .map_or_else(String::new, |n| plain(&n.name).to_owned());
        world.write_message(OpenUi::new::<QuantumConsoleUi>(
            click.client,
            click.mob,
            click.target,
            title,
        ));
    }
}

fn unpowered_windows(
    mut windows: Query<&mut UiWindow>,
    unpowered: Query<(), (With<QuantumConsole>, With<Unpowered>)>,
) {
    for mut window in &mut windows {
        if window.interface == QuantumConsoleUi::NAME && unpowered.contains(window.source) {
            window.cap_status(UiStatus::Update);
        }
    }
}

type ConsoleParams = (
    Query<'static, 'static, (&'static QuantumServer, Has<Unpowered>)>,
    Query<
        'static,
        'static,
        (
            &'static AvatarConnection,
            &'static Health,
            Option<&'static Name>,
        ),
    >,
    Option<Res<'static, DomainGrades>>,
);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConsoleAct {
    SetDomain(String),
    RandomDomain,
    StopDomain,
    Refresh,
}

/// `get_available_domains(scanner_tier, points)`.
pub fn available_domains(
    scanner_tier: u32,
    points: i32,
    grades: Option<&DomainGrades>,
) -> Vec<DomainInfo> {
    domains::DOMAINS
        .iter()
        .filter(|d| d.flags & TEST_ONLY == 0)
        .map(|d| {
            let can_view = d.can_view_name(points);
            let grade = grades
                .and_then(|g| g.0.get(d.key).copied())
                .unwrap_or_default();
            DomainInfo {
                announce_ghosts: d.announce_ghosts,
                cost: d.cost,
                desc: if can_view {
                    d.desc.to_owned()
                } else {
                    "Возможности сканирования ограничены. Не удается определить данные о домене."
                        .to_owned()
                },
                difficulty: d.difficulty,
                id: d.key.to_owned(),
                is_modular: d.is_modular,
                has_secondary_objectives: d.has_secondary,
                name: if can_view { d.name } else { REDACTED }.to_owned(),
                reward: if d.can_view_reward(scanner_tier, points) {
                    d.reward_points.to_string()
                } else {
                    REDACTED.to_owned()
                },
                grade_symbol: grade.symbol().to_owned(),
            }
        })
        .collect()
}

impl ServerUi for QuantumConsoleUi {
    type Source = &'static QuantumConsole;
    type UserParam = ConsoleParams;
    type Act = ConsoleAct;

    fn static_data(_: &QuantumConsole) {}

    fn data(
        console: &QuantumConsole,
        _: Entity,
        (servers, avatars, grades): &SystemParamItem<'_, '_, ConsoleParams>,
    ) -> QuantumConsoleData {
        let Some((server, unpowered)) = console.server.and_then(|s| servers.get(s).ok()) else {
            return QuantumConsoleData::default();
        };
        let scanner_tier = server.scanner_tier;
        let exits = server.domain.as_ref().map_or(0, |d| d.exit_turfs.len()) as i32;
        QuantumConsoleData {
            connected: true,
            available_domains: available_domains(scanner_tier, server.points, grades.as_deref()),
            avatars: server
                .connections
                .iter()
                .filter_map(|&a| avatars.get(a).ok())
                .map(|(conn, health, name)| AvatarInfo {
                    health: health.health(),
                    name: name.map_or_else(String::new, |n| n.to_string()),
                    pilot: conn.pilot.clone(),
                    brute: health.brute,
                    burn: health.burn,
                    tox: health.tox,
                    oxy: health.oxy,
                })
                .collect(),
            generated_domain: server.domain.as_ref().map(|d| d.domain.key.to_owned()),
            occupants: server.connections.len() as u32,
            points: server.points,
            randomized: server.randomized,
            ready: server.is_ready && !unpowered,
            retries_left: exits - server.retries_spent as i32,
            scanner_tier,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<ConsoleAct, UiReject> {
        match action {
            SET_DOMAIN => params
                .get(DOMAIN_ID)
                .map(|id| ConsoleAct::SetDomain(id.to_owned()))
                .ok_or_else(|| UiReject::BadParams(format!("{DOMAIN_ID}: expected a key"))),
            RANDOM_DOMAIN => Ok(ConsoleAct::RandomDomain),
            STOP_DOMAIN => Ok(ConsoleAct::StopDomain),
            REFRESH => Ok(ConsoleAct::Refresh),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

fn console_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<QuantumConsoleUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, ConsoleAct)> = acts
        .read(world.resource::<Messages<UiActed<QuantumConsoleUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, console, act) in pending {
        if !operational(world, console) {
            continue;
        }
        let Some(server) = world.get::<QuantumConsole>(console).and_then(|c| c.server) else {
            continue;
        };
        if world.get::<QuantumServer>(server).is_none() {
            continue;
        }
        match act {
            ConsoleAct::SetDomain(id) => {
                cold_boot(world, server, Some(id.as_str()), false, Some(client));
            }
            ConsoleAct::RandomDomain => {
                let key = random_domain(world, server);
                cold_boot(world, server, key, true, Some(client));
            }
            ConsoleAct::StopDomain => begin_shutdown(world, client, actor, server, console),
            ConsoleAct::Refresh => {}
        }
    }
}

/// `get_random_domain_id`.
fn random_domain(world: &mut World, server: Entity) -> Option<&'static str> {
    let points = world.get::<QuantumServer>(server)?.points;
    let picks = domains::random_candidates(points);
    if picks.is_empty() {
        return None;
    }
    let i = world
        .resource_mut::<crate::combat::CombatRng>()
        .range(0, picks.len() as u32 - 1) as usize;
    Some(picks[i].key)
}

fn say(world: &mut World, client: Option<Entity>, span: Span, text: &str) {
    if let Some(client) = client {
        tell(world, client, span, text);
    }
}

/// `cold_boot_map(map_key, was_random_selection)`: every check, the load, the cost. True when
/// the domain loaded.
pub fn cold_boot(
    world: &mut World,
    server: Entity,
    key: Option<&str>,
    random: bool,
    client: Option<Entity>,
) -> bool {
    let Some(state) = world.get::<QuantumServer>(server).cloned() else {
        return false;
    };
    if !state.is_ready || !operational(world, server) {
        return false;
    }
    let Some(key) = key else {
        say(world, client, Span::Warning, "домен не выбран!");
        return false;
    };
    if state.domain.is_some() {
        say(world, client, Span::Warning, "остановите текущий домен!");
        return false;
    }
    if !state.connections.is_empty() {
        say(
            world,
            client,
            Span::Warning,
            "все клиенты должны отключиться!",
        );
        return false;
    }
    // `load_domain`: the key and the points.
    let Some(domain) = domains::by_key(key).filter(|d| state.points >= d.cost) else {
        say(world, client, Span::Warning, "инициализация провалена!");
        return false;
    };
    let Some(loaded) = load_domain(world, domain) else {
        say(world, client, Span::Warning, "инициализация провалена!");
        return false;
    };
    let now = now(world);
    if let Some(mut s) = world.get_mut::<QuantumServer>(server) {
        s.domain = Some(LoadedDomain {
            start_time: now,
            ..loaded
        });
        s.points -= domain.cost;
        s.randomized = random;
        s.complete = false;
        s.retries_spent = 0;
    }
    info!(
        "bitrunning: {server} loaded {} ({})",
        domain.key,
        if random { "random" } else { "chosen" }
    );
    say(world, client, Span::Notice, "домен загружен.");
    true
}

/// `load_domain` + `load_map_items`: the map with its safehouse onto a new level, the exits,
/// goals and the cache, initialized. `None` when the map is missing or does not load, or has no
/// exit or goal (tg `CRASH`es).
fn load_domain(world: &mut World, domain: &'static Domain) -> Option<LoadedDomain> {
    let source = world.get_resource::<MapLoadPlugin>()?.source.clone();
    let path = source.join(DOMAINS_DIR).join(format!("{}.dmm", domain.map));
    if !ss13_data::exists(&path) {
        warn!("virtual domain {}: no {}", domain.key, path.display());
        return None;
    }
    let found = RefCell::new(Prepared::default());
    let prepare = |_: &Path, map: &mut DmmMap| {
        *found.borrow_mut() = prepare_domain(&source, map);
    };
    let first_z = match ss13_mapload::load_template_with(world, &path, &prepare) {
        Ok(z) => z,
        Err(e) => {
            warn!("virtual domain {}: {e:#}", domain.key);
            ss13_world::run_map_init(world);
            return None;
        }
    };
    let found = found.into_inner();
    let at = |x: u32, y: u32, z: u32| TilePos::new(x as i32, y as i32, first_z + (z - 1) as u16);
    let mut loaded = LoadedDomain {
        domain,
        levels: (0..found.levels.max(1))
            .map(|i| first_z + i as u16)
            .collect(),
        start_time: 0.0,
        exit_turfs: Vec::new(),
        goal_turfs: Vec::new(),
    };
    let mut cache_turfs = Vec::new();
    for &(mark, x, y, z) in &found.marks {
        let pos = at(x, y, z);
        match mark {
            Mark::Exit => loaded.exit_turfs.push(pos),
            Mark::PermanentExit => {
                loaded.exit_turfs.push(pos);
                avatar::spawn_ladder(world, pos);
            }
            Mark::Goal => loaded.goal_turfs.push(pos),
            Mark::CacheSpawn => cache_turfs.push(pos),
        }
    }
    // `attempt_spawn_cache`: none to spawn when the map places its own (or a boss drops it).
    if !cache_turfs.is_empty() {
        let i = world
            .resource_mut::<crate::combat::CombatRng>()
            .range(0, cache_turfs.len() as u32 - 1) as usize;
        if let Some(pos) = validate_turf(world, cache_turfs[i]) {
            crate::construction::spawn_obj(world, ENCRYPTED_CACHE, pos);
        }
    }
    ss13_world::run_map_init(world);
    // The map's own caches and the spawned one, the megafauna.
    for e in level_entities(world, &loaded.levels) {
        let Some(path) = path_of(world, e) else {
            continue;
        };
        if is_under(&path, ENCRYPTED_CACHE) {
            make_encrypted(world, e);
        } else if MEGAFAUNA_ROOTS.iter().any(|r| is_under(&path, r)) {
            world.entity_mut(e).insert(VirtualBoss);
        }
    }
    if loaded.exit_turfs.is_empty() || loaded.goal_turfs.is_empty() {
        warn!(
            "virtual domain {}: no exit or goal turfs; scrubbed",
            domain.key
        );
        scrub_levels(world, &loaded.levels);
        return None;
    }
    Some(loaded)
}

/// `validate_turf`: the tile, or a free neighbour when something dense is on it.
fn validate_turf(world: &World, pos: TilePos) -> Option<TilePos> {
    let index = world.resource::<SpatialIndex>();
    let free = |p: TilePos| index.in_bounds(p) && index.dense_on_tile(p).is_empty();
    if free(pos) {
        return Some(pos);
    }
    // tg returns the blocked tile here (a typo): the free neighbour is what it meant.
    [
        ss13_shared::Dir::North,
        ss13_shared::Dir::South,
        ss13_shared::Dir::East,
        ss13_shared::Dir::West,
    ]
    .into_iter()
    .map(|d| pos.step(d))
    .find(|&p| free(p))
}

/// A closet entity's live state from its prototype, closed, `locked`, needing `req_access`.
fn setup_closet(world: &mut World, e: Entity, locked: bool, req_access: Vec<String>) {
    let closet = {
        let icons = world.get_resource::<IconIndex>();
        let entity = world.entity(e);
        match (entity.get::<DmCloset>(), entity.get::<DmAtom>()) {
            (Some(dm), Some(atom)) => Some(Closet::from_dm(dm, atom, icons)),
            _ => world.get::<Closet>(e).cloned(),
        }
    };
    if let Some(mut closet) = closet {
        closet.opened = false;
        closet.locked = locked;
        closet.req_access = req_access;
        world.entity_mut(e).insert(closet);
        crate::closets::refresh_look(world, e);
    }
}

/// An encrypted cache: locked for good (`can_unlock` is `FALSE`).
fn make_encrypted(world: &mut World, e: Entity) {
    world.entity_mut(e).insert(EncryptedCache);
    setup_closet(world, e, true, vec![NO_ACCESS.to_owned()]);
}

/// Everything on `levels`.
fn level_entities(world: &World, levels: &[u16]) -> Vec<Entity> {
    let index = world.resource::<SpatialIndex>();
    let mut out = Vec::new();
    for &z in levels {
        let Some((w, h)) = index.level_size(z) else {
            continue;
        };
        out.extend(index.in_rect(z, (0, 0), (w as i32 + 1, h as i32 + 1)));
    }
    out.sort();
    out.dedup();
    out
}

/// `scrub_vdom`'s release of the reservation: everything on `levels` but the turfs, areas and
/// ghosts is deleted; players there become ghosts first.
// Cost: one pass over the domain's tiles per scrub (once per boot).
fn scrub_levels(world: &mut World, levels: &[u16]) {
    for e in level_entities(world, levels) {
        let Ok(entity) = world.get_entity(e) else {
            continue;
        };
        if entity.contains::<ss13_mapload::Turf>()
            || entity.contains::<ss13_mapload::Area>()
            || entity.contains::<crate::ghost::Ghost>()
        {
            continue;
        }
        let client = entity.get::<PlayerMob>().map(|p| p.client);
        if let Some(client) = client {
            crate::ghost::ghostize_client(world, client);
        }
        world.try_despawn(e).ok();
    }
}

/// `begin_shutdown(user)`.
fn begin_shutdown(
    world: &mut World,
    client: Entity,
    actor: Entity,
    server: Entity,
    console: Entity,
) {
    let Some(state) = world.get::<QuantumServer>(server).cloned() else {
        return;
    };
    if state.domain.is_none() {
        return;
    }
    if state.connections.is_empty() {
        tell(world, client, Span::Notice, "отключение домена...");
        reset(world, server);
        return;
    }
    tell(world, client, Span::Notice, "Вы отключаете клиентов...");
    for &a in &state.connections {
        avatar::tell_mob(
            world,
            a,
            Span::Danger,
            "Перезагрузка домена. Домен перезагружается. Найдите выход.",
        );
    }
    let who = avatar::actor(world, client, actor);
    crate::tools::do_after(
        world,
        who,
        console,
        SHUTDOWN_DS,
        0,
        crate::construction::Job::Bitrunning(BitrunJob::Shutdown { server }),
    );
}

/// `reset()`: everyone out, the scrub in 15 s, the cooldown.
pub fn reset(world: &mut World, server: Entity) {
    sever_connections(world, server);
    let cooldown = COOLDOWN_SECS * f64::from(server_parts(world, server).capacitor_coefficient);
    let now = now(world);
    if let Some(mut s) = world.get_mut::<QuantumServer>(server) {
        s.is_ready = false;
        s.complete = false;
        s.scrub_at = Some(now + SCRUB_DELAY_SECS);
        s.ready_at = Some(now + cooldown.max(0.0));
        s.randomized = false;
        s.retries_spent = 0;
    }
}

/// `sever_connections()`: every avatar disconnects (with damage).
pub fn sever_connections(world: &mut World, server: Entity) {
    let avatars = world
        .get::<QuantumServer>(server)
        .map(|s| s.connections.clone())
        .unwrap_or_default();
    for a in avatars {
        avatar::full_disconnect(world, a, true);
    }
}

/// `scrub_vdom()`.
pub fn scrub(world: &mut World, server: Entity) {
    sever_connections(world, server);
    let levels = world
        .get_mut::<QuantumServer>(server)
        .and_then(|mut s| {
            s.scrub_at = None;
            s.connections.clear();
            s.domain.take()
        })
        .map(|d| d.levels);
    if let Some(levels) = levels {
        scrub_levels(world, &levels);
    }
}

/// The scrub and cooldown timers; power loss severs everyone (`on_broken`).
// Cost: one pass over the quantum servers (one per station) per tick, its parts' ratings
// included.
fn server_timers(world: &mut World) {
    let now = now(world);
    let mut query = world.query::<(Entity, &QuantumServer)>();
    let servers: Vec<(Entity, QuantumServer)> =
        query.iter(world).map(|(e, s)| (e, s.clone())).collect();
    for (e, s) in servers {
        let tier = server_parts(world, e).scanner_tier;
        if s.scanner_tier != tier
            && let Some(mut state) = world.get_mut::<QuantumServer>(e)
        {
            state.scanner_tier = tier;
        }
        if !s.connections.is_empty() && !operational(world, e) {
            sever_connections(world, e);
        }
        if s.scrub_at.is_some_and(|t| t <= now) {
            scrub(world, e);
        }
        if s.ready_at.is_some_and(|t| t <= now)
            && let Some(mut state) = world.get_mut::<QuantumServer>(e)
        {
            // `cool_off`.
            state.ready_at = None;
            state.is_ready = true;
        }
    }
}

/// `on_goal_turf_entered`: an encrypted cache moved onto a goal tile of a loaded domain.
fn deliveries(
    caches: Query<(Entity, &TilePos), (With<EncryptedCache>, Changed<TilePos>)>,
    servers: Query<(Entity, &QuantumServer)>,
    mut commands: Commands,
) {
    for (cache, &pos) in &caches {
        let Some(server) = servers
            .iter()
            .find(|(_, s)| {
                s.domain
                    .as_ref()
                    .is_some_and(|d| d.goal_turfs.contains(&pos))
            })
            .map(|(e, _)| e)
        else {
            continue;
        };
        commands.queue(move |world: &mut World| {
            generate_loot(world, server, cache);
        });
    }
}

/// `death_drops` of a virtual megafauna: the encrypted cache where it died.
fn boss_drops(
    bosses: Query<(Entity, &TilePos, &Stat), (With<VirtualBoss>, Changed<Stat>)>,
    mut commands: Commands,
) {
    for (boss, &pos, &stat) in &bosses {
        if stat != Stat::Dead {
            continue;
        }
        commands.queue(move |world: &mut World| {
            if let Ok(mut e) = world.get_entity_mut(boss) {
                e.remove::<VirtualBoss>();
            }
            if let Some(cache) = crate::construction::spawn_obj(world, ENCRYPTED_CACHE, pos) {
                make_encrypted(world, cache);
            }
        });
    }
}

/// `get_multiplayer_bonus`: every connection after the first.
fn multiplayer_bonus(connections: usize) -> f32 {
    connections.saturating_sub(1) as f32 * MULTIPLAYER_BONUS
}

/// `get_nohit_bouns`.
fn nohit_bonus(world: &World, server: &QuantumServer, domain: &Domain) -> f32 {
    if domain.flags & NO_NOHIT_BONUS != 0 {
        return 0.0;
    }
    server
        .connections
        .iter()
        .filter(|&&a| world.get::<AvatarConnection>(a).is_some_and(|c| c.nohit))
        .count() as f32
        * NOHIT_BONUS
}

/// `calculate_rewards()` (no threats here).
pub fn calculate_rewards(randomized: bool, servo_bonus: f32, multiplayer: f32, nohit: f32) -> f32 {
    0.8 + if randomized { 0.2 } else { 0.0 } + servo_bonus + multiplayer + nohit
}

/// `calculate_loot`: `max(ceil((rand() + 0.5) × (multiplier + points) × ore), 1)`.
pub fn ore_amount(unit: f32, reward_points: i32, multiplier: f32, ore: f32) -> u32 {
    let base = multiplier + reward_points as f32;
    (((unit + 0.5) * base * ore).ceil() as u32).max(1)
}

/// The parts of `calculate_rewards()` the certificate lists.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bonuses {
    pub randomized: bool,
    pub servo: f32,
    pub multiplayer: f32,
    pub nohit: f32,
}

impl Bonuses {
    pub fn total(&self) -> f32 {
        calculate_rewards(self.randomized, self.servo, self.multiplayer, self.nohit)
    }
}

/// `get_completion_certificate`.
fn certificate(domain: &Domain, b: Bonuses, secs: f64, grade: Grade) -> String {
    let Bonuses {
        randomized,
        servo,
        multiplayer,
        nohit,
    } = b;
    let bonus = b.total();
    let base_points = domain.reward_points - i32::from(randomized);
    let rnd = if randomized {
        " (Случайно)"
    } else {
        ""
    };
    let mut text = format!(
        "# Сертификат о прохождении домена\n\n---\n\n### {}{rnd}\n- **Сложность:** {}\n- \
         **Угрозы:** 0\n- **Начальная награда:** {base_points}{}\n\n- **Общий бонус:** \
         {bonus}x\n\n",
        domain.name,
        domain.difficulty,
        if randomized { " +1" } else { "" },
    );
    let time = format!(
        "### Completion Time: {}\n",
        crate::jobs::display_time_text(secs)
    );
    let rating = format!("\n---\n\n# Rating: {}", grade.letter());
    if bonus > 1.0 {
        text.push_str("### Бонусы\n");
        if randomized {
            text.push_str("- **Случайно:** + 0.2\n");
        }
        if multiplayer > 0.0 {
            text.push_str(&format!("- **Множитель:** + {multiplayer}\n"));
        }
        if nohit > 0.0 {
            text.push_str(&format!("- **Без урона:** + {nohit}\n"));
        }
        if servo > 0.2 {
            text.push_str(&format!("- **Компоненты:** + {servo}\n"));
        }
    }
    text.push_str(&time);
    text.push_str(&rating);
    text
}

/// `generate_loot(cache, chosen_forge)`: the cache is gone, the server is paid, the crate comes
/// out of a byteforge near the server. Nothing without a byteforge (tg returns before it).
pub fn generate_loot(world: &mut World, server: Entity, cache: Entity) -> bool {
    let (Some(state), Some(&server_pos), Some(&cache_pos)) = (
        world.get::<QuantumServer>(server).cloned(),
        world.get::<TilePos>(server),
        world.get::<TilePos>(cache),
    ) else {
        return false;
    };
    let Some(loaded) = state.domain.clone() else {
        return false;
    };
    if world.get::<EncryptedCache>(cache).is_none() || !loaded.goal_turfs.contains(&cache_pos) {
        return false;
    }
    let Some(forge) = nearby::<Byteforge>(world, server_pos, MAX_DISTANCE) else {
        return false;
    };
    let Some(&forge_pos) = world.get::<TilePos>(forge) else {
        return false;
    };
    let domain = loaded.domain;
    crate::effects::particles::do_sparks(world, 5, false, cache_pos);
    world.try_despawn(cache).ok();
    crate::construction::tile_changed(world, cache_pos);
    let bonuses = Bonuses {
        randomized: state.randomized,
        servo: server_parts(world, server).servo_bonus,
        multiplayer: multiplayer_bonus(state.connections.len()),
        nohit: nohit_bonus(world, &state, domain),
    };
    let bonus = bonuses.total();
    let secs = now(world) - loaded.start_time;
    let score = completion_score(domain, 0, secs);
    let grade = Grade::of_score(score);
    if let Some(mut s) = world.get_mut::<QuantumServer>(server) {
        s.points += domain.reward_points;
        s.threat += score;
        s.complete = true;
    }
    {
        let mut grades = world.resource_mut::<DomainGrades>();
        let best = grades.0.entry(domain.key).or_default();
        *best = (*best).max(grade);
    }
    let text = certificate(domain, bonuses, secs, grade);
    if let Some(reward) = crate::construction::spawn_obj(world, DECRYPTED_CACHE, forge_pos) {
        setup_closet(world, reward, false, Vec::new());
        fill_reward(world, reward, domain, bonus, &text);
    }
    crate::effects::particles::do_sparks(world, 5, true, forge_pos);
    for &a in &state.connections {
        avatar::tell_mob(
            world,
            a,
            Span::Notice,
            "Домен пройден: тайник расшифрован. Можно безопасно выйти.",
        );
    }
    info!(
        "bitrunning: {server} completed {} (grade {})",
        domain.key,
        grade.letter()
    );
    true
}

/// `PopulateContents` of the decrypted cache and its certificate.
fn fill_reward(world: &mut World, reward: Entity, domain: &Domain, multiplier: f32, text: &str) {
    for &(path, amount) in domain.completion_loot {
        for _ in 0..amount {
            put_new(world, reward, path, None);
        }
    }
    for (path, ore, needs) in ORES {
        if domain.reward_points <= needs {
            continue;
        }
        let unit = world.resource_mut::<crate::combat::CombatRng>().unit();
        let mut left = ore_amount(unit, domain.reward_points, multiplier, ore);
        while left > 0 {
            let here = left.min(50);
            put_new(world, reward, path, Some(here));
            left -= here;
        }
    }
    if let Some(paper) = put_new(world, reward, PAPER, None) {
        world.entity_mut(paper).insert(DmName {
            name: "Сертификат о прохождении домена".to_owned(),
            desc: String::new(),
        });
        crate::service::write_printed(world, paper, text);
    }
}

/// A new `path` inside `container` (`amount` for a stack).
fn put_new(
    world: &mut World,
    container: Entity,
    path: &str,
    amount: Option<u32>,
) -> Option<Entity> {
    let item = crate::construction::spawn_detached(world, path)?;
    world
        .entity_mut(item)
        .insert((Networked, StoredIn(container)));
    if let Some(amount) = amount {
        world.entity_mut(item).insert(Stack { amount });
    }
    Some(item)
}

/// End of a [`BitrunJob`] `do_after` (`crate::tools` checked that neither side moved).
pub fn complete(world: &mut World, mob: Entity, action: &crate::tools::DoAfter, job: BitrunJob) {
    match job {
        BitrunJob::Shutdown { server } => {
            if world
                .get::<QuantumServer>(server)
                .is_some_and(|s| s.domain.is_some() && s.is_ready)
            {
                reset(world, server);
            }
        }
        BitrunJob::Connect { pod } => avatar::complete_connect(world, mob, pod),
        BitrunJob::Ladder => avatar::complete_ladder(world, mob, action.target),
        BitrunJob::PryPod => avatar::complete_pry(world, action.target),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOMAIN: &str = r#""a" = (/turf/open/floor,/area/virtual_domain)
"r" = (/obj/modular_map_root/safehouse{key = "test_only"},/turf/template_noop,/area/virtual_domain/safehouse)
"c" = (/obj/effect/landmark/bitrunning/cache_spawn,/turf/open/floor,/area/virtual_domain)

(1,1,1) = {"
aaaa
acaa
raaa
"}
"#;

    const HOUSE: &str = r#""e" = (/obj/effect/landmark/bitrunning/hololadder_spawn,/turf/open/floor,/area/virtual_domain/safehouse)
"g" = (/obj/effect/landmark/bitrunning/cache_goal_turf,/turf/open/floor,/area/virtual_domain/safehouse)

(1,1,1) = {"
e
g
"}
"#;

    #[test]
    fn the_safehouse_is_pasted_and_landmarks_taken_out() {
        let dir = std::env::temp_dir().join(format!("bitrun_prepare_{}", std::process::id()));
        std::fs::create_dir_all(dir.join(SAFEHOUSES_DIR)).unwrap();
        std::fs::write(
            dir.join(SAFEHOUSES_DIR).join("test_only_safehouse.dmm"),
            HOUSE,
        )
        .unwrap();
        let mut map = DmmMap::parse(DOMAIN).unwrap();
        let prepared = prepare_domain(&dir, &mut map);
        assert_eq!(prepared.levels, 1);
        assert_eq!(prepared.safehouses, vec!["test_only".to_owned()]);
        let mut marks = prepared.marks.clone();
        marks.sort_by_key(|&(_, x, y, _)| (x, y));
        // The house's rows from its bottom-left corner on the root (1, 1): the goal at y 1.
        assert_eq!(
            marks,
            vec![
                (Mark::Goal, 1, 1, 1),
                (Mark::Exit, 1, 2, 1),
                (Mark::CacheSpawn, 2, 2, 1),
            ]
        );
        for prefab in &map.prefabs {
            assert!(
                prefab
                    .objects
                    .iter()
                    .all(|o| !is_under(&o.path, LANDMARK_ROOT) && !is_under(&o.path, MODULAR_ROOT))
            );
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn grades_and_rewards_follow_tg() {
        let drake = domains::by_key("ash_drake").unwrap();
        // Reward 4 + time 10 × (difficulty 2 + 1).
        assert_eq!(completion_score(drake, 0, 50.0), 34);
        assert_eq!(Grade::of_score(34), Grade::S);
        let beach = domains::by_key("beach_bar").unwrap();
        // Reward 1 + time 1 × 1.
        assert_eq!(completion_score(beach, 0, 3600.0), 2);
        assert_eq!(Grade::of_score(2), Grade::D);
        assert_eq!(Grade::of_score(12), Grade::A);
        assert!(Grade::S > Grade::A && Grade::D > Grade::None);
        assert_eq!(time_score(120.0), 5);
        assert_eq!(time_score(301.0), 2);
        // Two runners, one unhurt, a random domain, tier 1 servos (2 × 0.1).
        let bonus = calculate_rewards(true, 0.2, multiplayer_bonus(2), NOHIT_BONUS);
        assert!((bonus - (0.8 + 0.2 + 0.2 + 1.1 + 0.8)).abs() < 1e-5);
        assert_eq!(multiplayer_bonus(0), 0.0);
        assert_eq!(multiplayer_bonus(1), 0.0);
        // (0.5 + 0.5) × (1 + 1) × 3 iron.
        assert_eq!(ore_amount(0.5, 1, 1.0, 3.0), 6);
        assert_eq!(ore_amount(0.0, 1, 0.0, 0.2), 1);
        assert_eq!(safehouse_file("wood"), "wood.dmm");
    }

    #[test]
    fn hidden_domains_are_redacted() {
        let list = available_domains(1, 0, None);
        assert!(list.iter().all(|d| !d.id.starts_with("test_only")));
        let vapor = list.iter().find(|d| d.id == "vaporwave").unwrap();
        assert_eq!(vapor.name, REDACTED);
        assert_eq!(vapor.reward, REDACTED);
        let beach = list.iter().find(|d| d.id == "beach_bar").unwrap();
        assert_eq!(beach.name, "Бар на пляже");
        assert_eq!(beach.reward, "1");
        assert_eq!(beach.grade_symbol, "minus");
        assert_eq!(domains::DIFFICULTY_MEDIUM, 2);
    }
}
