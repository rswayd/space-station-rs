//! Nuclear operatives (server) after tg `/datum/antagonist/nukeop` (+ `/leader`),
//! `/datum/team/nuclear`, `/datum/dynamic_ruleset/roundstart/nukies`, `/datum/outfit/syndicate`,
//! `/obj/item/disk/nuclear` (`/datum/component/stationloving`), `/obj/item/pinpointer/nuke` and
//! the infiltrator (`/obj/docking_port/mobile/infiltrator`, `/obj/machinery/computer/shuttle/
//! syndicate`).
//!
//! - **Where they start.** tg loads the nukie base template and flies the infiltrator to it at
//!   round start. Here the infiltrator template (`_maps/shuttles/infiltrator/infiltrator_basic.dmm`)
//!   loads with the station onto its own level, like the emergency shuttle, and that level is the
//!   syndicate base: the operatives spawn aboard, its `syndicate_away` dock is "home"
//!   (`is_infiltrator_docked_at_syndiebase`). Its console (`crate::shuttle_nav`) flies it
//!   (ignition 5.5 s, transit 10 s, tg's mobile port defaults) to the station's `syndicate_*`
//!   docks and back.
//! - **Roundstart** ([`spawn_operatives`]): the ruleset's players get no station job (tg
//!   `forced_occupations` = nuclear operative); the first is the leader (tg: the most
//!   experienced). Each gets a body aboard, the alias "<alias> <team name>" (the leader with a
//!   title), `/datum/outfit/syndicate` (or `/leader`): ID with syndicate access, syndicate
//!   headset locked to the syndicate frequency, and a nuclear uplink in the backpack with 25 TC
//!   plus `CEILING(players / 5, 5)`; the team objective "nuke the station"; the bomb's code (the
//!   team's `memorized_code`, also on the leader's paper). The leader also holds the war
//!   declaration item ([`super::war`]).
//! - **Clown operatives** ([`spawn_operatives_as`] with `clown`, the clown rulesets): the clown
//!   outfits and job, the clown leader's titles, the clown uplink (`UPLINK_CLOWN_OPS`) and war
//!   declaration; a bananium bomb's detonation makes clowns ([`make_into_clown`]).
//! - **Other kinds** ([`add_op`]): reinforcements ([`super::reinforcements`]), the lone
//!   operative with its own team ([`LoneopTeam`], [`super::lone`]); the battlecruiser crew is
//!   [`super::battlecruiser`].
//! - **The disk** ([`NukeDisk`]): the station's real disk; out of bounds (tg `atom_in_bounds`) or
//!   gone, it reappears on a random station floor.
//! - **The result** ([`compute_result`], tg `get_result`) and the round-end report
//!   ([`roundend_report`]). A detonation on the station ends the round (`check_finished`).
// shortcut: no nukie base template (no elevator, armory, war room, reinforcement beacons): the
// infiltrator's level stands in for it; upgrade when lazy templates load at runtime.
// shortcut: the team name is a random last name (tg asks the leader, defaulting to one); the
// leader is the first selected player (tg: most playtime); no team discounts, implants, MOD
// suits, species switch or skillchips; the operatives see only their own antag HUD icon.
// shortcut: clown operatives fly the basic infiltrator with its syndicate bomb (the clown
// infiltrator template with the bananium bomb is not loaded), are not clumsy (no clumsy
// mutation) and get no comedy metabolism or sad trombone implant.

use std::collections::HashMap;
use std::path::PathBuf;

use bevy::prelude::*;
use ss13_mapload::antag::{AntagData, UPLINK_NUKE_OPS};
use ss13_mapload::jobs::Outfit;
use ss13_mapload::spawner::Rng;
use ss13_mapload::{
    Areas, DmAtom, DmItem, DmName, IconIndex, MapLoadPlugin, ShuttleTemplates, Template,
    read_manifest,
};
use ss13_net::chat::server::SendChat;
use ss13_net::server::{ClientToken, Networked};
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::{Appearance, Dir, Overlay, TilePos};
use ss13_tgui::server::OpenUi;
use ss13_world::SpatialIndex;

use super::bomb::{Detonation, NuclearBomb, NuclearBombPlugin, classify, is_space_area};
use crate::antag::objectives::RoundEnd;
use crate::antag::uplink::PurchaseLog;
use crate::antag::{AntagHudData, AntagHudUi};
use crate::construction::{path_of, turf_at};
use crate::health::{DamageType, Health, Stat, apply_damage};
use crate::inventory::{CarriedBy, IdCard, Inventory};
use crate::outfit::IdRegistration;
use crate::server::PlayerMob;
use crate::shuttle::{
    EmergencyShuttle, MobileDock, ShuttleMode, ordered_turfs, setup_template_shuttle,
};

/// tg `/datum/antagonist/nukeop` `name`.
pub const NUKEOP_NAME: &str = "Ядерный оперативник";
/// `/datum/antagonist/nukeop/leader` `name`.
pub const LEADER_NAME: &str = "Лидер ядерных оперативников";
/// `/datum/objective/nuclear` `explanation_text`.
pub const NUCLEAR_OBJECTIVE: &str =
    "Уничтожьте станцию \u{200b}\u{200b}с помощью ядерного устройства.";
/// `/datum/outfit/syndicate` `tc`.
pub const OUTFIT_TC: u32 = 25;
const ANTAG_HUD: (&str, &str) = ("icons/mob/huds/antag_hud.dmi", "synd");
const INFILTRATOR_PORT: &str = "/obj/docking_port/mobile/infiltrator";
const INFILTRATOR_ID: &str = "syndicate";
const AWAY_DOCK: &str = "syndicate_away";
const DISK_PATH: &str = "/obj/item/disk/nuclear";
const PAPER: &str = "/obj/item/paper";
const NUCLEAR_UPLINK: &str = "/obj/item/uplink/nuclear";
const LONEOP_UPLINK: &str = "/obj/item/uplink/loneop";
const CLOWNOP_UPLINK: &str = "/obj/item/uplink/clownop";
/// `/obj/machinery/nuclearbomb/selfdestruct`: the lone operative's and battlecruiser's target.
pub const SELFDESTRUCT: &str = "/obj/machinery/nuclearbomb/selfdestruct";
/// `nukeop_outfit` of each kind.
pub const OUTFIT: &str = "/datum/outfit/syndicate";
pub const OUTFIT_LEADER: &str = "/datum/outfit/syndicate/leader";
pub const OUTFIT_LONEOP: &str = "/datum/outfit/syndicate/full/loneop";
pub const OUTFIT_CLOWN: &str = "/datum/outfit/syndicate/clownop";
pub const OUTFIT_CLOWN_LEADER: &str = "/datum/outfit/syndicate/clownop/leader";
pub const OUTFIT_CLOWN_REINFORCEMENT: &str = "/datum/outfit/syndicate/clownop/no_crystals";
/// `/datum/antagonist/nukeop/lone` `name`.
pub const LONE_NAME: &str = "Одинокий оперативник";
/// `/datum/antagonist/nukeop/reinforcement` `name`.
pub const REINFORCEMENT_NAME: &str = "Поддержка ядерных оперативников";
/// `/datum/antagonist/nukeop/clownop` `name` (`ROLE_CLOWN_OPERATIVE`).
pub const CLOWNOP_NAME: &str = "Clown Operative";
/// `/datum/antagonist/nukeop/leader/clownop/give_alias` titles.
const CLOWN_LEADER_TITLES: [&str; 4] = ["Head Honker", "Slipmaster", "Clown King", "Honkbearer"];
const PINPOINTER: &str = "/obj/item/pinpointer/nuke";
const SYNDICATE_PINPOINTER: &str = "/obj/item/pinpointer/nuke/syndicate";
/// `/datum/antagonist/nukeop/leader/give_alias` titles.
const LEADER_TITLES: [&str; 7] = [
    "Царь",
    "Босс",
    "Командир",
    "Шеф",
    "Главарь",
    "Директор",
    "Повелитель",
];
/// Station areas a relocated disk may land in.
const STATION_AREA: &str = "/area/station";
/// `/obj/item/pinpointer` `close_range`, `medium_range`.
const CLOSE_RANGE: i32 = 8;
const MEDIUM_RANGE: i32 = 16;
/// tg `SSfastprocess` wait: pinpointers refresh this often.
const PINPOINTER_SECS: f64 = 0.2;

/// Insert before `GameServerPlugin` to override where the infiltrator template comes from.
#[derive(Resource, Clone, Debug, Default)]
pub struct NukeopConfig {
    /// `None`: `_maps/shuttles/infiltrator/infiltrator_basic.dmm` in the BYOND checkout.
    pub infiltrator: Option<PathBuf>,
}

/// `/datum/team/nuclear`.
#[derive(Resource, Clone, Debug, Default)]
pub struct NukeTeam {
    pub syndicate_name: String,
    pub members: Vec<Entity>,
    pub leader: Option<Entity>,
    pub memorized_code: Option<String>,
    pub tracked_nuke: Option<Entity>,
    /// Clown operatives (`/datum/antagonist/nukeop/clownop` and its leader and reinforcements).
    pub clown: bool,
}

