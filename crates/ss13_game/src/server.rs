//! Server half: replication marking, player mobs, move intents, viewer follow.
//!
//! Needs `ss13_world::WorldPlugin`, `ss13_net` (`NetServerPlugin` + `ReplicationServerPlugin`),
//! `ss13_proto::ProtoPlugin` and `ss13_mapload::MapLoadPlugin`.

use std::collections::{HashMap, VecDeque};

use bevy::ecs::entity::{EntityHashMap, EntityHashSet};
use bevy::ecs::message::MessageCursor;
use bevy::platform::time::Instant;
use bevy::prelude::*;
use ss13_assets::dmm::{DmValue, DmmMap};
use ss13_mapload::{
    DmAtom, DmName, HiddenUnderTile, MapLoadPlugin, MapLoadReport, MapSeed, Template, read_manifest,
};
use ss13_net::move_ack::server::SendMoveAck;
use ss13_net::server::{
    ClientConnected, ClientDisconnected, ClientToken, ControlledEntity, IntentReceived,
    NetServerConfig, Networked, SeeInvisible, ServerCertificateDigest, Sight, Viewer,
};
use ss13_shared::protocol::{ClientIntent, MoveAck};
use ss13_shared::{Appearance, Dir, Facing, TilePos};
use ss13_world::{MoveRequest, Mover, SpatialIndex, WorldSystems};

use crate::ghost::{Ghost, GhostStep, Ghostize};
use crate::health::{Health, Stat, living_bundle};
use crate::hud::HudData;
use crate::human::{HumanAppearances, player_body};
use crate::jobs::JobSpawns;
use crate::pull::{HeldByGrab, MoveResist};
use crate::{MAX_UNACKED_STEPS, RUN_DELAY_SECS};

/// Spawn landmarks in order of preference: round-start assistant spots (the job every new player
/// gets in tg), latejoin spots, then any job start. A tile matches a rule when one of its objects
/// is the type or a subtype. Jobs other than the assistant prefer their own start landmark
/// ([`JobSpawns`]).
const SPAWN_LANDMARKS: [&str; 3] = [
    "/obj/effect/landmark/start/assistant",
    "/obj/effect/landmark/latejoin",
    "/obj/effect/landmark/start",
];

pub struct GameServerPlugin {
    /// Prototype of player mobs (a converted DM type).
    pub mob_proto: String,
}

impl Default for GameServerPlugin {
    fn default() -> Self {
        Self {
            // Drawn by `crate::human` when it is this type or a subtype.
            mob_proto: "/mob/living/carbon/human".into(),
        }
    }
}

/// Where new players appear, recorded at map load.
#[derive(Resource, Default, Debug)]
pub struct SpawnPoints {
    pub tiles: Vec<TilePos>,
    next: usize,
    /// `/obj/effect/landmark/latejoin` (tg `SSjob.latejoin_trackers`).
    pub latejoin: Vec<TilePos>,
    next_latejoin: usize,
    /// `/obj/effect/landmark/observer_start`: where an observer from the lobby appears.
    pub observer: Option<TilePos>,
    /// Dev: `SS13_SPAWN_AT`, every body spawns here.
    pub forced: Option<TilePos>,
    /// `/obj/effect/landmark/carpspawn` (`crate::events::carp_migration`).
    pub carp: Vec<TilePos>,
    /// `/obj/effect/landmark/generic_maintenance_landmark` (tg `GLOB.generic_maintenance_landmarks`,
    /// `find_maintenance_spawn`).
    pub maintenance: Vec<TilePos>,
    /// Dev: handshake token -> spawn tile ([`DEV_SPAWNS_ENV`]).
    dev: HashMap<String, TilePos>,
}

/// Dev, debug builds only: `SS13_DEV_SPAWNS=alice=108,66;bob=98,72,1` spawns the client whose
/// handshake token is `alice` on (108,66), so scripted multi-client checks do not depend on the
/// join order. The server operator sets the tiles; a client only names its token.
pub const DEV_SPAWNS_ENV: &str = "SS13_DEV_SPAWNS";

/// `token=x,y[,z]` entries separated by `;`; malformed entries are skipped.
pub fn parse_dev_spawns(spec: &str) -> HashMap<String, TilePos> {
    spec.split(';')
        .filter_map(|entry| {
            let (token, tile) = entry.split_once('=')?;
            let v: Vec<i32> = tile
                .split(',')
                .map(|n| n.trim().parse().ok())
                .collect::<Option<_>>()?;
            let z = match v[..] {
                [_, _] => 1,
                [_, _, z] => u16::try_from(z).ok()?,
                _ => return None,
            };
            let token = token.trim();
            (!token.is_empty()).then(|| (token.to_owned(), TilePos::new(v[0], v[1], z)))
        })
        .collect()
}

/// The [`DEV_SPAWNS_ENV`] tile of `client`'s handshake token, if any.
pub(crate) fn dev_spawn(world: &World, client: Entity) -> Option<TilePos> {
    let token = world.get::<ClientToken>(client)?;
    world.resource::<SpawnPoints>().dev.get(&token.0).copied()
}

fn dev_spawns() -> HashMap<String, TilePos> {
    let Some(spec) = std::env::var(DEV_SPAWNS_ENV).ok().filter(|s| !s.is_empty()) else {
        return HashMap::new();
    };
    if !cfg!(debug_assertions) {
        warn!("{DEV_SPAWNS_ENV} is ignored: dev spawns exist in debug builds only");
        return HashMap::new();
    }
    let spawns = parse_dev_spawns(&spec);
    warn!("dev spawns by token: {spawns:?}");
    spawns
}