/// `/datum/team/nuclear/loneop`: the lone operative's own team, its bomb the station's
/// self-destruct.
#[derive(Resource, Clone, Debug, Default)]
pub struct LoneopTeam(pub NukeTeam);

/// Which `/datum/antagonist/nukeop` subtype.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OpKind {
    #[default]
    Operative,
    Leader,
    /// `/reinforcement`: no bonus telecrystals, not sent to the base.
    Reinforcement,
    /// `/lone`: its own team, the self-destruct's code.
    Lone,
}

/// The operative antagonist datum, on its body.
#[derive(Component, Clone, Debug)]
pub struct Nukeop {
    pub key: String,
    pub leader: bool,
    /// `antag_memory`.
    pub memory: String,
    pub kind: OpKind,
    /// A member of [`LoneopTeam`].
    pub lone_team: bool,
}

/// The station's real nuclear authentication disk (tg `real_nuclear_disks`).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct NukeDisk;

/// tg `GLOB.station_was_nuked` and `GLOB.station_nuke_source`.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NukeOutcome {
    pub station_was_nuked: bool,
    pub station_nuke_source: Option<Detonation>,
}

/// The infiltrator's flight lives on its port (`crate::shuttle_nav::NavShuttle`).
pub use crate::shuttle_nav::Flight;

/// The syndicate infiltrator (`SSshuttle.getShuttle("syndicate")`).
#[derive(Resource, Clone, Debug)]
pub struct Infiltrator {
    pub port: Entity,
    pub away: Entity,
    pub transit: Entity,
    /// The level standing in for the syndicate base.
    pub level_z: u16,
    pub transit_z: u16,
}

impl Infiltrator {
    /// Its flight (`crate::shuttle_nav::NavShuttle`).
    pub fn flight(&self, world: &World) -> Flight {
        crate::shuttle_nav::flight(world, self.port)
    }
}

/// A pinpointer (`/obj/item/pinpointer/nuke`).
#[derive(Component, Clone, Debug, Default)]
pub struct Pinpointer {
    pub active: bool,
    /// tg `TRACK_INFILTRATOR` (syndicate pinpointers while a bomb counts down), else the disk.
    pub track_infiltrator: bool,
    pub syndicate: bool,
    pub alert: bool,
}

/// The templates of what the operatives get, and the converted data.
#[derive(Resource, Default)]
struct NukeopData {
    outfits: Vec<Outfit>,
    aliases: Vec<String>,
    last_names: Vec<String>,
    templates: HashMap<String, Template>,
}

#[derive(Resource)]
struct NukeopRng(Rng);

#[derive(Resource)]
struct InfiltratorTemplate(PathBuf);

pub struct NukeopPlugin;

impl Plugin for NukeopPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // The nuclear uplink opens, a pinpointer toggles.
        app.add_click_handler(ClickPhase::AttackSelf, 1500, |w, c| {
            crate::antag::nukeop::server::attack_self(w, c.client, c.mob, c.target)
        });
        if !app.world().contains_resource::<NukeopConfig>() {
            app.init_resource::<NukeopConfig>();
        }
        let config = app.world().resource::<NukeopConfig>().clone();
        let template = config.infiltrator.or_else(|| {
            let map = app.world().get_resource::<MapLoadPlugin>()?;
            // Only with a real station map: fixture maps name `.dmm` files.
            map.maps.first().filter(|m| !m.ends_with(".dmm"))?;
            let path = map
                .source
                .join("_maps/shuttles/infiltrator/infiltrator_basic.dmm");
            ss13_data::exists(&path).then_some(path)
        });
        if let Some(template) = template {
            info!("infiltrator template {}", template.display());
            let world = app.world_mut();
            match world.get_resource_mut::<ShuttleTemplates>() {
                Some(mut templates) => templates.0.push(template.clone()),
                None => {
                    world.insert_resource(ShuttleTemplates(vec![template.clone()]));
                }
            }
            app.insert_resource(InfiltratorTemplate(template));
        }
        app.add_plugins(NuclearBombPlugin)
            .init_resource::<NukeOutcome>()
            .init_resource::<super::war::WarState>()
            .add_systems(
                Startup,
                (
                    load_data,
                    setup_infiltrator.after(crate::shuttle::setup_shuttle),
                    setup_world,
                ),
            )
            .add_systems(
                Update,
                (
                    watch_disk,
                    super::lone::disk_security,
                    tick_pinpointers,
                    super::war::tick_war,
                    super::war::answers
                        .in_set(ss13_tgui::server::UiSystems::Handle)
                        .after(ss13_tgui::prompt::PromptSystems::Answer),
                ),
            );
    }
}

fn load_data(world: &mut World) {
    let seed = world
        .get_resource::<ss13_mapload::MapSeed>()
        .map_or(0, |s| s.0);
    world.insert_resource(NukeopRng(Rng::new(seed ^ 0x6e75_6b65)));
    let Some(assets) = world
        .get_resource::<MapLoadPlugin>()
        .map(|m| m.assets.clone())
    else {
        world.init_resource::<NukeopData>();
        return;
    };
    let data = AntagData::read(&assets.join(ss13_mapload::antag::ANTAG_FILE)).unwrap_or_default();
    let manifest = read_manifest(&assets).unwrap_or_default();
    let index = manifest.index();
    let mut templates = HashMap::new();
    for path in data.nukeop_types() {
        let proto = world
            .get_resource::<ss13_proto::Prototypes>()
            .and_then(|p| p.get(path).cloned());
        if let Some(proto) = proto {
            templates.insert(path.to_owned(), Template::new(proto, &index));
        }
    }
    info!(
        "nuclear operatives: {} outfits, {} item templates",
        data.nukeop_outfits.len(),
        templates.len()
    );
    world.insert_resource(NukeopData {
        outfits: data.nukeop_outfits.clone(),
        aliases: data.operative_aliases.clone(),
        last_names: data.last_names.clone(),
        templates,
    });
}

/// Disks and pinpointers on the maps (the console: `crate::shuttle_nav`).
fn setup_world(world: &mut World) {
    let mut items = world.query::<(Entity, &DmItem)>();
    let found: Vec<(Entity, String)> = items
        .iter(world)
        .map(|(e, i)| (e, i.path.clone()))
        .filter(|(_, p)| p == DISK_PATH || ss13_mapload::fidelity::is_under(p, PINPOINTER))
        .collect();
    for (e, path) in found {
        if path == DISK_PATH {
            world.entity_mut(e).insert(NukeDisk);
        } else {
            world.entity_mut(e).insert(Pinpointer {
                syndicate: ss13_mapload::fidelity::is_under(&path, SYNDICATE_PINPOINTER),
                ..default()
            });
        }
    }
}

/// `/datum/antagonist/nukeop/spawn_infiltrator` for a template loaded with the station.
fn setup_infiltrator(world: &mut World) {
    let Some(template) = world
        .get_resource::<InfiltratorTemplate>()
        .map(|t| t.0.clone())
    else {
        return;
    };
    let Some(shuttle) = setup_template_shuttle(
        world,
        &template,
        INFILTRATOR_PORT,
        INFILTRATOR_ID,
        AWAY_DOCK,
        "syndicate base",
    ) else {
        return;
    };
    info!(
        "infiltrator {} at z{} (transit z{})",
        shuttle.name, shuttle.level_z, shuttle.transit_z
    );
    world.insert_resource(Infiltrator {
        port: shuttle.port,
        away: shuttle.away,
        transit: shuttle.transit,
        level_z: shuttle.level_z,
        transit_z: shuttle.transit_z,
    });
    world
        .entity_mut(shuttle.port)
        .insert(crate::shuttle_nav::NavShuttle::new(shuttle.transit));
}

pub(crate) use crate::tools::client_of;

pub(crate) fn chat_to(
    world: &mut World,
    client: Option<Entity>,
    span: Span,
    text: impl Into<String>,
) {
    if let Some(client) = client {
        world.write_message(SendChat {
            clients: vec![client],
            message: ChatMessage::simple(ChatChannel::System, span, text),
        });
    }
}

fn to_everyone(world: &mut World, span: Span, text: &str) {
    let mut q = world.query_filtered::<Entity, With<ClientToken>>();
    let clients: Vec<Entity> = q.iter(world).collect();
    world.write_message(SendChat {
        clients,
        message: ChatMessage::simple(ChatChannel::System, span, text),
    });
}

/// The operatives' random generator, taken out for a caller that also needs the world.
pub(super) fn take_rng(world: &mut World) -> Rng {
    world
        .remove_resource::<NukeopRng>()
        .map_or(Rng::new(0), |r| r.0)
}

pub(super) fn put_rng(world: &mut World, rng: Rng) {
    world.insert_resource(NukeopRng(rng));
}

/// `SSpolling.poll_ghost_candidates(role)` without the poll: the first ghost (by entity order)
/// whose client opted in to `role`. Returns the ghost and its client.
// shortcut: no 15 s poll window; the opted-in ghost is taken at once, as the midround rulesets
// do (`crate::antag::midround`).
pub fn willing_ghost(world: &mut World, role: &str) -> Option<(Entity, Entity)> {
    let mut ghosts: Vec<(Entity, Entity)> = world
        .query_filtered::<(Entity, &PlayerMob), With<crate::ghost::Ghost>>()
        .iter(world)
        .map(|(g, p)| (g, p.client))
        .collect();
    ghosts.sort();
    ghosts.into_iter().find(|&(_, c)| {
        world
            .get::<crate::antag::dynamic::AntagPreferences>(c)
            .is_some_and(|p| p.wants(role))
    })
}

/// tg `syndicate_name()`.
pub fn syndicate_name(rng: &mut Rng) -> String {
    let pick = |rng: &mut Rng, list: &[&str]| {
        list[rng.range(0, list.len() as i64 - 1) as usize].to_owned()
    };
    let mut name = pick(
        rng,
        &[
            "Clandestine",
            "Prima",
            "Blue",
            "Zero-G",
            "Max",
            "Blasto",
            "North",
            "Omni",
            "Newton",
            "Cyber",
            "Bonk",
            "Gene",
            "Gib",
        ],
    );
    if rng.prob(80.0) {
        name.push(' ');
        if rng.prob(60.0) {
            name += &pick(
                rng,
                &[
                    "Syndicate",
                    "Consortium",
                    "Collective",
                    "Corporation",
                    "Group",
                    "Holdings",
                    "Biotech",
                    "Industries",
                    "Systems",
                    "Products",
                    "Chemicals",
                    "Enterprises",
                    "Family",
                    "Creations",
                    "International",
                    "Intergalactic",
                    "Interplanetary",
                    "Foundation",
                    "Positronics",
                    "Hive",
                ],
            );
        } else {
            name += &pick(
                rng,
                &[
                    "Syndi", "Corp", "Bio", "System", "Prod", "Chem", "Inter", "Hive",
                ],
            );
            name += &pick(rng, &["", "-"]);
            name += &pick(rng, &["Tech", "Co", "Tek", "X", "Inc", "Code"]);
        }
    } else {
        name += &pick(rng, &["-", "*", ""]);
        name += &pick(
            rng,
            &[
                "Tech", "Co", "Tek", "X", "Inc", "Gen", "Star", "Dyne", "Code", "Hive",
            ],
        );
    }
    name
}

/// tg `random_nukecode()`.
pub fn random_nukecode(rng: &mut Rng) -> String {
    format!("{:05}", rng.range(0, 99_999))
}

/// tg `/datum/antagonist/nukeop/on_gain` bonus: `CEILING(joined / 5, 5)`.
pub fn bonus_tc(joined: u32) -> u32 {
    joined.div_ceil(5).div_ceil(5) * 5
}

/// Free open floor aboard the infiltrator, hallway first (tg reinforcements: EVA, hallway).
fn infiltrator_spawns(world: &World) -> Vec<TilePos> {
    let Some(infiltrator) = world.get_resource::<Infiltrator>() else {
        return Vec::new();
    };
    let (Some(mobile), Some(&port_pos)) = (
        world.get::<MobileDock>(infiltrator.port),
        world.get::<TilePos>(infiltrator.port),
    ) else {
        return Vec::new();
    };
    let index = world.resource::<SpatialIndex>();
    let areas = world.resource::<Areas>();
    let mut tiles: Vec<(bool, TilePos)> = ordered_turfs(port_pos, mobile.dir, mobile.size)
        .into_iter()
        .filter(|&p| index.in_bounds(p) && index.dense_on_tile(p).is_empty())
        .filter_map(|p| {
            let area = areas.area_at(p).filter(|a| mobile.areas.contains(a))?;
            let floor = turf_at(world, p)
                .and_then(|t| path_of(world, t))
                .is_some_and(|t| ss13_mapload::fidelity::is_under(&t, "/turf/open/floor"));
            floor.then(|| {
                let hallway =
                    super::bomb::area_path(world, area).is_some_and(|a| a.ends_with("/hallway"));
                (!hallway, p)
            })
        })
        .collect();
    tiles.sort_by_key(|&(h, p)| (h, p.z, p.y, p.x));
    tiles.into_iter().map(|(_, p)| p).collect()
}

/// The ruleset's `execute()` for the players it selected: bodies aboard the infiltrator, the
/// team, the leader first.
pub fn spawn_operatives(world: &mut World, clients: &[Entity]) {
    spawn_operatives_as(world, clients, false);
}

/// [`spawn_operatives`]; `clown` for the clown operatives' rulesets
/// (`/datum/dynamic_ruleset/*/nukies/clown`).
pub fn spawn_operatives_as(world: &mut World, clients: &[Entity], clown: bool) {
    if clients.is_empty() {
        return;
    }
    let spawns = infiltrator_spawns(world);
    spawn_team(world, clown);
    let mut rng = world
        .remove_resource::<NukeopRng>()
        .map_or(Rng::new(0), |r| r.0);
    let joined = world
        .query_filtered::<(), With<PlayerMob>>()
        .iter(world)
        .count() as u32
        + clients.len() as u32;
    for (i, &client) in clients.iter().enumerate() {
        let pos = if spawns.is_empty() {
            crate::server::pick_latejoin(world)
        } else {
            crate::server::pick_free(world, &spawns, i).1
        };
        let mob = crate::server::spawn_mob(world, client, pos, String::new());
        world.entity_mut(client).remove::<crate::round::NewPlayer>();
        // `forced_occupations`: the operative (leader) job.
        let job = match (clown, i == 0) {
            (false, true) => crate::noncrew_jobs::NUCLEAR_OPERATIVE_LEADER,
            (false, false) => crate::noncrew_jobs::NUCLEAR_OPERATIVE,
            (true, true) => CLOWN_OPERATIVE_LEADER_JOB,
            (true, false) => CLOWN_OPERATIVE_JOB,
        };
        crate::noncrew_jobs::set_assigned_role(world, mob, job);
        add_nukeop(world, mob, i == 0, joined, &mut rng);
    }
    world.insert_resource(NukeopRng(rng));
}

/// Dev (`SS13_DEV_TRAITOR=nukeop`): a joined crew body becomes an operative aboard the
/// infiltrator (a dev spawn on the infiltrator's level is kept), its job gear replaced; the
/// first one makes the team and leads it.
pub fn dev_nukeop(world: &mut World, client: Entity, mob: Entity) {
    let leader = world.get_resource::<NukeTeam>().is_none();
    let mut rng = world
        .remove_resource::<NukeopRng>()
        .map_or(Rng::new(0), |r| r.0);
    if leader {
        world.insert_resource(NukeopRng(rng));
        spawn_team(world, false);
        rng = world
            .remove_resource::<NukeopRng>()
            .map_or(Rng::new(0), |r| r.0);
    }
    if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
        let items: Vec<Entity> = inventory
            .equipped()
            .chain(inventory.hands.iter().flatten().copied())
            .collect();
        for item in items {
            world.despawn(item);
        }
    }
    crate::noncrew_jobs::set_assigned_role(world, mob, crate::noncrew_jobs::NUCLEAR_OPERATIVE);
    let spawns = infiltrator_spawns(world);
    let level = world.get_resource::<Infiltrator>().map(|i| i.level_z);
    let kept = crate::server::dev_spawn(world, client).is_some_and(|p| Some(p.z) == level);
    if !spawns.is_empty() && !kept {
        let n = world.resource::<NukeTeam>().members.len();
        let pos = crate::server::pick_free(world, &spawns, n).1;
        world.entity_mut(mob).insert(pos);
    }
    let joined = world
        .query_filtered::<(), With<PlayerMob>>()
        .iter(world)
        .count() as u32;
    add_nukeop(world, mob, leader, joined, &mut rng);
    world.insert_resource(NukeopRng(rng));
}

/// `/datum/job/nuclear_operative/clown_operative` (+ `/leader`).
pub const CLOWN_OPERATIVE_JOB: &str = "/datum/job/nuclear_operative/clown_operative";
pub const CLOWN_OPERATIVE_LEADER_JOB: &str = "/datum/job/nuclear_operative/clown_operative/leader";
/// `/datum/job/lone_operative`.
pub const LONE_OPERATIVE_JOB: &str = "/datum/job/lone_operative";