/// Client connection entity -> its mob.
#[derive(Resource, Default, Debug)]
pub struct Players(EntityHashMap<Entity>);

impl Players {
    pub fn mob(&self, client: Entity) -> Option<Entity> {
        self.0.get(&client).copied()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The client now controls `mob` (a ghost leaving its body).
    pub(crate) fn set_mob(&mut self, client: Entity, mob: Entity) {
        self.0.insert(client, mob);
    }
}

/// On a player's mob.
#[derive(Component, Clone, Copy, Debug)]
pub struct PlayerMob {
    pub client: Entity,
}

/// On a player's mob: the client's `Move` steps waiting for the mob's cooldown, oldest first.
/// Changes (ignoring arrivals) when a step is applied, which [`ack_moves`] answers.
#[derive(Component, Debug, Default)]
pub struct MoveQueue {
    steps: VecDeque<(u32, Dir)>,
    /// Newest `seq` received; older or repeated ones are ignored.
    received: u32,
    /// Newest `seq` applied (moved or blocked).
    applied: u32,
}

impl MoveQueue {
    pub fn applied(&self) -> u32 {
        self.applied
    }

    /// The queue of the client's next mob: the step numbering goes on, nothing waits.
    pub(crate) fn continued(&self) -> Self {
        Self {
            steps: VecDeque::new(),
            received: self.received,
            applied: self.applied,
        }
    }
}

#[derive(Resource)]
struct MobProto(String);

#[derive(Resource)]
struct MobTemplate(Template);

/// Job start landmarks: `/obj/effect/landmark/start` and subtypes, keyed by their `name`.
const START_LANDMARK: &str = "/obj/effect/landmark/start";
const OBSERVER_LANDMARK: &str = "/obj/effect/landmark/observer_start";
const CARP_LANDMARK: &str = "/obj/effect/landmark/carpspawn";
const MAINTENANCE_LANDMARK: &str = "/obj/effect/landmark/generic_maintenance_landmark";
/// tg `/obj/effect/landmark/start/ai` with `primary_ai = TRUE` (not its `/secondary`).
const PRIMARY_AI_LANDMARK: &str = "/obj/effect/landmark/start/ai";

impl Plugin for GameServerPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<ss13_tgui::server::TguiServerPlugin>() {
            app.add_plugins(ss13_tgui::server::TguiServerPlugin);
        }
        app.add_plugins((
            crate::doors::DoorServerPlugin,
            crate::inventory::InventoryServerPlugin,
            crate::click::ClickServerPlugin,
            crate::vending::VendingServerPlugin,
            crate::storage::StorageServerPlugin,
            crate::closets::ClosetServerPlugin,
            crate::health::HealthServerPlugin,
            crate::combat::CombatServerPlugin,
            crate::ghost::GhostServerPlugin,
            crate::multiz::MultiZPlugin,
            crate::power::PowerServerPlugin,
            crate::jobs::JobsServerPlugin,
            crate::atmos::AtmosGluePlugin,
        ))
        .add_plugins(crate::round::RoundServerPlugin)
        .add_plugins((
            crate::tables::TablesPlugin,
            crate::rest::RestPlugin,
            crate::furniture::FurniturePlugin,
            crate::devices::DevicesPlugin,
            crate::plumbing::PlumbingPlugin,
            crate::decor::DecorPlugin,
            crate::flora::FloraPlugin,
            crate::airlock_construction::AirlockConstructionPlugin,
        ))
        .add_plugins((
            crate::pull::PullServerPlugin,
            crate::throw::ThrowServerPlugin,
            crate::examine::ExamineServerPlugin,
            crate::speech::SpeechServerPlugin,
            crate::breath::BreathPlugin,
            crate::internals::InternalsPlugin,
            crate::tools::ToolsServerPlugin,
            crate::construction::ConstructionServerPlugin,
            crate::obj_damage::ObjDamageServerPlugin,
            crate::radio::RadioServerPlugin,
            crate::status_effects::StatusEffectsPlugin,
            crate::restraints::RestraintsPlugin,
            crate::security::SecurityServerPlugin,
            crate::medical::MedicalServerPlugin,
            crate::fire::FireServerPlugin,
        ))
        .add_plugins((
            crate::chem::ChemServerPlugin,
            crate::hunger::HungerServerPlugin,
            crate::food::FoodServerPlugin,
            crate::kitchen::KitchenServerPlugin,
            crate::botany::BotanyServerPlugin,
            crate::wires::WiresServerPlugin,
            crate::shuttle::ShuttleServerPlugin,
            crate::cargo::CargoServerPlugin,
            crate::antag::AntagServerPlugin,
            crate::modpc::pda::ModpcServerPlugin,
            crate::science::ScienceServerPlugin,
            crate::drag::DragServerPlugin,
            crate::strip::StripServerPlugin,
            crate::loot::LootServerPlugin,
            crate::point::PointServerPlugin,
        ))
        .add_plugins(crate::janitor::JanitorServerPlugin)
        .add_plugins(crate::buffs::BuffsPlugin)
        .add_plugins(crate::language_menu::LanguageMenuServerPlugin)
        .add_plugins((
            crate::ssd::SsdPlugin,
            crate::screentips::ScreentipsServerPlugin,
            crate::escape_menu::EscapeMenuServerPlugin,
        ))
        .add_plugins((
            crate::spacemove::SpaceMovePlugin,
            crate::stumble::StumblePlugin,
        ))
        .add_plugins(crate::mining::MiningServerPlugin)
        .add_plugins(crate::forensics::server::ForensicsServerPlugin)
        .add_plugins(crate::service::server::ServiceServerPlugin)
        .add_plugins(crate::fishing::server::FishingServerPlugin)
        .add_plugins(crate::martial::MartialPlugin)
        .add_plugins((
            crate::mood::MoodServerPlugin,
            crate::quirks::QuirksServerPlugin,
            crate::hallucination::HallucinationPlugin,
        ))
        .add_plugins(crate::vision::VisionServerPlugin)
        .add_plugins(crate::stock_parts::StockPartsPlugin)
        .add_plugins(crate::sound::SoundServerPlugin)
        .add_plugins(crate::atmos_machinery::AtmosMachineryPlugin)
        .add_plugins(crate::silicon::SiliconServerPlugin)
        .add_plugins(crate::records::RecordsServerPlugin)
        .add_plugins(crate::clothing::ClothingPlugin)
        .add_plugins(crate::brig::BrigServerPlugin)
        .add_plugins(crate::events::EventsServerPlugin)
        .add_plugins(crate::explosions::ExplosionsServerPlugin)
        .add_plugins(crate::radiation::RadiationPlugin)
        .add_plugins(crate::emp::EmpPlugin)
        .add_plugins(crate::emag::EmagPlugin)
        .add_plugins(crate::grenades::GrenadesServerPlugin)
        .add_plugins((crate::effects::EffectsPlugin, crate::weather::WeatherPlugin))
        .add_plugins(crate::assemblies::server::AssembliesServerPlugin)
        .add_plugins(crate::wiremod::server::WiremodServerPlugin)
        .add_plugins(crate::mobs::MobsServerPlugin)
        .add_plugins(crate::bots::BotsServerPlugin)
        .add_plugins(crate::mecha::MechaServerPlugin)
        .add_plugins(crate::vehicles::VehiclesPlugin)
        .add_plugins(crate::modsuit::server::ModSuitPlugin)
        .add_plugins(crate::disposals::DisposalsServerPlugin)
        .add_plugins(crate::genetics::GeneticsServerPlugin)
        .add_plugins(crate::wallmount::WallMountPlugin)
        .add_plugins(crate::security_level::SecurityLevelPlugin)
        .add_plugins(crate::announce::AnnouncePlugin)
        .add_plugins((
            crate::station_traits::StationTraitsPlugin,
            crate::station_goals::StationGoalsPlugin,
        ))
        .add_plugins((
            crate::implants::ImplantsPlugin,
            crate::defenses::server::DefensesPlugin,
        ))
        .add_plugins(crate::restaurant::RestaurantPlugin)
        .add_plugins((
            crate::cooking::CookingServerPlugin,
            crate::gibber::GibberServerPlugin,
            crate::coffee::CoffeeServerPlugin,
            crate::icecream::IceCreamPlugin,
            crate::stove::StovePlugin,
            crate::decay::DecayPlugin,
            crate::barsign::BarsignPlugin,
            crate::chem_dispenser::ChemDispenserPlugin,
            crate::chem_master::ChemMasterPlugin,
            crate::chem_heater::ChemHeaterPlugin,
            crate::smartfridge::SmartFridgePlugin,
        ))
        .add_plugins((
            crate::projectile::ProjectileServerPlugin,
            crate::guns::GunsServerPlugin,
        ))
        .add_plugins((
            crate::actions::ActionButtonsPlugin,
            crate::antag::vampire::server::VampirePlugin,
            crate::antag::changeling::server::ChangelingPlugin,
            crate::antag::nukeop::server::NukeopPlugin,
            crate::antag::cult::server::CultPlugin,
            crate::antag::heretic::server::HereticPlugin,
            crate::spells::SpellsPlugin,
            crate::antag::wizard::server::WizardPlugin,
        ))
        .insert_resource(MobProto(self.mob_proto.clone()))
        .init_resource::<SpawnPoints>()
        .init_resource::<Players>()
        // The map is loaded in `PreStartup`.
        .add_systems(
            Startup,
            (mark_networked, find_spawn_points, build_mob_template)
                .chain()
                .before(crate::jobs::JobsStartup),
        )
        .add_systems(
            PostStartup,
            // None on a loopback (in-memory) server.
            write_dev_cert_digest.run_if(resource_exists::<ServerCertificateDigest>),
        )
        .add_systems(
            Update,
            (handle_connections, forward_moves)
                .chain()
                .before(WorldSystems::Movement),
        )
        .add_systems(
            Update,
            (follow_mobs, ack_moves).after(WorldSystems::Movement),
        )
        .add_observer(hide_under_tile)
        .add_observer(show_from_under_tile);
    }

    fn finish(&self, app: &mut App) {
        seed_rngs(app.world_mut());
    }
}