/// `/datum/team/nuclear/New` + `assign_nuke`.
fn spawn_team(world: &mut World, clown: bool) {
    let mut rng = world
        .remove_resource::<NukeopRng>()
        .map_or(Rng::new(0), |r| r.0);
    let last_names = world
        .get_resource::<NukeopData>()
        .map(|d| d.last_names.clone())
        .unwrap_or_default();
    // `/datum/team/nuclear/New` names the team; the leader renames it at once (`ask_name`,
    // whose default is a random last name).
    let mut team = NukeTeam {
        syndicate_name: syndicate_name(&mut rng),
        clown,
        ..default()
    };
    if !last_names.is_empty() {
        team.syndicate_name =
            last_names[rng.range(0, last_names.len() as i64 - 1) as usize].clone();
    }
    let mut bombs = world.query::<(Entity, &NuclearBomb)>();
    let nuke = bombs
        .iter(world)
        .filter(|(_, b)| b.syndicate)
        .map(|(e, _)| e)
        .min();
    if let Some(nuke) = nuke {
        let code = random_nukecode(&mut rng);
        let mut bomb = world.get_mut::<NuclearBomb>(nuke).expect("queried");
        if bomb.state.r_code == super::bomb::NUKE_CODE_UNSET {
            bomb.state.r_code = code.clone();
            team.memorized_code = Some(code);
        } else {
            team.memorized_code = Some(bomb.state.r_code.clone());
        }
        team.tracked_nuke = Some(nuke);
    }
    world.insert_resource(team);
    world.insert_resource(NukeopRng(rng));
}

/// The self-destruct terminal's entity (`/obj/machinery/nuclearbomb/selfdestruct`).
pub fn selfdestruct(world: &mut World) -> Option<Entity> {
    let mut bombs = world.query::<(Entity, &NuclearBomb, &ss13_mapload::DmObj)>();
    bombs
        .iter(world)
        .filter(|(_, _, o)| o.path == SELFDESTRUCT)
        .map(|(e, ..)| e)
        .min()
}

/// `/datum/team/nuclear/loneop/New` + `assign_nuke`: a syndicate name, the self-destruct's
/// code (BandaStation gives every bomb a random code at init).
pub fn spawn_lone_team(world: &mut World) {
    if world.contains_resource::<LoneopTeam>() {
        return;
    }
    let mut rng = world
        .remove_resource::<NukeopRng>()
        .map_or(Rng::new(0), |r| r.0);
    let mut team = NukeTeam {
        syndicate_name: syndicate_name(&mut rng),
        ..default()
    };
    if let Some(nuke) = selfdestruct(world) {
        let code = random_nukecode(&mut rng);
        let mut bomb = world.get_mut::<NuclearBomb>(nuke).expect("queried");
        if bomb.state.r_code == super::bomb::NUKE_CODE_UNSET {
            bomb.state.r_code = code;
        }
        team.memorized_code = Some(bomb.state.r_code.clone());
        team.tracked_nuke = Some(nuke);
    }
    world.insert_resource(LoneopTeam(team));
    world.insert_resource(NukeopRng(rng));
}

/// The lone operative's own team (`lone`), else the nuclear team.
pub fn team_of(world: &World, lone: bool) -> Option<&NukeTeam> {
    if lone {
        world.get_resource::<LoneopTeam>().map(|t| &t.0)
    } else {
        world.get_resource::<NukeTeam>()
    }
}

fn with_team(world: &mut World, lone: bool, f: impl FnOnce(&mut NukeTeam)) {
    if lone {
        if let Some(mut team) = world.get_resource_mut::<LoneopTeam>() {
            f(&mut team.0);
        }
    } else if let Some(mut team) = world.get_resource_mut::<NukeTeam>() {
        f(&mut team);
    }
}

/// `nukeop_outfit` of `kind` in a (clown) team.
pub fn outfit_path(kind: OpKind, clown: bool) -> &'static str {
    match (kind, clown) {
        (OpKind::Lone, _) => OUTFIT_LONEOP,
        (OpKind::Operative, false) => OUTFIT,
        (OpKind::Leader, false) => OUTFIT_LEADER,
        // `/datum/outfit/syndicate/reinforcement` subtypes are picked by the beacon.
        (OpKind::Reinforcement, false) => OUTFIT,
        (OpKind::Operative, true) => OUTFIT_CLOWN,
        (OpKind::Leader, true) => OUTFIT_CLOWN_LEADER,
        (OpKind::Reinforcement, true) => OUTFIT_CLOWN_REINFORCEMENT,
    }
}

/// The outfit's `tc` (25; a reinforcement's 0) plus `give_bonus_tc`'s `CEILING(joined / 5, 5)`
/// for the roundstart operatives.
pub fn starting_tc(kind: OpKind, joined: u32) -> u32 {
    match kind {
        OpKind::Operative | OpKind::Leader => OUTFIT_TC + bonus_tc(joined),
        OpKind::Lone => OUTFIT_TC,
        OpKind::Reinforcement => 0,
    }
}

/// `/datum/outfit/syndicate` `uplink_type`'s flag.
pub fn uplink_of(kind: OpKind, clown: bool) -> (&'static str, u32) {
    use ss13_mapload::antag::{UPLINK_CLOWN_OPS, UPLINK_LONE_OP};
    match (kind, clown) {
        (OpKind::Lone, _) => (LONEOP_UPLINK, UPLINK_LONE_OP),
        (_, true) => (CLOWNOP_UPLINK, UPLINK_CLOWN_OPS),
        _ => (NUCLEAR_UPLINK, UPLINK_NUKE_OPS),
    }
}

/// `add_antag_datum(/datum/antagonist/nukeop[/leader])` on a fresh body: `give_alias`,
/// `forge_objectives`, `equip_op`, the bonus TC, `memorize_code`, `greet`.
pub fn add_nukeop(world: &mut World, mob: Entity, leader: bool, joined: u32, rng: &mut Rng) {
    let kind = if leader {
        OpKind::Leader
    } else {
        OpKind::Operative
    };
    add_op(world, mob, kind, false, joined, None, rng);
}

/// [`add_nukeop`] of any kind; `lone_team` joins the lone operative's team (the lone
/// operative, its reinforcements); `outfit` overrides the kind's (a reinforcement's subtype).
pub fn add_op(
    world: &mut World,
    mob: Entity,
    kind: OpKind,
    lone_team: bool,
    joined: u32,
    outfit: Option<&str>,
    rng: &mut Rng,
) {
    let leader = kind == OpKind::Leader;
    let clown = !lone_team && world.get_resource::<NukeTeam>().is_some_and(|t| t.clown);
    let outfit_path = outfit
        .unwrap_or_else(|| outfit_path(kind, clown))
        .to_owned();
    let client = client_of(world, mob);
    let key = client
        .and_then(|c| world.get::<ClientToken>(c))
        .map(|t| t.0.clone())
        .unwrap_or_default();
    let (aliases, outfit) = world
        .get_resource::<NukeopData>()
        .map(|d| {
            (
                d.aliases.clone(),
                d.outfits
                    .iter()
                    .find(|o| o.path == outfit_path)
                    .or(d.outfits.first())
                    .cloned(),
            )
        })
        .unwrap_or_default();
    let syndicate_name = team_of(world, lone_team)
        .map(|t| t.syndicate_name.clone())
        .unwrap_or_default();
    let alias = if aliases.is_empty() {
        "Агент".to_owned()
    } else {
        aliases[rng.range(0, aliases.len() as i64 - 1) as usize].clone()
    };
    let mut name = format!("{alias} {syndicate_name}");
    let title = match (leader, clown) {
        (false, _) => "",
        (true, true) => {
            CLOWN_LEADER_TITLES[rng.range(0, CLOWN_LEADER_TITLES.len() as i64 - 1) as usize]
        }
        (true, false) => LEADER_TITLES[rng.range(0, LEADER_TITLES.len() as i64 - 1) as usize],
    };
    if leader {
        name = format!("{title} {name}");
    }
    world.entity_mut(mob).insert(Name::new(name.clone()));
    with_team(world, lone_team, |team| {
        team.members.push(mob);
        if leader {
            team.leader = Some(mob);
        }
    });
    if let Some(outfit) = outfit {
        let (uplink, flag) = uplink_of(kind, clown);
        equip_op(
            world,
            mob,
            &outfit,
            &name,
            leader,
            clown,
            (uplink, flag, starting_tc(kind, joined)),
        );
    }
    let hud_icon = world
        .get_resource::<IconIndex>()
        .and_then(|index| index.get(ANTAG_HUD.0, ANTAG_HUD.1));
    let code = team_of(world, lone_team).and_then(|t| t.memorized_code.clone());
    let mut memory = String::new();
    if let Some(code) = &code {
        memory = format!("Код от ядерной боеголовки: {code}");
    }
    world.entity_mut(mob).insert((
        Nukeop {
            key,
            leader,
            memory,
            kind,
            lone_team,
        },
        AntagHudData {
            hud_icon,
            ..default()
        },
    ));
    // `greet`.
    if leader {
        chat_to(
            world,
            client,
            Span::Warning,
            format!(
                "Вы - {title} отряда Синдиката для этой задачи. Вы несете ответственность за \
                 руководство своей командой."
            ),
        );
        chat_to(
            world,
            client,
            Span::Warning,
            "Если вы чувствуете, что не справляетесь с этой задачей, обменяйте свой наушник у \
             другого оперативника.",
        );
        chat_to(
            world,
            client,
            Span::Warning,
            "В вашей руке найдете специальный предмет, что повысит сложность для вашей команды. \
             Внимательно изучите его и обсудите план с вашей командой, прежде чем активируете \
             его.",
        );
    } else {
        chat_to(
            world,
            client,
            Span::Bold,
            format!("Вы - агент {syndicate_name}!"),
        );
    }
    if let Some(faction) = outfit_path.strip_prefix("/datum/outfit/syndicate/reinforcement/") {
        // `/datum/outfit/syndicate/reinforcement/post_equip`.
        chat_to(
            world,
            client,
            Span::Notice,
            format!(
                "Вы - агент {}, посланный сопровождать ядерный отряд в его задании. \
                 Поддерживайте союзников и помните: долой Нанотрейзен.",
                reinforcement_faction(faction)
            ),
        );
    }
    chat_to(world, client, Span::Notice, "Ваши текущие задачи:");
    if let Some(client) = client {
        world.write_message(SendChat {
            clients: vec![client],
            message: ChatMessage {
                channel: ChatChannel::System,
                segments: vec![
                    ChatSegment::new(Span::Bold, "Задача #1"),
                    ChatSegment::new(Span::Plain, format!(": {NUCLEAR_OBJECTIVE}")),
                ],
            },
        });
    }
    if let Some(code) = code {
        chat_to(
            world,
            client,
            Span::Plain,
            format!("Код ядерной аутентификации: {code}"),
        );
        if leader {
            give_item(world, mob, PAPER, |world, paper| {
                if let Some(mut n) = world.get_mut::<DmName>(paper) {
                    n.name = "коды ядерной аутентификации".into();
                    n.desc = format!("Код активации ядерной боеголовки: {code}");
                }
                crate::service::write_printed(
                    world,
                    paper,
                    format!("Код активации ядерной боеголовки: {code}"),
                );
            });
        }
    }
    if leader {
        let challenge = if clown {
            super::war::CHALLENGE_CLOWN
        } else {
            super::war::CHALLENGE
        };
        give_item(world, mob, challenge, |_, _| {});
    }
    if let Some(client) = client {
        world.write_message(OpenUi::new::<AntagHudUi>(client, mob, mob, ""));
    }
    info!("{mob} is a nuclear operative ({kind:?}, clown: {clown})");
}