/// Seeds the game's clock-seeded RNGs from the [`MapSeed`] (inserted by `MapLoadPlugin`:
/// `SS13_MAP_SEED` or the clock; test servers set it, `ss13_game::testing`), so one seed
/// reproduces a round's rolls: [`CombatRng`](crate::combat::CombatRng) (melee, wounds,
/// surgery, throws, pulls, fire, hunger, wires ...), [`EngineRng`](crate::power::EngineRng),
/// the records [`Manifest`](crate::records::manifest::Manifest)'s rng and chat's
/// [`SpeechRng`](ss13_chat::server::SpeechRng). The other RNGs read the map seed themselves when
/// they are created: map spawners, closet contents, wires, jobs, cargo supply and economy,
/// shuttle, dynamic (unless `DynamicConfig::seed`), random events. Without a map nothing changes.
/// Runs in `finish`, after every plugin built: a resource a test inserts after `finish` wins.
pub fn seed_rngs(world: &mut World) {
    let Some(&MapSeed(seed)) = world.get_resource::<MapSeed>() else {
        return;
    };
    world.insert_resource(crate::combat::CombatRng(seed ^ 0x636f_6d62_6174));
    world.insert_resource(crate::power::EngineRng(crate::combat::CombatRng(
        seed ^ 0x656e_6769_6e65,
    )));
    world.insert_resource(ss13_chat::server::SpeechRng(seed ^ 0x7370_6565_6368));
    world.insert_resource(ss13_chat::language::ScrambleSeed(seed ^ 0x6c61_6e67));
    world.insert_resource(crate::sound::SoundRng(crate::combat::CombatRng(
        seed ^ 0x0073_6f75_6e64,
    )));
    world.insert_resource(crate::emp::EmpRng(ss13_mapload::spawner::Rng::new(
        seed ^ 0x0065_6d70_756c_7365,
    )));
    world.insert_resource(crate::genetics::GeneticsRng(
        ss13_mapload::spawner::Rng::new(seed ^ 0x6765_6e65_7469_6373),
    ));
    if let Some(mut manifest) = world.get_resource_mut::<crate::records::manifest::Manifest>() {
        manifest.reseed(seed ^ 0x7265_636f_7264);
    }
}