/// `/datum/outfit/syndicate/reinforcement/*` `faction`.
pub fn reinforcement_faction(subtype: &str) -> &'static str {
    match subtype {
        "gorlex" => "Мародёров Горлекса",
        "cybersun" => "Cybersun Industries",
        "donk" => "Donk Company",
        "waffle" => "Waffle Corporation",
        "interdyne" => "Interdyne Pharmaceutics",
        "mi13" => "MI13",
        _ => "Синдиката",
    }
}

/// Spawns `path` into `mob`'s free hand (tg `put_in_hands`), else at its feet.
fn give_item(world: &mut World, mob: Entity, path: &str, then: impl FnOnce(&mut World, Entity)) {
    let item = world.resource_scope(|world, data: Mut<NukeopData>| {
        Some(data.templates.get(path)?.spawn(world, None))
    });
    let Some(item) = item else {
        warn!("no prototype for {path}");
        return;
    };
    crate::storage::finish_item(world, item, 0);
    then(world, item);
    let hand = world.get::<Inventory>(mob).and_then(|inv| {
        [inv.active, 1 - inv.active]
            .into_iter()
            .find(|&h| inv.hands[h].is_none())
    });
    match hand {
        Some(h) => {
            world.entity_mut(item).insert((Networked, CarriedBy(mob)));
            world.get_mut::<Inventory>(mob).expect("checked").hands[h] = Some(item);
        }
        None => {
            let pos = world.get::<TilePos>(mob).copied().unwrap_or_default();
            world.entity_mut(item).insert((Networked, pos));
        }
    }
}

/// Spawns `path` into `pack` (`equip_to_storage(del_on_fail = TRUE)`).
fn into_pack(world: &mut World, pack: Entity, path: &str) -> Option<Entity> {
    let item = world.resource_scope(|world, data: Mut<NukeopData>| {
        Some(data.templates.get(path)?.spawn(world, None))
    })?;
    crate::storage::finish_item(world, item, 1);
    if crate::storage::can_insert(world, pack, item).is_err() {
        world.despawn(item);
        return None;
    }
    world.entity_mut(item).insert(Networked);
    crate::storage::put_inside(world, item, pack);
    Some(item)
}

/// `equip_op` with `/datum/outfit/syndicate` and its `post_equip`; `uplink` is the
/// `uplink_type`, its flag and the outfit's `tc`.
fn equip_op(
    world: &mut World,
    mob: Entity,
    outfit: &Outfit,
    name: &str,
    leader: bool,
    clown: bool,
    uplink: (&str, u32, u32),
) {
    let assignment = match (clown, leader) {
        (false, true) => "Syndicate Operative Leader",
        (false, false) => "Syndicate Operative",
        (true, true) => "Syndicate Entertainment Operative Leader",
        (true, false) => "Syndicate Entertainment Operative",
    };
    let mut inventory = world.resource_scope(|world, data: Mut<NukeopData>| {
        crate::outfit::equip_outfit(world, mob, outfit, &data.templates)
    });
    if let Some(card) = inventory.id {
        // `/datum/id_trim/chameleon/operative[/nuke_leader]`.
        let mut access = vec!["syndicate".to_owned(), "maint_tunnels".to_owned()];
        if leader {
            access.push("syndicate_leader".into());
        }
        let label = crate::outfit::id_label(name, assignment);
        let mut card = world.entity_mut(card);
        card.insert((
            IdCard { access },
            IdRegistration {
                registered_name: name.to_owned(),
                assignment: assignment.to_owned(),
                job: assignment.to_owned(),
            },
        ));
        if let Some(mut n) = card.get_mut::<DmName>() {
            n.name = label;
        }
    }
    // `post_equip`: the headset on the syndicate frequency.
    if let Some(ears) = inventory.ears
        && let Some(mut radio) = world.get_mut::<crate::radio::Radio>(ears)
    {
        radio.frequency = ss13_chat::radio::FREQ_SYNDICATE;
    }
    // shortcut: storage's grey backpack, as every job gets (`crate::jobs::join`).
    inventory.back = crate::storage::spawn_backpack(world, mob);
    world.entity_mut(mob).insert(inventory.clone());
    crate::modpc::pda::post_equip(world, mob, name, assignment);
    if let Some(pack) = inventory.back {
        for (path, count) in &outfit.backpack_contents {
            for _ in 0..*count {
                into_pack(world, pack, path);
            }
        }
        let (uplink_path, flag, tc) = uplink;
        if let Some(device) = into_pack(world, pack, uplink_path) {
            crate::antag::uplink::give_op_uplink(world, mob, device, tc, flag);
        }
        // shortcut: the base's pinpointers come with the operative.
        if let Some(p) = into_pack(world, pack, SYNDICATE_PINPOINTER) {
            world.entity_mut(p).insert(Pinpointer {
                syndicate: true,
                ..default()
            });
        }
    }
    if world.get::<PurchaseLog>(mob).is_none() {
        world.entity_mut(mob).insert(PurchaseLog::default());
    }
}

/// tg `attack_self` of the operatives' items: the nuclear uplink opens, the pinpointer toggles,
/// the war declaration item explains itself. `false` for other items.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if world
        .get::<crate::antag::uplink::Uplink>(item)
        .is_some_and(|u| u.uplink_flag & ss13_mapload::antag::UPLINK_ALL_SYNDIE_OPS != 0)
    {
        crate::antag::uplink::open_uplink(world, client, mob, item);
        return true;
    }
    if let Some(mut p) = world.get_mut::<Pinpointer>(item) {
        p.active = !p.active;
        let active = p.active;
        if !active {
            set_pinpointer_overlay(world, item, None);
        }
        chat_to(
            world,
            Some(client),
            Span::Notice,
            if active {
                "Вы активируете пинпоинтер."
            } else {
                "Вы деактивируете пинпоинтер."
            },
        );
        return true;
    }
    if super::war::is_challenge(world, item) {
        super::war::use_challenge(world, client, mob, item);
        return true;
    }
    if super::reinforcements::is_beacon(world, item) {
        super::reinforcements::use_beacon(world, client, mob, item);
        return true;
    }
    false
}

/// `arm_nuke`: syndicate pinpointers switch to the infiltrator.
pub(crate) fn bomb_armed(world: &mut World, _bomb: Entity) {
    let mut q = world.query::<&mut Pinpointer>();
    for mut p in q.iter_mut(world) {
        if p.syndicate {
            p.track_infiltrator = true;
        }
    }
}

/// `explode`: everyone hears the alarm.
pub(crate) fn bomb_exploding(world: &mut World, _bomb: Entity) {
    to_everyone(
        world,
        Span::BoldAnnounce,
        "Ядерное устройство активировано! Взрыв неизбежен!",
    );
}

/// `actually_explode` + `really_actually_explode`.
pub(crate) fn actually_explode(world: &mut World, bomb: Entity) {
    let Some(pos) = world.get::<TilePos>(bomb).copied() else {
        return;
    };
    let station = crate::events::controller::station_levels(world).contains(&pos.z);
    let base = world
        .get_resource::<Infiltrator>()
        .is_some_and(|i| i.level_z == pos.z);
    let size = world
        .resource::<SpatialIndex>()
        .level_size(pos.z)
        .unwrap_or((255, 255));
    let status = classify(pos, station, is_space_area(world, pos), base, size);
    info!("nuclear detonation at {pos:?}: {status:?}");
    {
        let mut outcome = world.resource_mut::<NukeOutcome>();
        outcome.station_nuke_source = Some(status);
        if status == Detonation::HitStation {
            outcome.station_was_nuked = true;
        }
    }
    let gib_z: Vec<u16> = match status {
        Detonation::HitStation => crate::events::controller::station_levels(world).collect(),
        Detonation::HitSyndieBase => vec![pos.z],
        _ => Vec::new(),
    };
    match status {
        Detonation::HitStation => {}
        Detonation::HitSyndieBase => crate::shuttle::priority_announce(
            world,
            "Приоритетное оповещение",
            "Командование ядерных операций",
            "Сканеры дальнего действия показывают, что ядерное устройство взорвалось на ранее \
             неизвестной базе, мы предполагаем,что база имеет синдикатовское происхождение. \
             Хорошая работа.",
        ),
        _ => crate::shuttle::priority_announce(
            world,
            "Приоритетное оповещение",
            "Командование ядерных операций",
            "Сканеры дальнего действия показывают, что ядерное устройство взорвалось; однако \
             сейсмическая активность на станции минимальна. Мы предполагаем, что устройство \
             взорвалось не на самой станции.",
        ),
    }
    // `nuke_gib` on every living mob of the hit levels.
    // shortcut: the mob dies where it stood (tg gibs it; no freezer survival).
    let mut mobs = world.query::<(Entity, &TilePos, &Health, &Stat)>();
    let victims: Vec<Entity> = mobs
        .iter(world)
        .filter(|(_, p, _, s)| gib_z.contains(&p.z) && **s != Stat::Dead)
        .map(|(e, ..)| e)
        .collect();
    // The bananium bomb's `nuke_effects`: everyone there becomes a clown instead.
    let bananium = world
        .get::<ss13_mapload::DmObj>(bomb)
        .is_some_and(|o| ss13_mapload::fidelity::is_under(&o.path, super::bomb::BANANIUM_NUKE));
    if bananium {
        for e in victims.iter().copied() {
            make_into_clown(world, e);
        }
    }
    for e in victims.into_iter().filter(|_| !bananium) {
        let (Some(mut health), Some(mut stat)) = (
            world.get::<Health>(e).cloned(),
            world.get::<Stat>(e).copied(),
        ) else {
            continue;
        };
        apply_damage(&mut health, &mut stat, DamageType::Brute, 1000.0);
        world.entity_mut(e).insert((health, stat));
        chat_to(
            world,
            client_of(world, e),
            Span::Danger,
            "Вас разрывает на атомы ядерным взрывом!",
        );
    }
    // `qdel(src)`: the disk inside is not deleted (stationloving), it drops out first.
    if let Some(disk) = world.get::<NuclearBomb>(bomb).and_then(|b| b.disk) {
        crate::storage::take_out(world, disk, pos);
    }
    world.despawn(bomb);
}

/// What `make_into_clown` dresses a victim in, by slot.
const CLOWN_CLOTHES: [(ss13_shared::protocol::InventorySlot, &str); 3] = [
    (
        ss13_shared::protocol::InventorySlot::Uniform,
        "/obj/item/clothing/under/rank/civilian/clown",
    ),
    (
        ss13_shared::protocol::InventorySlot::Shoes,
        "/obj/item/clothing/shoes/clown_shoes",
    ),
    (
        ss13_shared::protocol::InventorySlot::Mask,
        "/obj/item/clothing/mask/gas/clown_hat",
    ),
];

/// `make_into_clown`: a human's jumpsuit, shoes and mask drop and clown clothes take their
/// place.
// shortcut: the clothes can be taken off (no `TRAIT_NODROP`), no clumsiness (no clumsy
// mutation), no clown phobia and no stun.
pub fn make_into_clown(world: &mut World, mob: Entity) {
    if world.get::<crate::human::HumanLook>(mob).is_none() || world.get::<Inventory>(mob).is_none()
    {
        return;
    }
    let pos = world.get::<TilePos>(mob).copied().unwrap_or_default();
    for (slot, path) in CLOWN_CLOTHES {
        let worn = world
            .get_mut::<Inventory>(mob)
            .and_then(|mut inv| *inv.slot_mut(slot));
        if let Some(old) = worn {
            crate::inventory::unequip(world, mob, pos, slot);
            crate::inventory::put_down(world, old, pos);
        }
        let item = world.resource_scope(|world, data: Mut<NukeopData>| {
            Some(data.templates.get(path)?.spawn(world, None))
        });
        let Some(item) = item else {
            continue;
        };
        crate::storage::finish_item(world, item, 0);
        world.entity_mut(item).insert((Networked, CarriedBy(mob)));
        if let Some(mut inv) = world.get_mut::<Inventory>(mob) {
            *inv.slot_mut(slot) = Some(item);
        }
    }
    chat_to(world, client_of(world, mob), Span::Danger, "ХОНК!");
}

/// tg `check_finished`: the station was nuked.
pub fn station_destroyed(world: &World) -> bool {
    world
        .get_resource::<NukeOutcome>()
        .is_some_and(|o| o.station_was_nuked)
}

/// tg `NUKE_RESULT_*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NukeResult {
    Fluke,
    NukeWin,
    NoSurvivors,
    WrongStation,
    WrongStationDead,
    HijackDisk,
    HijackNoDisk,
    CrewWinSyndiesDead,
    CrewWin,
    DiskLost,
    DiskStolen,
    Interrupted,
}

/// What `get_result` reads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResultInputs {
    pub shuttle_evacuated: bool,
    pub shuttle_landed_base: bool,
    pub disk_rescued: bool,
    pub syndies_didnt_escape: bool,
    pub team_is_dead: bool,
    pub station_was_nuked: bool,
    pub station_nuke_source: Option<Detonation>,
}

/// `/datum/team/nuclear/proc/get_result`.
pub fn compute_result(i: ResultInputs) -> NukeResult {
    if i.station_nuke_source == Some(Detonation::HitSyndieBase) {
        return NukeResult::Fluke;
    }
    if i.station_was_nuked {
        return if i.syndies_didnt_escape {
            NukeResult::NoSurvivors
        } else {
            NukeResult::NukeWin
        };
    }
    if i.station_nuke_source.is_some() && !i.disk_rescued {
        return if i.syndies_didnt_escape {
            NukeResult::WrongStationDead
        } else {
            NukeResult::WrongStation
        };
    }
    if i.shuttle_landed_base {
        return if i.disk_rescued {
            NukeResult::HijackDisk
        } else {
            NukeResult::HijackNoDisk
        };
    }
    if i.disk_rescued {
        return if i.shuttle_evacuated && i.team_is_dead {
            NukeResult::CrewWinSyndiesDead
        } else {
            NukeResult::CrewWin
        };
    }
    if i.team_is_dead {
        NukeResult::DiskLost
    } else if i.shuttle_evacuated {
        NukeResult::DiskStolen
    } else {
        // tg CRASHes here; the report prints "Миссия прервана!".
        NukeResult::Interrupted
    }
}

/// `is_disk_rescued`.
fn disk_rescued(world: &mut World) -> bool {
    let mut q = world.query_filtered::<Entity, With<NukeDisk>>();
    let disks: Vec<Entity> = q.iter(world).collect();
    let shuttle = world.get_resource::<EmergencyShuttle>();
    let station = crate::events::controller::station_levels(world);
    disks.into_iter().all(|disk| {
        let Some(pos) = crate::modpc::pda::turf_of(world, disk) else {
            return false;
        };
        match shuttle.map(|s| (s.state.mode, s)) {
            Some((ShuttleMode::Escape, s)) => {
                let area = world.resource::<Areas>().area_at(pos);
                area.is_some_and(|a| {
                    world
                        .get::<MobileDock>(s.port)
                        .is_some_and(|m| m.areas.contains(&a))
                })
            }
            Some((ShuttleMode::Endgame, s)) => pos.z == s.centcom_z,
            _ => station.contains(&pos.z),
        }
    })
}

/// `are_all_operatives_dead`.
fn team_dead(world: &World, team: &NukeTeam) -> bool {
    team.members
        .iter()
        .all(|&m| world.get::<Stat>(m).is_none_or(|s| *s == Stat::Dead))
}

/// `is_infiltrator_docked_at_syndiebase`.
pub fn infiltrator_home(world: &World) -> bool {
    world.get_resource::<Infiltrator>().is_some_and(|i| {
        world
            .get::<MobileDock>(i.port)
            .is_some_and(|m| m.docked == Some(i.away))
    })
}