/// Everything drawable on the map replicates, except what BYOND hides from players
/// (`invisibility > 0`: landmarks, markers; pipes and cables under floor tiles). One batched
/// insert after load: map load itself stays free of network types, and the batch moves each
/// archetype's entities together.
/// Entities spawned later must add `Networked` themselves.
fn mark_networked(world: &mut World) {
    let start = Instant::now();
    let mut query = world.query_filtered::<(Entity, Option<&DmAtom>), (
        With<Appearance>,
        With<TilePos>,
        Without<Networked>,
        Without<HiddenUnderTile>,
    )>();
    let mut hidden = 0usize;
    let batch: Vec<(Entity, Networked)> = query
        .iter(world)
        .filter(|(_, atom)| {
            let visible = atom.is_none_or(|a| a.invisibility == 0);
            hidden += usize::from(!visible);
            visible
        })
        .map(|(entity, _)| (entity, Networked))
        .collect();
    let marked = batch.len();
    world.insert_batch(batch);
    info!(
        "replicating {marked} entities ({hidden} invisible skipped), marked in {:.2?}",
        start.elapsed()
    );
}

/// A floor tile now covers it (`ss13_mapload::tile_changed`): clients forget it.
fn hide_under_tile(add: On<Add<HiddenUnderTile>>, mut commands: Commands) {
    if let Ok(mut entity) = commands.get_entity(add.entity) {
        entity.remove::<Networked>();
    }
}

/// The floor tile above it is gone: it replicates again.
fn show_from_under_tile(
    remove: On<Remove<HiddenUnderTile>>,
    drawable: Query<&DmAtom, (With<Appearance>, With<TilePos>)>,
    mut commands: Commands,
) {
    if drawable
        .get(remove.entity)
        .is_ok_and(|a| a.invisibility == 0)
    {
        commands.entity(remove.entity).try_insert(Networked);
    }
}

/// `/a/b` is under `/a` and `/a/b`, not under `/a/bc`.
fn is_under(path: &str, root: &str) -> bool {
    path.strip_prefix(root)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// Landmarks are not kept as entities with their DM type, so the loaded `.dmm` files are read
/// again for them.
// shortcut: a second parse of every map at startup (~tens of ms for a station); record landmark
// cells in the map loader if startup time starts to matter.
fn find_spawn_points(world: &mut World) {
    let Some(report) = world.get_resource::<MapLoadReport>() else {
        return;
    };
    let start = Instant::now();
    let mut found: [Vec<TilePos>; SPAWN_LANDMARKS.len()] = Default::default();
    let mut jobs = JobSpawns::default();
    let mut observer_start = None;
    let mut carp = Vec::new();
    let mut maintenance = Vec::new();
    let protos = world.get_resource::<ss13_proto::Prototypes>();
    for loaded in &report.maps {
        let map = match DmmMap::from_file(&loaded.path) {
            Ok(map) => map,
            Err(e) => {
                error!("spawn points: {}: {e}", loaded.path.display());
                continue;
            }
        };
        let rank: Vec<Option<usize>> = map
            .prefabs
            .iter()
            .map(|p| {
                SPAWN_LANDMARKS
                    .iter()
                    .position(|root| p.objects.iter().any(|o| is_under(&o.path, root)))
            })
            .collect();
        // The `name` of a start landmark is its job title (tg `get_default_roundstart_spawn_point`).
        let titles: Vec<Option<String>> = map
            .prefabs
            .iter()
            .map(|p| {
                let o = p
                    .objects
                    .iter()
                    .find(|o| is_under(&o.path, START_LANDMARK))?;
                let edited = o.vars.iter().find_map(|(k, v)| match v {
                    DmValue::Str(s) if k == "name" => Some(s.clone()),
                    _ => None,
                });
                edited.or_else(|| Some(protos?.get(&o.path)?.get::<DmName>()?.name.clone()))
            })
            .collect();
        let primary_ai: Vec<bool> = map
            .prefabs
            .iter()
            .map(|p| p.objects.iter().any(|o| o.path == PRIMARY_AI_LANDMARK))
            .collect();
        let observer: Vec<bool> = map
            .prefabs
            .iter()
            .map(|p| {
                p.objects
                    .iter()
                    .any(|o| is_under(&o.path, OBSERVER_LANDMARK))
            })
            .collect();
        let carp_marks: Vec<bool> = map
            .prefabs
            .iter()
            .map(|p| p.objects.iter().any(|o| is_under(&o.path, CARP_LANDMARK)))
            .collect();
        let maint_marks: Vec<bool> = map
            .prefabs
            .iter()
            .map(|p| {
                p.objects
                    .iter()
                    .any(|o| is_under(&o.path, MAINTENANCE_LANDMARK))
            })
            .collect();
        let (sx, sy, sz) = map.size;
        for mz in 1..=sz {
            for y in 1..=sy {
                for x in 1..=sx {
                    let Some(i) = map.index(x, y, mz).map(|i| i as usize) else {
                        continue;
                    };
                    let pos = TilePos::new(x as i32, y as i32, loaded.first_z + (mz - 1) as u16);
                    if let Some(r) = rank[i] {
                        found[r].push(pos);
                    }
                    if let Some(title) = &titles[i] {
                        jobs.by_title.entry(title.clone()).or_default().push(pos);
                    }
                    if primary_ai[i] {
                        jobs.primary_ai.push(pos);
                    }
                    if carp_marks[i] {
                        carp.push(pos);
                    }
                    if maint_marks[i] {
                        maintenance.push(pos);
                    }
                    if observer[i] && observer_start.is_none() {
                        observer_start = Some(pos);
                    }
                }
            }
        }
    }
    // Dev: `SS13_SPAWN_AT=x,y[,z]` puts every player on one tile (visual checks of a spot).
    let forced = std::env::var("SS13_SPAWN_AT").ok().and_then(|s| {
        let v: Vec<i32> = s.split(',').filter_map(|n| n.trim().parse().ok()).collect();
        let z = v.get(2).map_or(1, |&z| z as u16);
        (v.len() >= 2).then(|| TilePos::new(v[0], v[1], z))
    });
    let latejoin = found[1].clone();
    let tiles = forced
        .map(|t| vec![t])
        .into_iter()
        .chain(found)
        .find(|t| !t.is_empty())
        .unwrap_or_else(|| {
            let center = report.maps.first().map_or(TilePos::new(1, 1, 1), |m| {
                TilePos::new((m.size.0 / 2) as i32, (m.size.1 / 2) as i32, m.first_z)
            });
            warn!("no spawn landmarks on the map; players spawn at {center:?}");
            vec![center]
        });
    info!(
        "{} spawn points (first {:?}) found in {:.2?}",
        tiles.len(),
        tiles[0],
        start.elapsed()
    );
    world.insert_resource(SpawnPoints {
        observer: observer_start.or(Some(tiles[0])),
        tiles,
        next: 0,
        latejoin,
        next_latejoin: 0,
        forced,
        carp,
        maintenance,
        dev: dev_spawns(),
    });
    world.insert_resource(jobs);
}

fn build_mob_template(world: &mut World) {
    // No player can join without a mob: fail at startup.
    let id = world.resource::<MobProto>().0.clone();
    let id = id.as_str();
    let proto = world
        .resource::<ss13_proto::Prototypes>()
        .get(id)
        .unwrap_or_else(|| panic!("player mob prototype {id} is not loaded"))
        .clone();
    let assets = &world
        .get_resource::<MapLoadPlugin>()
        .expect("GameServerPlugin needs MapLoadPlugin")
        .assets;
    let manifest = read_manifest(assets).unwrap_or_else(|e| panic!("player mob: {e:#}"));
    let index = manifest.index();
    let template = Template::new(proto, &index);
    match &template.appearance {
        None => warn!("player mob has no drawable icon ({:?})", template.icon),
        Some(base) if is_under(id, "/mob/living/carbon/human") => {
            let humans = HumanAppearances::new(&manifest, base.clone());
            world.insert_resource(humans);
        }
        Some(_) => {}
    }
    world.insert_resource(MobTemplate(template));
}

/// Dev convenience, see [`crate::dev_cert_digest_file`]; the stdout line is for a client that
/// started this server as its child ([`crate::LOCAL_DIGEST_LINE`]).
fn write_dev_cert_digest(digest: Res<ServerCertificateDigest>, config: Res<NetServerConfig>) {
    println!("{}{}", crate::LOCAL_DIGEST_LINE, digest.0);
    let written = crate::dev_cert_digest_file(config.bind_addr.port())
        .and_then(|path| crate::write_dev_cert_digest(&path, &digest.0).map(|()| path));
    match written {
        Ok(path) => info!("dev: certificate digest written to {}", path.display()),
        Err(e) => warn!("dev: cannot write the certificate digest file: {e}"),
    }
}

/// Spawns a mob for every new client. A client that leaves keeps its body on the map without a
/// client (tg SSD); a ghost is removed.
// shortcut: rejoining gives a new mob; reconnect to the SSD body once clients have identities.
fn handle_connections(
    world: &mut World,
    mut connected: Local<MessageCursor<ClientConnected>>,
    mut disconnected: Local<MessageCursor<ClientDisconnected>>,
) {
    let joined: Vec<Entity> = connected
        .read(world.resource::<Messages<ClientConnected>>())
        .map(|c| c.client)
        .collect();
    let left: Vec<Entity> = disconnected
        .read(world.resource::<Messages<ClientDisconnected>>())
        .map(|d| d.client)
        .collect();
    for client in joined {
        spawn_player(world, client);
    }
    for client in left {
        let Some(mob) = world.resource_mut::<Players>().0.remove(&client) else {
            continue;
        };
        let Ok(mut entity) = world.get_entity_mut(mob) else {
            continue;
        };
        if entity.contains::<Ghost>() {
            entity.despawn();
            info!("client {client} left; ghost {mob} removed");
        } else {
            entity.remove::<(PlayerMob, MoveQueue)>();
            info!("client {client} left; body {mob} stays");
        }
    }
}

/// A new client enters the lobby (or, with the lobby skipped, joins at once: `crate::round`).
fn spawn_player(world: &mut World, client: Entity) {
    if world.get_entity(client).is_err() || world.resource::<Players>().mob(client).is_some() {
        return;
    }
    crate::round::new_player(world, client);
}

/// A body for `client` at `pos`, named `name` (chat reads `Name`), replacing its current one
/// (which is despawned with everything it carries). Equipment is the caller's.
pub(crate) fn spawn_mob(world: &mut World, client: Entity, pos: TilePos, name: String) -> Entity {
    let mob = world
        .resource_scope(|world, template: Mut<MobTemplate>| template.0.spawn(world, Some(pos)));
    // Steps keep their numbering across bodies: the client's `seq` counts per connection.
    let old = world.resource::<Players>().mob(client);
    let queue = old
        .and_then(|old| world.get::<MoveQueue>(old))
        .map(|q| MoveQueue {
            steps: VecDeque::new(),
            received: q.received,
            applied: q.applied,
        })
        .unwrap_or_default();
    let body = player_body(client, world.get::<crate::human::AppearancePrefs>(client));
    let species = body.species;
    world.entity_mut(mob).insert((
        Mover::new(RUN_DELAY_SECS),
        Networked,
        PlayerMob { client },
        queue,
        Name::new(name),
        body,
        HudData::default(),
        living_bundle(),
    ));
    // `set_species(pref_load = TRUE)`: organs, damage modifiers, blood of the character's
    // species.
    crate::species::set_species(world, mob, species);
    world
        .entity_mut(client)
        // A ghost's sight does not come along into a body.
        .remove::<(SeeInvisible, Sight)>()
        .insert((Viewer::new(pos), ControlledEntity(mob)));
    world.resource_mut::<Players>().0.insert(client, mob);
    if let Some(old) = old {
        world.try_despawn(old).ok();
    }
    mob
}

/// Tiles the search around a full spawn point visits before giving up: a crowd of ~250 mobs.
const SPAWN_SEARCH_TILES: usize = 256;

/// Round-robin over spawn points, skipping tiles a dense mob already stands on. When all are
/// taken, the nearest free tile reachable from the next spawn point through other mobs.
pub(crate) fn pick_spawn(world: &mut World) -> TilePos {
    let spawns = world.resource::<SpawnPoints>();
    let (tiles, next) = (spawns.tiles.clone(), spawns.next);
    let (i, pos) = pick_free(world, &tiles, next);
    world.resource_mut::<SpawnPoints>().next = i + 1;
    pos
}

/// tg `get_latejoin_spawn_point`: the next free chair of the arrivals shuttle
/// (`crate::arrivals`), else latejoin landmark, else [`pick_spawn`] (tg
/// `get_last_resort_spawn_points`).
pub(crate) fn pick_latejoin(world: &mut World) -> TilePos {
    let chairs = crate::arrivals::latejoin_tiles(world);
    let spawns = world.resource::<SpawnPoints>();
    let tiles = chairs.unwrap_or_else(|| spawns.latejoin.clone());
    if tiles.is_empty() {
        return pick_spawn(world);
    }
    let next = spawns.next_latejoin;
    let (i, pos) = pick_free(world, &tiles, next);
    world.resource_mut::<SpawnPoints>().next_latejoin = i + 1;
    pos
}

/// [`pick_spawn`] over any non-empty list of tiles, starting at `next`: the chosen index and tile.
pub(crate) fn pick_free(world: &World, tiles: &[TilePos], next: usize) -> (usize, TilePos) {
    let index = world.resource::<SpatialIndex>();
    let n = tiles.len();
    let free = (0..n)
        .map(|i| (next + i) % n)
        .find(|&i| index.dense_on_tile(tiles[i]).is_empty());
    match free {
        Some(i) => (i, tiles[i]),
        None => {
            let i = next % n;
            let start = tiles[i];
            let pos = nearest_free_tile(world, start).unwrap_or_else(|| {
                warn!("no free tile near spawn {start:?}; spawning on an occupied tile");
                start
            });
            (i, pos)
        }
    }
}

/// Breadth-first over cardinal steps from `start`, through tiles blocked only by player mobs
/// (walls end the search), to the first tile with nothing dense on it.
fn nearest_free_tile(world: &World, start: TilePos) -> Option<TilePos> {
    let index = world.resource::<SpatialIndex>();
    let mut seen = std::collections::HashSet::from([start]);
    let mut queue = std::collections::VecDeque::from([start]);
    while let Some(pos) = queue.pop_front() {
        let dense = index.dense_on_tile(pos);
        if dense.is_empty() {
            return Some(pos);
        }
        if !dense.iter().all(|&e| world.get::<PlayerMob>(e).is_some()) {
            continue;
        }
        for dir in [Dir::North, Dir::East, Dir::South, Dir::West] {
            let next = pos.step(dir);
            if seen.len() < SPAWN_SEARCH_TILES && index.in_bounds(next) && seen.insert(next) {
                queue.push_back(next);
            }
        }
    }
    None
}

/// `Move` -> `MoveRequest` for the sender's own mob: each `seq` once, in order, one per tick and
/// only when the mob is off cooldown (BYOND also takes one step per client per tick). Steps that
/// arrive during the cooldown wait in the mob's [`MoveQueue`] instead of being dropped, so every
/// step the client predicted is applied exactly once; the queue holds at most what an honest
/// client keeps in flight ([`MAX_UNACKED_STEPS`]), anything beyond is dropped. Spam therefore
/// cannot walk faster than the cooldown.
///
/// A step of a mob that cannot move (tg `MOBILITY_MOVE`: hard crit, dead, `TRAIT_IMMOBILIZED`
/// from `crate::status_effects`) is applied as blocked; a confused mob's step may turn;
/// a dead mob's step ghostizes it (tg `client/Move`); a mob held in a grab resists first
/// (`crate::pull`); a driver's step drives its vehicle (`crate::vehicles`). A ghost (no `Mover`)
/// takes one step per tick through `crate::ghost`.
// Cost: per tick, one lookup per received intent and per mob with waiting steps.
pub(crate) fn forward_moves(
    mut intents: MessageReader<IntentReceived>,
    players: Res<Players>,
    time: Res<Time>,
    mut mobs: Query<MoveItem>,
    mut requests: MessageWriter<MoveRequest>,
    mut ghost_steps: MessageWriter<GhostStep>,
    mut eye_steps: MessageWriter<crate::silicon::ai::EyeStep>,
    mut ghostize: MessageWriter<Ghostize>,
    mut resists: MessageWriter<MoveResist>,
    mut relays: MessageWriter<crate::closets::ClosetRelayMove>,
    (mut mech_steps, mut vehicle_steps): (
        MessageWriter<crate::mecha::core::MechRelayMove>,
        MessageWriter<crate::vehicles::VehicleRelayMove>,
    ),
    mut jaunt_steps: MessageWriter<crate::spells::JauntStep>,
    (mechs, vehicles): (Query<&crate::mecha::Mech>, Query<&crate::vehicles::Vehicle>),
    mut waiting: Local<EntityHashSet>,
    mut rng: ResMut<crate::combat::CombatRng>,
    // Weightless mobs step through `crate::spacemove`.
    (weightless, mut space_steps): (
        Query<(), With<crate::spacemove::Weightless>>,
        MessageWriter<crate::spacemove::SpaceStep>,
    ),
) {
    for intent in intents.read() {
        let ClientIntent::Move { dir, seq } = intent.intent else {
            continue;
        };
        let Some(mob) = players.mob(intent.client) else {
            continue;
        };
        let Ok((player, _, mut queue, ..)) = mobs.get_mut(mob) else {
            continue;
        };
        let queue = queue.bypass_change_detection();
        if player.client != intent.client || seq <= queue.received {
            continue;
        }
        queue.received = seq;
        if queue.steps.len() < MAX_UNACKED_STEPS {
            queue.steps.push_back((seq, dir));
            waiting.insert(mob);
        }
    }
    let now = time.elapsed_secs_f64();
    waiting.retain(|&mob| {
        let Ok((
            player,
            mover,
            mut queue,
            health,
            stat,
            ghost,
            held,
            traits,
            effects,
            closeted,
            eye,
            pilot,
            coma,
            jaunting,
            rider,
        )) = mobs.get_mut(mob)
        else {
            return false;
        };
        if mover.is_some_and(|m| now < m.ready_at()) {
            return true;
        }
        // A pilot's steps wait for the mech's move cooldown (`crate::mecha`).
        if let Some(pilot) = pilot
            && mechs.get(pilot.mech).is_ok_and(|m| now < m.next_move)
        {
            return true;
        }
        // A driver's steps wait for the vehicle's move cooldown (`crate::vehicles`).
        if let Some(rider) = rider
            && vehicles
                .get(rider.vehicle)
                .is_ok_and(|v| now < v.next_move())
        {
            return true;
        }
        let Some((seq, dir)) = queue.steps.pop_front() else {
            return false;
        };
        queue.applied = seq;
        match (health, stat) {
            _ if ghost => {
                ghost_steps.write(GhostStep { ghost: mob, dir });
            }
            // The AI's eye (`crate::silicon::ai`), tg `/mob/eye/camera/ai` `relaymove`.
            _ if eye => {
                eye_steps.write(crate::silicon::ai::EyeStep { eye: mob, dir });
            }
            // tg `phased_mob/relaymove` of an ethereal jaunt (`crate::spells`).
            _ if jaunting => {
                jaunt_steps.write(crate::spells::JauntStep { mob, dir });
            }
            // A mob in a death coma (a changeling's stasis) is not dead: it stays.
            (_, Some(&Stat::Dead)) if !coma => {
                ghostize.write(Ghostize {
                    client: player.client,
                });
            }
            // tg `relaymove` of the mech the mob pilots (`crate::mecha`).
            (_, Some(&Stat::Conscious)) if pilot.is_some() => {
                mech_steps.write(crate::mecha::core::MechRelayMove { pilot: mob, dir });
            }
            _ if pilot.is_some() => {}
            // tg `relaymove` of the closet the mob is in (`crate::closets`).
            _ if closeted => {
                relays.write(crate::closets::ClosetRelayMove { mob });
            }
            // tg `Process_Grab`: the grab is resisted first (`crate::pull`).
            _ if held => {
                resists.write(MoveResist { mob, dir });
            }
            // tg `buckled.relaymove` of the vehicle the mob drives (`crate::vehicles`).
            _ if rider.is_some() => {
                vehicle_steps.write(crate::vehicles::VehicleRelayMove { rider: mob, dir });
            }
            (Some(health), Some(&stat)) if !health.can_move(stat) => {}
            _ if traits.is_some_and(|t| t.immobilized) => {}
            _ if mover.is_some() => {
                let dir = crate::status_effects::confused_step(effects, &mut rng, dir);
                if weightless.contains(mob) {
                    space_steps.write(crate::spacemove::SpaceStep { mob, dir });
                } else {
                    requests.write(MoveRequest { entity: mob, dir });
                }
            }
            _ => {}
        }
        !queue.steps.is_empty()
    });
}

type MoveItem<'a> = (
    &'a PlayerMob,
    Option<&'a Mover>,
    &'a mut MoveQueue,
    Option<&'a Health>,
    Option<&'a Stat>,
    Has<Ghost>,
    Has<HeldByGrab>,
    Option<&'a crate::status_effects::Incapacitation>,
    Option<&'a crate::status_effects::StatusEffects>,
    Has<crate::closets::InCloset>,
    Has<crate::silicon::ai::AiEye>,
    Option<&'a crate::mecha::Pilot>,
    Has<crate::health::DeathComa>,
    Has<crate::spells::Jaunting>,
    Option<&'a crate::vehicles::Rider>,
);