/// `get_result` against the world.
pub fn get_result(world: &mut World) -> Option<NukeResult> {
    let team = world.get_resource::<NukeTeam>()?.clone();
    Some(result_for(world, &team))
}

/// `get_result` of `team`.
fn result_for(world: &mut World, team: &NukeTeam) -> NukeResult {
    let outcome = *world.resource::<NukeOutcome>();
    let inputs = ResultInputs {
        shuttle_evacuated: crate::shuttle::evacuated(world),
        shuttle_landed_base: crate::shuttle::hijacked(world),
        disk_rescued: disk_rescued(world),
        syndies_didnt_escape: !infiltrator_home(world),
        team_is_dead: team_dead(world, team),
        station_was_nuked: outcome.station_was_nuked,
        station_nuke_source: outcome.station_nuke_source,
    };
    compute_result(inputs)
}

/// `/datum/team/nuclear/roundend_report`.
pub fn roundend_report(world: &mut World, _end: RoundEnd) -> Vec<ChatMessage> {
    let Some(team) = world.get_resource::<NukeTeam>().cloned() else {
        return Vec::new();
    };
    report_for(world, &team)
}

/// The lone operative's team's `roundend_report` (`/datum/team/nuclear/loneop`).
pub fn lone_roundend_report(world: &mut World, _end: RoundEnd) -> Vec<ChatMessage> {
    let Some(team) = world.get_resource::<LoneopTeam>().map(|t| t.0.clone()) else {
        return Vec::new();
    };
    report_for(world, &team)
}

fn report_for(world: &mut World, team: &NukeTeam) -> Vec<ChatMessage> {
    let result = result_for(world, team);
    let station = crate::round::station_name(world);
    let name = &team.syndicate_name;
    let line = |segments: Vec<ChatSegment>| ChatMessage {
        channel: ChatChannel::System,
        segments,
    };
    let (span, title, text) = match result {
        NukeResult::Fluke => (
            Span::RedText,
            "Унизительное поражение Синдиката!".to_owned(),
            format!(
                "Экипаж станции {station} вернули оперативникам {name} бомбу обратно на их базу! \
                 База Синдиката была уничтожена! В следующий раз - не теряйте ядерную бомбу!"
            ),
        ),
        NukeResult::NukeWin => (
            Span::GreenText,
            "Разгромная победа Синдиката!".to_owned(),
            format!("Оперативники {name} уничтожили {station}!"),
        ),
        NukeResult::NoSurvivors => (
            Span::Bold,
            "Полное уничтожение!".to_owned(),
            format!(
                "Оперативники {name} уничтожили {station}, но не смогли покинуть станцию вовремя \
                 и попали под взрыв. В следующий раз - не теряйте диск!"
            ),
        ),
        NukeResult::WrongStation => (
            Span::RedText,
            "Незначительная победа экипажа!".to_owned(),
            format!(
                "Оперативники {name} сохранили диск аутентификации, но взорвали то, что не \
                 являлось {station}. В следующий раз - не делайте так!"
            ),
        ),
        NukeResult::WrongStationDead => (
            Span::RedText,
            format!("Оперативники {name} получили премию Дарвина!"),
            format!(
                "Оперативники {name} взорвали то, что не являлось {station} и попали под взрыв. \
                 В следующий раз - не делайте так!"
            ),
        ),
        NukeResult::HijackDisk | NukeResult::HijackNoDisk => (
            Span::GreenText,
            "Незначительная победа Синдиката!".to_owned(),
            format!("Оперативники {name} угнали эвакуационный шаттл."),
        ),
        NukeResult::CrewWinSyndiesDead => (
            Span::RedText,
            "Разгромная победа экипажа!".to_owned(),
            format!("Экипаж спас диск и уничтожил оперативников {name}!"),
        ),
        NukeResult::CrewWin => (
            Span::RedText,
            "Разгромная победа экипажа!".to_owned(),
            format!("Экипаж спас диск и остановил оперативников {name}!"),
        ),
        NukeResult::DiskLost => (
            Span::Bold,
            "Победа экипажа!".to_owned(),
            format!(
                "Экипаж не смог сохранить диск, но смогли уничтожить большую часть оперативников \
                 {name}!"
            ),
        ),
        NukeResult::DiskStolen => (
            Span::GreenText,
            "Незначительная победа Синдиката!".to_owned(),
            format!(
                "Оперативники {name} пережили задание, но не смогли уничтожить {station}. В \
                 следующий раз - не теряйте диск!"
            ),
        ),
        NukeResult::Interrupted => (
            Span::Bold,
            "Победа".to_owned(),
            "Миссия прервана!".to_owned(),
        ),
    };
    let mut out = vec![
        line(vec![ChatSegment::new(
            Span::Bold,
            format!("Оперативники {name}:"),
        )]),
        line(vec![ChatSegment::new(span, title)]),
        line(vec![ChatSegment::new(Span::Bold, text)]),
        line(vec![ChatSegment::new(
            Span::Bold,
            "Оперативниками Синдиката были:",
        )]),
    ];
    let mut spent = 0;
    for &member in &team.members {
        let key = world
            .get::<Nukeop>(member)
            .map(|n| n.key.clone())
            .unwrap_or_default();
        out.push(crate::antag::traitor::print_player(world, member, &key));
        spent += world
            .get::<PurchaseLog>(member)
            .map_or(0, |l| l.total_spent);
    }
    out.push(line(vec![ChatSegment::new(
        Span::Plain,
        format!("(Оперативники потратили {spent} ТК)"),
    )]));
    let done = result == NukeResult::NukeWin || result == NukeResult::NoSurvivors;
    out.push(line(vec![
        ChatSegment::new(Span::Bold, "Задача #1"),
        ChatSegment::new(Span::Plain, format!(": {NUCLEAR_OBJECTIVE} ")),
        if done {
            ChatSegment::new(Span::GreenText, "Успех!")
        } else {
            ChatSegment::new(Span::RedText, "Неудача.")
        },
    ]));
    info!("nuclear operatives: {result:?}");
    out
}

/// `/datum/dynamic_ruleset/roundstart/nukies/round_result` `mode_result`.
pub fn mode_result(result: NukeResult) -> &'static str {
    match result {
        NukeResult::Fluke => "loss - syndicate nuked - disk secured",
        NukeResult::NukeWin => "win - syndicate nuke",
        NukeResult::NoSurvivors => "halfwin - syndicate nuke - did not evacuate in time",
        NukeResult::WrongStation => "halfwin - blew wrong station",
        NukeResult::WrongStationDead => "halfwin - blew wrong station - did not evacuate in time",
        NukeResult::CrewWinSyndiesDead => "loss - evacuation - disk secured - syndi team dead",
        NukeResult::CrewWin => "loss - evacuation - disk secured",
        NukeResult::DiskLost => "halfwin - evacuation - disk not secured",
        NukeResult::DiskStolen => "halfwin - detonation averted",
        _ => "halfwin - interrupted",
    }
}

// --- The disk -------------------------------------------------------------------------------

/// `atom_in_bounds`.
fn disk_in_bounds(world: &World, pos: TilePos) -> bool {
    if crate::events::controller::station_levels(world).contains(&pos.z) {
        return true;
    }
    if world
        .get_resource::<Infiltrator>()
        .is_some_and(|i| i.level_z == pos.z)
    {
        return true;
    }
    if world
        .get_resource::<EmergencyShuttle>()
        .is_some_and(|s| s.centcom_z == pos.z)
    {
        return true;
    }
    // Transit levels: only aboard the allowed shuttles.
    world
        .resource::<Areas>()
        .area_at(pos)
        .and_then(|a| super::bomb::area_path(world, a))
        .is_some_and(|p| {
            [
                "/area/shuttle/syndicate",
                "/area/shuttle/escape",
                "/area/shuttle/pod_",
            ]
            .iter()
            .any(|root| p.starts_with(root))
        })
}

/// `get_safe_random_station_turf`: a random free floor in a station area.
fn safe_station_turf(world: &World, rng: &mut Rng) -> Option<TilePos> {
    let index = world.resource::<SpatialIndex>();
    let (w, h) = index.level_size(1)?;
    for _ in 0..500 {
        let pos = TilePos::new(
            rng.range(1, i64::from(w)) as i32,
            rng.range(1, i64::from(h)) as i32,
            1,
        );
        if !index.dense_on_tile(pos).is_empty() {
            continue;
        }
        let floor = turf_at(world, pos)
            .and_then(|t| path_of(world, t))
            .is_some_and(|t| ss13_mapload::fidelity::is_under(&t, "/turf/open/floor"));
        let station = world
            .resource::<Areas>()
            .area_at(pos)
            .and_then(|a| super::bomb::area_path(world, a))
            .is_some_and(|p| ss13_mapload::fidelity::is_under(p, STATION_AREA));
        if floor && station {
            return Some(pos);
        }
    }
    None
}