/// The mob's authoritative state to its client after a step was applied (moved or blocked) or
/// the mob moved or turned otherwise; also once at spawn.
pub(crate) fn ack_moves(
    mobs: Query<
        (&PlayerMob, &TilePos, Option<&Facing>, &MoveQueue),
        Or<(Changed<TilePos>, Changed<Facing>, Changed<MoveQueue>)>,
    >,
    mut acks: MessageWriter<SendMoveAck>,
) {
    for (player, pos, facing, queue) in &mobs {
        acks.write(SendMoveAck {
            client: player.client,
            ack: MoveAck {
                seq: queue.applied,
                pos: *pos,
                facing: facing.map(|f| f.0),
            },
        });
    }
}

/// The client's view box follows its mob; only mobs that moved this tick are touched.
pub(crate) fn follow_mobs(
    mobs: Query<(&TilePos, &PlayerMob), Changed<TilePos>>,
    // A camera console places the view of its user (`crate::silicon::cameras`).
    mut viewers: Query<&mut Viewer, Without<crate::silicon::cameras::ViewOverride>>,
) {
    for (pos, player) in &mobs {
        if let Ok(mut viewer) = viewers.get_mut(player.client)
            && viewer.center != *pos
        {
            viewer.center = *pos;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_spawns_parse_tokens_and_tiles() {
        let spawns = parse_dev_spawns("client1=108,66; client2 = 98,72,2;bad=1;=3,4;x=a,b");
        assert_eq!(spawns.len(), 2, "{spawns:?}");
        assert_eq!(spawns["client1"], TilePos::new(108, 66, 1));
        assert_eq!(spawns["client2"], TilePos::new(98, 72, 2));
    }

    #[test]
    fn full_spawn_point_spawns_on_the_nearest_free_tile_inside_walls() {
        let mut app = App::new();
        app.add_plugins(ss13_world::WorldPlugin);
        let world = app.world_mut();
        world
            .resource_mut::<SpatialIndex>()
            .create_level(1, 8, 8)
            .unwrap();
        let spawn = TilePos::new(2, 2, 1);
        world.insert_resource(SpawnPoints {
            tiles: vec![spawn],
            ..default()
        });
        let client = world.spawn_empty().id();
        // A mob on the spawn point and a wall column east of it.
        world.spawn((spawn, ss13_world::Density, PlayerMob { client }));
        for y in 0..8 {
            world.spawn((TilePos::new(3, y, 1), ss13_world::Density));
        }
        let first = pick_spawn(world);
        assert_eq!(first.chebyshev(spawn), Some(1));
        assert_ne!(first.x, 3);
        assert!(
            world
                .resource::<SpatialIndex>()
                .dense_on_tile(first)
                .is_empty()
        );

        // Fully walled in: nothing reachable, falls back to the spawn point itself.
        for pos in [
            first,
            TilePos::new(1, 2, 1),
            TilePos::new(2, 1, 1),
            TilePos::new(2, 3, 1),
        ] {
            world.spawn((pos, ss13_world::Density));
        }
        assert_eq!(pick_spawn(world), spawn);
    }

    #[test]
    fn subtype_match_respects_path_segments() {
        assert!(is_under(
            "/obj/effect/landmark/start",
            "/obj/effect/landmark/start"
        ));
        assert!(is_under(
            "/obj/effect/landmark/start/assistant/override",
            "/obj/effect/landmark/start/assistant"
        ));
        assert!(!is_under(
            "/obj/effect/landmark/starts",
            "/obj/effect/landmark/start"
        ));
    }
}