/// The tracked disk, last seen once a second: out of bounds it is relocated, gone (despawned)
/// a new one appears (tg's disk cannot be deleted).
fn watch_disk(world: &mut World, mut next: Local<f64>, mut known: Local<Option<Entity>>) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now < *next {
        return;
    }
    *next = now + 1.0;
    let mut q = world.query_filtered::<Entity, With<NukeDisk>>();
    let disk = q.iter(world).next();
    let Some(mut rng) = world.remove_resource::<NukeopRng>() else {
        return;
    };
    match disk {
        Some(disk) => {
            *known = Some(disk);
            let pos = crate::modpc::pda::turf_of(world, disk);
            if pos.is_none_or(|p| !disk_in_bounds(world, p))
                && let Some(to) = safe_station_turf(world, &mut rng.0)
            {
                if let Some(holder) = world.get::<CarriedBy>(disk).map(|c| c.0) {
                    chat_to(
                        world,
                        client_of(world, holder),
                        Span::Danger,
                        "Вы не можете отделаться от ощущения, что вы что-то потеряли...",
                    );
                }
                world
                    .entity_mut(disk)
                    .remove::<(CarriedBy, crate::storage::StoredIn)>();
                crate::storage::take_out(world, disk, to);
                info!("nuclear disk out of bounds at {pos:?}: relocated to {to:?}");
            }
        }
        None if known.is_some() => {
            let to = safe_station_turf(world, &mut rng.0);
            let disk = to.and_then(|to| {
                world.resource_scope(|world, data: Mut<NukeopData>| {
                    Some(data.templates.get(DISK_PATH)?.spawn(world, Some(to)))
                })
            });
            if let (Some(disk), Some(to)) = (disk, to) {
                world.entity_mut(disk).insert((NukeDisk, Networked));
                *known = Some(disk);
                info!("nuclear disk was destroyed: a new one at {to:?}");
            }
        }
        None => {}
    }
    world.insert_resource(rng);
}

// --- Pinpointers ----------------------------------------------------------------------------

pub(crate) fn set_pinpointer_overlay(
    world: &mut World,
    item: Entity,
    state: Option<(String, Option<Dir>)>,
) {
    let icon = world
        .get::<DmAtom>(item)
        .map(|a| a.icon.clone())
        .unwrap_or_default();
    let overlay = state.and_then(|(s, dir)| {
        let id = world.get_resource::<IconIndex>()?.get(&icon, &s)?;
        Some(Overlay {
            dir,
            ..Overlay::new(id)
        })
    });
    if let Some(mut appearance) = world.get_mut::<Appearance>(item) {
        let new: Vec<Overlay> = overlay.into_iter().collect();
        if appearance.overlays != new {
            appearance.overlays = new;
        }
    }
}

/// BYOND `get_dir`.
fn get_dir(from: TilePos, to: TilePos) -> Dir {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    match (dx.signum(), dy.signum()) {
        (0, 1) => Dir::North,
        (0, -1) => Dir::South,
        (1, 0) => Dir::East,
        (-1, 0) => Dir::West,
        (1, 1) => Dir::NorthEast,
        (-1, 1) => Dir::NorthWest,
        (1, -1) => Dir::SouthEast,
        (-1, -1) => Dir::SouthWest,
        _ => Dir::South,
    }
}

/// The overlay state of a pinpointer at `here` pointing at `there` (`update_overlays`).
pub fn pinpointer_state(
    here: TilePos,
    there: Option<TilePos>,
    alert: bool,
) -> (String, Option<Dir>) {
    let a = if alert { "alert" } else { "" };
    let Some(there) = there.filter(|t| t.z == here.z) else {
        return (format!("pinon{a}null"), None);
    };
    if there == here {
        return (format!("pinon{a}direct"), None);
    }
    let d = (there.x - here.x).abs().max((there.y - here.y).abs());
    let range = if d <= CLOSE_RANGE {
        "close"
    } else if d <= MEDIUM_RANGE {
        "medium"
    } else {
        "far"
    };
    let state = if alert {
        "pinonalert".to_owned()
    } else {
        format!("pinon{range}")
    };
    (state, Some(get_dir(here, there)))
}

/// `process` of the active pinpointers (`SSfastprocess`).
fn tick_pinpointers(world: &mut World, mut next: Local<f64>) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now < *next {
        return;
    }
    *next = now + PINPOINTER_SECS;
    let mut q = world.query::<(Entity, &Pinpointer)>();
    let active: Vec<(Entity, Pinpointer)> = q
        .iter(world)
        .filter(|(_, p)| p.active)
        .map(|(e, p)| (e, p.clone()))
        .collect();
    if active.is_empty() {
        return;
    }
    let mut disks = world.query_filtered::<Entity, With<NukeDisk>>();
    let disk = disks.iter(world).next();
    let disk_pos = disk.and_then(|d| crate::modpc::pda::turf_of(world, d));
    let infiltrator_pos = world
        .get_resource::<Infiltrator>()
        .and_then(|i| world.get::<TilePos>(i.port).copied());
    let mut bombs = world.query::<&NuclearBomb>();
    let armed = bombs.iter(world).any(|b| b.state.timing);
    for (e, mut p) in active {
        let Some(here) = crate::modpc::pda::turf_of(world, e) else {
            continue;
        };
        if armed && !p.alert {
            p.alert = true;
            if let Some(holder) = world.get::<CarriedBy>(e).map(|c| c.0) {
                chat_to(
                    world,
                    client_of(world, holder),
                    Span::Danger,
                    "Ваш пинпоинтер вибрирует и издает зловещий сигнал. Ой-ой.",
                );
            }
        }
        if !armed {
            p.alert = false;
            p.track_infiltrator = false;
        }
        let target = if p.track_infiltrator {
            infiltrator_pos
        } else {
            disk_pos
        };
        let state = pinpointer_state(here, target, p.alert);
        world.entity_mut(e).insert(p);
        set_pinpointer_overlay(world, e, Some(state));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_follows_get_result() {
        let base = ResultInputs::default();
        let nuked = ResultInputs {
            station_was_nuked: true,
            station_nuke_source: Some(Detonation::HitStation),
            ..base
        };
        assert_eq!(compute_result(nuked), NukeResult::NukeWin);
        assert_eq!(
            compute_result(ResultInputs {
                syndies_didnt_escape: true,
                ..nuked
            }),
            NukeResult::NoSurvivors
        );
        assert_eq!(
            compute_result(ResultInputs {
                station_nuke_source: Some(Detonation::HitSyndieBase),
                ..base
            }),
            NukeResult::Fluke
        );
        let missed = ResultInputs {
            station_nuke_source: Some(Detonation::NearMissedStation),
            ..base
        };
        assert_eq!(compute_result(missed), NukeResult::WrongStation);
        assert_eq!(
            compute_result(ResultInputs {
                syndies_didnt_escape: true,
                ..missed
            }),
            NukeResult::WrongStationDead
        );
        // A miss with the disk rescued falls through to the disk checks.
        assert_eq!(
            compute_result(ResultInputs {
                disk_rescued: true,
                ..missed
            }),
            NukeResult::CrewWin
        );
        let rescued = ResultInputs {
            disk_rescued: true,
            ..base
        };
        assert_eq!(compute_result(rescued), NukeResult::CrewWin);
        assert_eq!(
            compute_result(ResultInputs {
                shuttle_evacuated: true,
                team_is_dead: true,
                ..rescued
            }),
            NukeResult::CrewWinSyndiesDead
        );
        assert_eq!(
            compute_result(ResultInputs {
                team_is_dead: true,
                ..base
            }),
            NukeResult::DiskLost
        );
        assert_eq!(
            compute_result(ResultInputs {
                shuttle_evacuated: true,
                ..base
            }),
            NukeResult::DiskStolen
        );
        assert_eq!(compute_result(base), NukeResult::Interrupted);
        assert_eq!(mode_result(NukeResult::NukeWin), "win - syndicate nuke");
    }

    #[test]
    fn helpers_match_tg() {
        // `CEILING(joined / 5, 5)`.
        assert_eq!(bonus_tc(0), 0);
        assert_eq!(bonus_tc(1), 5);
        assert_eq!(bonus_tc(25), 5);
        assert_eq!(bonus_tc(26), 10);
        assert_eq!(bonus_tc(30), 10);
        let mut rng = Rng::new(7);
        for _ in 0..50 {
            let code = random_nukecode(&mut rng);
            assert_eq!(code.len(), 5);
            assert!(code.bytes().all(|b| b.is_ascii_digit()));
            assert!(!syndicate_name(&mut rng).is_empty());
        }
        let here = TilePos::new(10, 10, 1);
        assert_eq!(
            pinpointer_state(here, Some(TilePos::new(10, 15, 1)), false),
            ("pinonclose".into(), Some(Dir::North))
        );
        assert_eq!(
            pinpointer_state(here, Some(TilePos::new(30, 10, 1)), false),
            ("pinonfar".into(), Some(Dir::East))
        );
        assert_eq!(
            pinpointer_state(here, Some(TilePos::new(10, 10, 2)), true),
            ("pinonalertnull".into(), None)
        );
        assert_eq!(
            pinpointer_state(here, Some(here), false),
            ("pinondirect".into(), None)
        );
    }
}
