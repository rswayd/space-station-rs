//! The blob after tg/BandaStation `code/modules/antagonists/blob` and
//! `modular_bandastation/balance/code/blob.dm`: the antagonist (`/datum/antagonist/blob`,
//! `/datum/antagonist/blob/infection`), the overmind (`/mob/eye/blob`), its powers (`powers.dm`,
//! `_onclick/overmind.dm`, the `blob_overmind` HUD), its structures ([`structures`]), strains
//! ([`strains`]) and minions ([`minions`]), and the "Blob" and "Blob Infection" heavy
//! midrounds.
//!
//! - **Selection** (`super::midround`): "Blob" polls the ghosts with `ROLE_BLOB`: an overmind
//!   on a blob start with 60 resources, which may place its core after a minute and does so on
//!   a blob start by itself after six ([`spawn_overmind`], [`place_core`]). "Blob Infection" makes
//!   a living crew member (`ROLE_BLOB_INFECTION`) a host ([`make_host`]) with the "Pop" button: it
//!   bursts on its tile into an overmind with a core (pressed on an invalid tile it refuses; after
//!   six minutes it bursts anyway, on a blob start if its tile is invalid) ([`pop`]).
//! - **The overmind**: an invisible eye on the blob plane (ghosts see it) that moves through
//!   walls; once placed, only within a tile of its blob. Every 2 s (`SSobj`, [`process`]) the
//!   core gives 2 resources (+ the strain's bonus, at most 100) and regenerates 2 (+ bonus), the
//!   core and every node pulse their area, and the overmind checks: core gone — it dies (its
//!   client becomes a ghost, its blobs and minions go dead); 75 structures or 10 minutes after
//!   placement — the level-5 biohazard announcement; a free reroll every 4 minutes after one is
//!   spent; 400 structures in station areas — critical mass, delta alert and, 45 s later,
//!   victory: every living non-blob mob in a station area dies (a spore rises over each) and the
//!   round ends.
//! - **Clicks** ([`overmind_clicks`]): a click grows the blob onto the tile (4 resources; living
//!   things there are hit with the strain's reagent and 2 come back; a diagonal-only reach or a
//!   tile that already has a blob refunds), ctrl makes a strong blob (15) and a strong one
//!   reflective (15), alt removes a blob for its `point_return`, middle rallies spores.
//! - **Buttons**: jump to the core (or place it), jump to a node, blobbernaut (40, on a factory),
//!   resource blob (40), node (50), factory (60) on the normal blob under the overmind (resource
//!   and factory blobs near a node or the core, all apart from their kind), readapt strain (one of
//!   six, free or 40), relocate the core to the node under it (80); the resources and the core's
//!   health are shown on two more.
//! - **BandaStation**: off the main station floor the expansion costs double and 700 structures
//!   win (`check_station_floor`).
// shortcut: no telepathy (the
// overmind's speech is heard by ghosts only), no "Place Blob Core" HUD before the button, the
// spore rally only says so, no status displays' biohazard alert, no news report, the victory takes everyone at once (no 10 s alarm, no area recolour),
// the core placement checks a 13-tile line of sight instead of `view`, no
// `main_floor` in the map configs here (every station level counts as the main floor), the
// overmind's info panel is the generic one.

pub mod minions;
pub mod strains;
pub mod structures;

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::IconIndex;
use ss13_net::server::{ClientToken, Invisibility, Networked, VisibleToMobs};
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::protocol::{ClickModifiers, TICK_DURATION};
use ss13_shared::{Appearance, Dir, Facing, IconStateId, Layer, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems, ask_world};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiSystems};
use ss13_world::{SpatialIndex, WorldSystems};

use self::strains::Strain;
use self::structures::{Blob, Kind, blob_at, blobs_allowed, blobs_in_range};
use super::objectives::RoundEnd;
use super::{AntagHudData, AntagHudUi, BrotherInfo, ObjectiveInfo};
use crate::actions::{ActionButton, ActionButtons, ActionButtonsUi, set_buttons};
use crate::combat::{CLICK_CD_MELEE, CombatRng};
use crate::ghost_poll::{POLL_IGNORE_BLOB, PollSpec};
use crate::health::{DamageType, Health, Stat};
use crate::hud::{HealthHudData, HealthHudUi};
use crate::server::{PlayerMob, SpawnPoints};
use crate::tools::{tell, tell_mob};

pub use super::{ROLE_BLOB, ROLE_BLOB_INFECTION};

/// `name` of the antagonist and its round-end category.
pub const ANTAG_NAME: &str = "Блоб";
const ROUNDEND_CATEGORY: &str = "Блобы";
/// tg `ROLE_BLOB` as a faction (`faction = list(ROLE_BLOB)`).
pub const FACTION_BLOB: &str = "Blob";
/// tg `PASSBLOB`.
pub const PASSBLOB: u32 = 1 << 3;
/// tg `BLOBS_ALLOWED` (`area_flags`).
pub const BLOBS_ALLOWED: u32 = 1 << 1;

/// `OVERMIND_MAX_POINTS_DEFAULT`, `OVERMIND_STARTING_POINTS`, `OVERMIND_STARTING_REROLLS`.
pub const MAX_POINTS: f32 = 100.0;
pub const STARTING_POINTS: f32 = 60.0;
const STARTING_REROLLS: u32 = 1;
/// `OVERMIND_STARTING_MIN_PLACE_TIME`, `OVERMIND_STARTING_AUTO_PLACE_TIME`, seconds.
pub const MIN_PLACE_SECS: f64 = 60.0;
pub const AUTO_PLACE_SECS: f64 = 360.0;
/// `OVERMIND_WIN_CONDITION_AMOUNT`, BandaStation's off-floor 700.
pub const WIN_COUNT: u32 = 400;
const WIN_COUNT_OFF_FLOOR: u32 = 700;
/// `OVERMIND_ANNOUNCEMENT_MIN_SIZE`, `OVERMIND_ANNOUNCEMENT_MAX_TIME`.
const ANNOUNCE_SIZE: u32 = 75;
const ANNOUNCE_SECS: f64 = 600.0;
/// `OVERMIND_MAX_CAMERA_STRAY` "3x3".
const CAMERA_STRAY: u32 = 1;
/// `BLOB_EXPAND_COST`, `BLOB_ATTACK_REFUND`.
pub const EXPAND_COST: f32 = 4.0;
pub const ATTACK_REFUND: f32 = 2.0;
/// `CLICK_CD_RAPID`.
const CLICK_CD_RAPID: f64 = 0.2;
/// Structure and power prices.
pub const UPGRADE_STRONG_COST: f32 = 15.0;
pub const UPGRADE_REFLECTOR_COST: f32 = 15.0;
pub const RESOURCE_COST: f32 = 40.0;
pub const FACTORY_COST: f32 = 60.0;
pub const NODE_COST: f32 = 50.0;
pub const BLOBBERNAUT_COST: f32 = 40.0;
/// `pick_blobbernaut_candidate`'s `poll_time`.
const BLOBBERNAUT_POLL_SECS: f64 = 20.0;
pub const RELOCATE_COST: f32 = 80.0;
pub const REROLL_COST: f32 = 40.0;
const REROLL_FREE_SECS: f64 = 240.0;
const REROLL_CHOICES: usize = 6;
/// `BLOB_NODE_MIN_DISTANCE`, `BLOB_FACTORY_MIN_DISTANCE`, `BLOB_RESOURCE_MIN_DISTANCE`;
/// `BLOB_NODE_PULSE_RANGE`, `BLOB_CORE_PULSE_RANGE`.
const NODE_MIN_DISTANCE: u32 = 5;
const FACTORY_MIN_DISTANCE: u32 = 7;
const RESOURCE_MIN_DISTANCE: u32 = 4;
const NODE_REACH: u32 = 3;
const CORE_REACH: u32 = 4;
/// `begin_victory` -> `victory`.
const VICTORY_DELAY: f64 = 45.0;
/// `SSobj` `wait`.
pub const PROCESS_SECS: f64 = 2.0;
/// `check_core_visibility`: nobody within 7, nobody seeing it within 13.
const PLACE_CLEAR_RANGE: u32 = 7;
const PLACE_SIGHT_RANGE: u32 = 13;

const EYE_DMI: &str = "icons/mob/eyemob.dmi";
const HUD_DMI: &str = "icons/hud/blob.dmi";
const ANTAG_HUD_DMI: &str = "icons/mob/huds/antag_hud.dmi";
const OUTBREAK5: &str = "announcer_outbreak5";

const B_CORE: &str = "blob:core";
const B_NODE: &str = "blob:jump_node";
const B_NAUT: &str = "blob:blobbernaut";
const B_RESOURCE: &str = "blob:resource";
const B_NODE_BLOB: &str = "blob:node";
const B_FACTORY: &str = "blob:factory";
const B_READAPT: &str = "blob:readapt";
const B_RELOCATE: &str = "blob:relocate";
const B_POWER: &str = "blob:power";
const B_HEALTH: &str = "blob:health";
const B_POP: &str = "blob:pop";

/// `/mob/eye/blob`.
#[derive(Component, Clone, Debug)]
pub struct BlobOvermind {
    /// The player's key, for the round-end report.
    pub key: String,
    pub strain: Strain,
    /// `blob_points`, `max_blob_points`.
    pub points: f32,
    pub max_points: f32,
    pub core: Option<Entity>,
    pub placed: bool,
    /// `manualplace_min_time` (0: passed), `autoplace_max_time`.
    pub manual_place_at: f64,
    pub auto_place_at: f64,
    pub free_rerolls: u32,
    pub last_reroll: f64,
    /// `last_attack`: the next manual expansion.
    pub last_attack: f64,
    pub max_count: u32,
    /// `blobwincount`, `expand_cost` (BandaStation doubles both off the main floor).
    pub win_count: u32,
    pub expand_cost: f32,
    pub nodes_required: bool,
    /// `announcement_time`, `has_announced`.
    pub announce_at: Option<f64>,
    pub announced: bool,
    /// `victory_in_progress`: when `victory` runs.
    pub victory_at: Option<f64>,
    pub won: bool,
    /// Reactive spines' `retaliate_cooldown`.
    pub retaliate_at: f64,
    /// `strain_choices` until a reroll is taken.
    pub reroll_choices: Vec<Strain>,
    pub next_process: f64,
}

impl BlobOvermind {
    pub fn new(strain: Strain, points: f32, now: f64) -> Self {
        Self {
            key: String::new(),
            strain,
            points,
            max_points: MAX_POINTS,
            core: None,
            placed: false,
            manual_place_at: now + MIN_PLACE_SECS,
            auto_place_at: now + AUTO_PLACE_SECS,
            free_rerolls: STARTING_REROLLS,
            last_reroll: 0.0,
            last_attack: now,
            max_count: 0,
            win_count: WIN_COUNT,
            expand_cost: EXPAND_COST,
            nodes_required: true,
            announce_at: None,
            announced: false,
            victory_at: None,
            won: false,
            retaliate_at: 0.0,
            reroll_choices: Vec::new(),
            next_process: now + PROCESS_SECS,
        }
    }
}

/// A crew member carrying the blob (`/datum/antagonist/blob/infection` before it pops).
#[derive(Component, Clone, Debug)]
pub struct BlobHost {
    pub key: String,
    /// `autoplace_time` of the pop action.
    pub pop_at: f64,
}

/// One step of an overmind, taken from its client's queue (`crate::server`).
#[derive(Message, Clone, Copy, Debug)]
pub struct BlobStep {
    pub overmind: Entity,
    pub dir: Dir,
}

/// A click of an overmind's client on a tile it sees (`crate::click`).
#[derive(Message, Clone, Copy, Debug)]
pub struct BlobClick {
    pub client: Entity,
    pub overmind: Entity,
    pub target_pos: TilePos,
    pub modifiers: ClickModifiers,
}

/// What a blob was, for the round-end report (the overmind is gone when its core dies).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlobRecord {
    pub overmind: Entity,
    pub key: String,
    pub name: String,
    pub max_count: u32,
    pub won: bool,
}

#[derive(Resource, Default, Debug)]
pub struct BlobRecords(pub Vec<BlobRecord>);

/// An open reroll or node-jump question.
#[derive(Component, Clone, Debug)]
enum BlobAsk {
    Reroll(Entity),
    Node(Entity, Vec<Entity>),
}

/// The overmind's and the host's panel (tg `AntagInfoBlob`, drawn as the generic one).
pub struct BlobInfoWindow;

impl UiInterface for BlobInfoWindow {
    const NAME: &'static str = crate::antag::cult::AntagInfoGenericUi::NAME;
    type Data = ();
    type StaticData = BrotherInfo;
}

impl ServerUi for BlobInfoWindow {
    type Source = (Option<&'static BlobOvermind>, Option<&'static BlobHost>);
    type UserParam = ();
    type Act = ();

    fn static_data((overmind, _): (Option<&BlobOvermind>, Option<&BlobHost>)) -> BrotherInfo {
        BrotherInfo {
            antag_name: ANTAG_NAME.to_owned(),
            objectives: vec![ObjectiveInfo {
                count: 1,
                explanation: "Достигните критической массы!".to_owned(),
                complete: overmind.is_some_and(|o| o.won),
            }],
        }
    }

    fn data(_: (Option<&BlobOvermind>, Option<&BlobHost>), _: Entity, _: &()) {}
}

pub struct BlobPlugin;

impl Plugin for BlobPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<BlobStep>()
            .add_message::<BlobClick>()
            .init_resource::<BlobRecords>()
            .register_ui::<BlobInfoWindow>()
            .add_systems(Update, move_overminds.in_set(WorldSystems::Movement))
            .add_systems(
                Update,
                (
                    open_info,
                    trigger_buttons,
                    prompt_answers.after(PromptSystems::Answer),
                )
                    .in_set(UiSystems::Handle),
            )
            .add_systems(
                Update,
                overmind_clicks
                    .after(crate::click::ResolveClicks)
                    .before(crate::health::HealthSystems),
            )
            .add_systems(Update, process.after(crate::health::HealthSystems));
        minions::build(app);
    }
}

// ---------------------------------------------------------------------------------------------
// Helpers

/// DM `prob(percent)` on the combat stream.
pub(crate) fn roll(world: &mut World, percent: f32) -> bool {
    world.resource_mut::<CombatRng>().chance(percent)
}

/// DM `rand(low, high)`.
pub(crate) fn rand_u(world: &mut World, low: u32, high: u32) -> u32 {
    if high <= low {
        return low;
    }
    world.resource_mut::<CombatRng>().range(low, high)
}

/// The overmind's strain, if it still is one.
pub fn strain_of(world: &World, overmind: Entity) -> Option<Strain> {
    world.get::<BlobOvermind>(overmind).map(|o| o.strain)
}

/// `TRAIT_BLOB_ALLY` / `faction_check(ROLE_BLOB)`: overminds and blob mobs.
pub fn is_ally(world: &World, e: Entity) -> bool {
    world.get::<BlobOvermind>(e).is_some()
        || world
            .get::<crate::mobs::Faction>(e)
            .is_some_and(|f| f.0.iter().any(|x| x == FACTION_BLOB))
}

/// `amount` of `kind` to a living mob (negative heals).
pub(crate) fn hurt(world: &mut World, mob: Entity, kind: DamageType, amount: f32) {
    let (Some(mut health), Some(mut stat)) = (
        world.get::<Health>(mob).cloned(),
        world.get::<Stat>(mob).copied(),
    ) else {
        return;
    };
    let changed = crate::health::apply_damage(&mut health, &mut stat, kind, amount);
    world.entity_mut(mob).insert(health);
    if changed.is_some() {
        world.entity_mut(mob).insert(stat);
    }
}

/// `add_points`: clamped to `0..=max_blob_points`.
pub fn add_points(world: &mut World, overmind: Entity, points: f32) {
    if let Some(mut o) = world.get_mut::<BlobOvermind>(overmind) {
        o.points = (o.points + points).clamp(0.0, o.max_points);
    }
}

/// `can_buy`: pays `cost` or says what is missing.
fn can_buy(world: &mut World, overmind: Entity, cost: f32) -> bool {
    let Some(points) = world.get::<BlobOvermind>(overmind).map(|o| o.points) else {
        return false;
    };
    if points < cost {
        tell_mob(
            world,
            overmind,
            Span::Warning,
            format!("Вам это не по карману: нужно как минимум {cost} ресурсов!"),
        );
        return false;
    }
    add_points(world, overmind, -cost);
    true
}

/// The overmind's structures of `kind`.
pub fn count_kind(world: &mut World, overmind: Entity, kind: Kind) -> usize {
    world
        .query::<&Blob>()
        .iter(world)
        .filter(|b| b.overmind == Some(overmind) && b.kind == kind)
        .count()
}

/// `blobs_legit`: the overmind's structures in station areas.
pub fn legit_count(world: &mut World, overmind: Entity) -> u32 {
    world
        .query::<&Blob>()
        .iter(world)
        .filter(|b| b.overmind == Some(overmind) && b.legit)
        .count() as u32
}

/// `is_valid_turf`: a `BLOBS_ALLOWED` area on a station level, with ground.
pub fn valid_turf(world: &World, pos: TilePos) -> bool {
    blobs_allowed(world, pos)
        && crate::events::controller::station_levels(world).contains(&pos.z)
        && !groundless(world, pos)
}

/// `isgroundlessturf`: space and open space.
fn groundless(world: &World, pos: TilePos) -> bool {
    crate::construction::is_space(world, pos) || crate::multiz::is_openspace(world, pos)
}

fn turf_dense(world: &World, pos: TilePos) -> bool {
    crate::construction::turf_at(world, pos)
        .is_some_and(|t| world.get::<ss13_world::Density>(t).is_some())
}

/// `SSmapping.is_main_station_floor`: no `main_floor` in the map configs here, so every
/// station level is it.
fn main_station_floor(_world: &World, _pos: TilePos) -> bool {
    true
}

/// `attack_living` with `BLOB_REAGENTATK_VOL` and the strain's message.
pub fn attack_living(world: &mut World, overmind: Entity, victim: Entity) {
    attack_living_volume(
        world,
        overmind,
        victim,
        strains::REAGENT_ATTACK_VOLUME,
        true,
    );
}

/// The strain's reagent on `victim` (`expose_mob` through its bio protection).
pub fn attack_living_volume(
    world: &mut World,
    overmind: Entity,
    victim: Entity,
    volume: f32,
    message: bool,
) {
    let Some(strain) = strain_of(world, overmind) else {
        return;
    };
    // `can_blob_attack`: not a phased mob.
    if world.get::<crate::spells::Jaunting>(victim).is_some()
        || world.get::<Health>(victim).is_none()
    {
        return;
    }
    let dead = world.get::<Stat>(victim) == Some(&Stat::Dead);
    let protection =
        crate::armor::getarmor(world, victim, crate::zone::BodyZone::Chest, "bio") / 100.0;
    let volume = strains::reac_volume(strain, volume, protection, dead || is_ally(world, victim));
    let e = strains::exposure(strain, volume);
    for (kind, amount) in [
        (DamageType::Brute, e.brute),
        (DamageType::Burn, e.burn),
        (DamageType::Tox, e.tox),
        (DamageType::Oxy, e.oxy),
    ] {
        if amount > 0.0 {
            hurt(world, victim, kind, amount);
        }
    }
    if e.stamina > 0.0 {
        crate::status_effects::adjust_stamina(world, victim, e.stamina);
    }
    if e.fire_stacks > 0.0 {
        crate::fire::adjust_fire_stacks(world, victim, e.fire_stacks);
    }
    if e.ignite {
        crate::fire::ignite_mob(world, victim);
    }
    if !e.reagents.is_empty()
        && let Some(chem) = world
            .get_resource::<crate::chem::Chem>()
            .map(|c| c.0.clone())
    {
        let list: Vec<_> = e
            .reagents
            .iter()
            .filter_map(|&(path, amount)| chem.id(path).map(|id| (id, amount)))
            .collect();
        if let Some(mut reagents) = world.get_mut::<crate::chem::MobReagents>(victim) {
            reagents.blood.add_reagent_list(&chem, &list, None, false);
        }
    }
    if message && !dead {
        let silicon = world.get::<crate::silicon::Silicon>(victim).is_some();
        tell_mob(world, victim, Span::Danger, strain.attack_message(silicon));
    }
}

fn name_of(world: &World, e: Entity) -> String {
    crate::medical::name_of(world, e)
}

// ---------------------------------------------------------------------------------------------
// The overmind

/// `can_be_selected` of the ghost midround: always (the overmind falls back to the observer
/// start without blob starts).
pub fn can_spawn(_world: &mut World) -> bool {
    true
}

/// `get_blobspawn`: a blob start, else the observer start.
fn blobspawn(world: &mut World) -> Option<TilePos> {
    let starts = world
        .get_resource::<SpawnPoints>()
        .map(|s| s.blobstart.clone())
        .unwrap_or_default();
    if !starts.is_empty() {
        let i = rand_u(world, 0, starts.len() as u32 - 1) as usize;
        return Some(starts[i]);
    }
    world.get_resource::<SpawnPoints>().and_then(|s| s.observer)
}

/// `create_ruleset_body` for each `(client, ghost)`: the overminds.
pub fn spawn_blobs(world: &mut World, ghosts: &[(Entity, Entity)]) -> Vec<Entity> {
    let mut out = Vec::new();
    for &(client, ghost) in ghosts {
        let Some(pos) = blobspawn(world) else {
            break;
        };
        let overmind = spawn_overmind(world, pos, STARTING_POINTS);
        if !crate::medical::brainmob::possess(world, client, ghost, overmind) {
            world.despawn(overmind);
            continue;
        }
        world.try_despawn(ghost).ok();
        take_control(world, overmind, client);
        out.push(overmind);
    }
    out
}

/// `new /mob/eye/blob(loc, starting_points)` without a player: a random strain, the marker in
/// its complementary colour.
pub fn spawn_overmind(world: &mut World, pos: TilePos, points: f32) -> Entity {
    let strain = Strain::ALL[rand_u(world, 0, Strain::ALL.len() as u32 - 1) as usize];
    let t = crate::tools::now(world);
    let mut state = BlobOvermind::new(strain, points, t);
    // BandaStation `check_station_floor`.
    if !main_station_floor(world, pos) {
        state.expand_cost = EXPAND_COST * 2.0;
        state.win_count = WIN_COUNT_OFF_FLOOR;
    }
    let number = rand_u(world, 1, 999);
    let marker = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(EYE_DMI, "marker"));
    let overmind = world
        .spawn((
            state,
            pos,
            Facing(Dir::South),
            Appearance {
                icon_state: marker.unwrap_or(IconStateId(u32::MAX)),
                layer: Layer(5.0),
                plane: crate::ghost::GHOST_PLANE,
                color: Some(strain.complementary()),
                pixel_offset: [0, 0],
                rotation: 0,
                mouse_opacity: ss13_shared::MouseOpacity::Transparent,
                overlays: vec![],
            },
            Invisibility(crate::ghost::INVISIBILITY_OBSERVER),
            Networked,
            Name::new(format!("Blob Overmind ({number})")),
            HealthHudData {
                health: None,
                move_delay: TICK_DURATION.as_secs_f32(),
                phasing: true,
                ..default()
            },
            crate::mobs::Faction(vec![FACTION_BLOB.to_owned()]),
        ))
        .id();
    world
        .entity_mut(overmind)
        .insert(VisibleToMobs(vec![overmind]));
    info!(
        "{overmind} is a blob overmind ({}) at {pos:?}",
        strain.def().name
    );
    // Cleared once the overmind is despawned (the `QDELETED` sweep).
    crate::shuttle::register_hostile_environment(world, crate::shuttle::Blocker::Entity(overmind));
    overmind
}

/// The client now plays `overmind` (`mind_initialize` -> `add_antag_datum`, `Login`): the
/// greeting, the panel, the HUD.
pub fn take_control(world: &mut World, overmind: Entity, client: Entity) {
    let key = world
        .get::<ClientToken>(client)
        .map(|t| t.0.clone())
        .unwrap_or_default();
    let hud_icon = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(ANTAG_HUD_DMI, "blob"));
    let Some(state) = world.get_mut::<BlobOvermind>(overmind).map(|mut o| {
        o.key = key;
        o.clone()
    }) else {
        return;
    };
    world.entity_mut(overmind).insert(AntagHudData {
        hud_icon,
        ..default()
    });
    tell(world, client, Span::Bold, format!("Вы - {ANTAG_NAME}."));
    tell(world, client, Span::BoldNotice, "Вы — надзиратель!");
    tell(
        world,
        client,
        Span::Notice,
        "Цель: достигните критической массы!",
    );
    let d = state.strain.def();
    tell(
        world,
        client,
        Span::Notice,
        format!("Ваш штамм — {}: он {}", d.name, d.description),
    );
    if !state.placed {
        let left = (state.auto_place_at - crate::tools::now(world)).max(0.0);
        tell(
            world,
            client,
            Span::BoldWarning,
            format!(
                "Ядро будет размещено автоматически через {} с. Разместить его вручную можно \
                 кнопкой «Разместить ядро» (не раньше чем через минуту).",
                left.round()
            ),
        );
    }
    world.write_message(OpenUi::new::<AntagHudUi>(client, overmind, overmind, ""));
    world.write_message(OpenUi::new::<HealthHudUi>(client, overmind, overmind, ""));
    world.write_message(OpenUi::new::<BlobInfoWindow>(
        client, overmind, overmind, ANTAG_NAME,
    ));
    refresh_buttons(world, overmind);
}

/// `set_strain`: the new strain everywhere (structures, minions, the marker) and its text.
pub fn set_strain(world: &mut World, overmind: Entity, strain: Strain) {
    match world.get_mut::<BlobOvermind>(overmind) {
        Some(mut o) => o.strain = strain,
        None => return,
    }
    if let Some(mut look) = world.get_mut::<Appearance>(overmind) {
        look.color = Some(strain.complementary());
    }
    let blobs: Vec<Entity> = world
        .query::<(Entity, &Blob)>()
        .iter(world)
        .filter(|(_, b)| b.overmind == Some(overmind))
        .map(|(e, _)| e)
        .collect();
    for blob in blobs {
        structures::refresh_look(world, blob);
    }
    let minions: Vec<Entity> = world
        .query::<(Entity, &minions::BlobMinion)>()
        .iter(world)
        .filter(|(_, m)| m.overmind == Some(overmind))
        .map(|(e, _)| e)
        .collect();
    let d = strain.def();
    for mob in minions {
        minions::strain_changed(world, mob);
        tell_mob(
            world,
            mob,
            Span::Notice,
            format!("Штамм вашего надзирателя теперь: {}!", d.name),
        );
    }
    tell_mob(
        world,
        overmind,
        Span::Notice,
        format!("Ваш штамм теперь: {}!", d.name),
    );
    tell_mob(
        world,
        overmind,
        Span::Notice,
        format!("Штамм {} {}", d.name, d.description),
    );
    if let Some(effect) = d.effectdesc {
        tell_mob(
            world,
            overmind,
            Span::Notice,
            format!("Штамм {} {effect}", d.name),
        );
    }
}

/// How the core is placed (`BLOB_NORMAL_PLACEMENT`, `BLOB_FORCE_PLACEMENT`,
/// `BLOB_RANDOM_PLACEMENT`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    Normal,
    Force,
    Random,
}

/// `check_core_visibility`: no player near enough to see the core appear.
fn core_unseen(world: &mut World, overmind: Entity, pos: TilePos) -> bool {
    let players: Vec<(Entity, TilePos)> = world
        .query_filtered::<(Entity, &TilePos), (With<PlayerMob>, With<Health>)>()
        .iter(world)
        .map(|(e, p)| (e, *p))
        .collect();
    for (mob, at) in players {
        if is_ally(world, mob) {
            continue;
        }
        let Some(d) = pos.chebyshev(at) else {
            continue;
        };
        if d <= PLACE_CLEAR_RANGE {
            tell_mob(
                world,
                overmind,
                Span::Warning,
                "Слишком близко кто-то есть, ядро не разместить!",
            );
            return false;
        }
        if crate::mobs::ai::behaviors::can_see(world, pos, at, PLACE_SIGHT_RANGE) {
            tell_mob(
                world,
                overmind,
                Span::Warning,
                "Отсюда кто-то может увидеть ваше ядро!",
            );
            return false;
        }
    }
    true
}

/// `check_objects_tile`: nothing dense; a normal blob there is replaced, another one refuses.
fn objects_tile_clear(world: &mut World, overmind: Entity, pos: TilePos) -> bool {
    let things: Vec<Entity> = world.resource::<SpatialIndex>().on_tile(pos).to_vec();
    for thing in things {
        if let Some(kind) = world.get::<Blob>(thing).map(|b| b.kind) {
            if kind == Kind::Normal {
                world.despawn(thing);
                continue;
            }
            tell_mob(world, overmind, Span::Warning, "Здесь уже есть блоб!");
            return false;
        }
        let turf = world.get::<ss13_mapload::Turf>(thing).is_some();
        if !turf && world.get::<ss13_world::Density>(thing).is_some() {
            tell_mob(
                world,
                overmind,
                Span::Warning,
                "Это место слишком загромождено для ядра!",
            );
            return false;
        }
    }
    true
}

/// `place_blob_core`. True when the core is (now) placed.
pub fn place_core(
    world: &mut World,
    overmind: Entity,
    placement: Placement,
    pop_override: bool,
) -> bool {
    let Some(state) = world.get::<BlobOvermind>(overmind).cloned() else {
        return false;
    };
    if state.placed && placement != Placement::Force {
        return false;
    }
    let t = crate::tools::now(world);
    match placement {
        Placement::Normal => {
            let Some(&pos) = world.get::<TilePos>(overmind) else {
                return false;
            };
            if !pop_override && !core_unseen(world, overmind, pos) {
                return false;
            }
            if turf_dense(world, pos) {
                tell_mob(
                    world,
                    overmind,
                    Span::Warning,
                    "Это место слишком плотное для ядра!",
                );
                return false;
            }
            if !valid_turf(world, pos) {
                tell_mob(
                    world,
                    overmind,
                    Span::Warning,
                    "Здесь нельзя разместить ядро!",
                );
                return false;
            }
            if !objects_tile_clear(world, overmind, pos) {
                return false;
            }
            if !pop_override && t <= state.manual_place_at && t <= state.auto_place_at {
                tell_mob(
                    world,
                    overmind,
                    Span::Warning,
                    "Ещё слишком рано размещать ядро!",
                );
                return false;
            }
        }
        Placement::Random => {
            if let Some(pos) = blobspawn(world) {
                world.entity_mut(overmind).insert(pos);
            }
        }
        Placement::Force => {}
    }
    let Some(&pos) = world.get::<TilePos>(overmind) else {
        return false;
    };
    let core = state.core.filter(|&c| world.get::<Blob>(c).is_some());
    match core {
        Some(core) if state.placed => {
            let old = world.get::<TilePos>(core).copied();
            world.entity_mut(core).insert(pos);
            if let Some(old) = old {
                crate::construction::tile_changed(world, old);
            }
            crate::construction::tile_changed(world, pos);
        }
        _ => {
            // A blob under the forced core goes first.
            if let Some(blob) = blob_at(world, pos) {
                world.despawn(blob);
            }
            let Some(core) = structures::spawn_blob(world, Kind::Core, pos, Some(overmind)) else {
                return false;
            };
            if let Some(mut o) = world.get_mut::<BlobOvermind>(overmind) {
                o.core = Some(core);
            }
        }
    }
    if let Some(mut o) = world.get_mut::<BlobOvermind>(overmind) {
        o.placed = true;
        o.announce_at = Some(t + ANNOUNCE_SECS);
    }
    info!("blob {overmind} placed its core at {pos:?}");
    refresh_buttons(world, overmind);
    true
}

/// `/mob/eye/blob/Move`: placed, it keeps within a tile of its blob; before that anywhere with
/// ground (not on shuttles).
fn move_overminds(world: &mut World, mut steps: Local<MessageCursor<BlobStep>>) {
    let steps: Vec<BlobStep> = steps
        .read(world.resource::<Messages<BlobStep>>())
        .copied()
        .collect();
    for BlobStep { overmind, dir } in steps {
        let (Some(state), Some(&pos)) = (
            world.get::<BlobOvermind>(overmind).cloned(),
            world.get::<TilePos>(overmind),
        ) else {
            continue;
        };
        if let Some(mut facing) = world.get_mut::<Facing>(overmind) {
            facing.set_if_neq(Facing(dir));
        }
        let to = pos.step(dir);
        if !world.resource::<SpatialIndex>().in_bounds(to) {
            continue;
        }
        let allowed = if state.placed {
            !blobs_in_range(world, to, CAMERA_STRAY).is_empty()
        } else {
            !groundless(world, to) && !on_shuttle(world, to)
        };
        if allowed {
            world.entity_mut(overmind).insert(to);
        }
    }
}

/// `istype(get_area(loc), /area/shuttle)`.
fn on_shuttle(world: &World, pos: TilePos) -> bool {
    crate::teleport::area_flags(world, pos)
        .0
        .and_then(|a| world.get::<ss13_mapload::DmPower>(a))
        .is_some_and(|p| crate::obj_damage::is_under(&p.path, "/area/shuttle"))
}

/// `/mob/eye/blob/ClickOn`.
fn overmind_clicks(world: &mut World, mut clicks: Local<MessageCursor<BlobClick>>) {
    let clicks: Vec<BlobClick> = clicks
        .read(world.resource::<Messages<BlobClick>>())
        .copied()
        .collect();
    for click in clicks {
        if world.get::<BlobOvermind>(click.overmind).is_none()
            || world.get::<PlayerMob>(click.overmind).map(|p| p.client) != Some(click.client)
        {
            continue;
        }
        let m = click.modifiers;
        let tile = click.target_pos;
        if m.middle {
            tell_mob(
                world,
                click.overmind,
                Span::Notice,
                "Вы собираете свои споры.",
            );
        } else if m.alt {
            remove_blob(world, click.overmind, tile);
        } else if m.ctrl {
            create_shield(world, click.overmind, tile);
        } else {
            expand_blob(world, click.overmind, tile);
        }
        refresh_buttons(world, click.overmind);
    }
}

/// `expand_blob`: grows onto `tile` from a blob next to it, hitting what lives there.
pub fn expand_blob(world: &mut World, overmind: Entity, tile: TilePos) {
    let Some(state) = world.get::<BlobOvermind>(overmind).cloned() else {
        return;
    };
    let t = crate::tools::now(world);
    if t < state.last_attack {
        return;
    }
    let possible: Vec<(Entity, TilePos, Kind)> = blobs_in_range(world, tile, 1);
    if possible.is_empty() {
        tell_mob(
            world,
            overmind,
            Span::Warning,
            "Рядом с целевым тайлом нет блоба!",
        );
        return;
    }
    let cost = state.expand_cost;
    if !can_buy(world, overmind, cost) {
        return;
    }
    let mut attacked = false;
    let living: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .on_tile(tile)
        .iter()
        .copied()
        .filter(|&e| world.get::<Health>(e).is_some())
        .collect();
    for victim in living {
        if is_ally(world, victim) || world.get::<crate::spells::Jaunting>(victim).is_some() {
            continue;
        }
        if world.get::<Stat>(victim) != Some(&Stat::Dead) {
            attacked = true;
        }
        attack_living(world, overmind, victim);
    }
    if blob_at(world, tile).is_some() {
        if attacked {
            add_points(world, overmind, ATTACK_REFUND);
        } else {
            tell_mob(world, overmind, Span::Warning, "Там уже есть блоб!");
            add_points(world, overmind, cost);
        }
    } else {
        directional_attack(world, overmind, tile, &possible, attacked, cost);
    }
    if let Some(mut o) = world.get_mut::<BlobOvermind>(overmind) {
        o.last_attack = t + if attacked {
            CLICK_CD_MELEE
        } else {
            CLICK_CD_RAPID
        };
    }
}

/// `directional_attack`: a cardinal neighbour grows onto the tile; a diagonal one only hits.
fn directional_attack(
    world: &mut World,
    overmind: Entity,
    tile: TilePos,
    possible: &[(Entity, TilePos, Kind)],
    attacked: bool,
    cost: f32,
) {
    let cardinal: Vec<Entity> = possible
        .iter()
        .filter(|(_, p, _)| p.x.abs_diff(tile.x) + p.y.abs_diff(tile.y) == 1)
        .map(|(e, ..)| *e)
        .collect();
    if !cardinal.is_empty() {
        let from = cardinal[rand_u(world, 0, cardinal.len() as u32 - 1) as usize];
        if structures::expand(world, from, Some(tile), Some(overmind), true).is_none() {
            add_points(world, overmind, ATTACK_REFUND);
        }
    } else if attacked {
        add_points(world, overmind, ATTACK_REFUND);
    } else {
        add_points(world, overmind, cost);
    }
}

/// `remove_blob`: gives back its `point_return`.
pub fn remove_blob(world: &mut World, overmind: Entity, tile: TilePos) {
    let Some(blob) = blob_at(world, tile) else {
        tell_mob(world, overmind, Span::Warning, "Там нет блоба!");
        return;
    };
    let Some(kind) = world.get::<Blob>(blob).map(|b| b.kind) else {
        return;
    };
    let back = kind.point_return();
    if back < 0 {
        tell_mob(
            world,
            overmind,
            Span::Warning,
            "Этот блоб невозможно удалить.",
        );
        return;
    }
    let Some(state) = world.get::<BlobOvermind>(overmind).cloned() else {
        return;
    };
    if state.max_points < back as f32 + state.points {
        tell_mob(
            world,
            overmind,
            Span::Warning,
            "У вас слишком много ресурсов, чтобы удалить этот блоб!",
        );
        return;
    }
    if back > 0 {
        add_points(world, overmind, back as f32);
        tell_mob(
            world,
            overmind,
            Span::Notice,
            format!("Получено {back} ресурсов за удаление блоба."),
        );
    }
    structures::destroy(world, blob);
}

/// `create_special`: a normal blob on `tile` becomes `kind` for `price`.
fn create_special(
    world: &mut World,
    overmind: Entity,
    price: f32,
    kind: Kind,
    min_separation: u32,
    needs_node: bool,
    tile: TilePos,
) -> Option<Entity> {
    let Some(blob) = blob_at(world, tile) else {
        tell_mob(world, overmind, Span::Warning, "Здесь нет блоба!");
        return None;
    };
    if world.get::<Blob>(blob).map(|b| b.kind) != Some(Kind::Normal) {
        tell_mob(
            world,
            overmind,
            Span::Warning,
            "Этот блоб не подходит, найдите обычный.",
        );
        return None;
    }
    if needs_node {
        let at = world.get::<TilePos>(overmind).copied().unwrap_or(tile);
        if !blobs_allowed(world, at) {
            tell_mob(
                world,
                overmind,
                Span::Warning,
                "Этот тип блоба можно разместить только на станции!",
            );
            return None;
        }
        let nodes_required = world
            .get::<BlobOvermind>(overmind)
            .is_some_and(|o| o.nodes_required);
        if nodes_required && !near_node_or_core(world, tile) {
            tell_mob(
                world,
                overmind,
                Span::Warning,
                "Нужно разместить ближе к узлу или ядру!",
            );
            return None;
        }
    }
    if min_separation > 0
        && blobs_in_range(world, tile, min_separation)
            .iter()
            .any(|(e, _, k)| *e != blob && *k == kind)
    {
        tell_mob(
            world,
            overmind,
            Span::Warning,
            format!(
                "Рядом есть похожий блоб, отойдите от него дальше чем на {min_separation} тайлов!"
            ),
        );
        return None;
    }
    if !can_buy(world, overmind, price) {
        return None;
    }
    structures::change_to(world, blob, kind)
}

/// A node within 3 tiles or the core within 4 (`orange`).
fn near_node_or_core(world: &World, tile: TilePos) -> bool {
    blobs_in_range(world, tile, CORE_REACH)
        .iter()
        .any(|(_, p, k)| {
            let d = tile.chebyshev(*p).unwrap_or(u32::MAX);
            d >= 1
                && ((*k == Kind::Node && d <= NODE_REACH) || (*k == Kind::Core && d <= CORE_REACH))
        })
}

/// `create_shield`: a strong blob, or a strong one made reflective.
pub fn create_shield(world: &mut World, overmind: Entity, tile: TilePos) {
    let shield =
        blob_at(world, tile).filter(|&b| world.get::<Blob>(b).is_some_and(|b| b.kind.shield()));
    let Some(shield) = shield else {
        create_special(
            world,
            overmind,
            UPGRADE_STRONG_COST,
            Kind::Strong,
            0,
            false,
            tile,
        );
        return;
    };
    let Some(b) = world.get::<Blob>(shield).cloned() else {
        return;
    };
    if b.kind.reflective() {
        tell_mob(
            world,
            overmind,
            Span::Warning,
            "Этот щит уже настолько прочен, насколько вы можете его сделать!",
        );
        return;
    }
    let integrity = world
        .get::<crate::obj_damage::Integrity>(shield)
        .map_or(0.0, |i| i.current);
    if integrity < b.max_integrity * 0.5 {
        tell_mob(
            world,
            overmind,
            Span::Warning,
            "Этот щит слишком повреждён, чтобы его улучшить!",
        );
        return;
    }
    if !can_buy(world, overmind, UPGRADE_REFLECTOR_COST) {
        return;
    }
    tell_mob(
        world,
        overmind,
        Span::Warning,
        "Вы покрываете щит отражающей слизью: он будет отражать снаряды, но хуже держать взрывы.",
    );
    structures::change_to(world, shield, Kind::Reflective);
}

/// `create_blobbernaut`: the factory under the overmind makes one once a ghost signs up.
pub fn create_blobbernaut(world: &mut World, overmind: Entity) {
    let Some(&pos) = world.get::<TilePos>(overmind) else {
        return;
    };
    let factory = blob_at(world, pos).filter(|&b| {
        world
            .get::<Blob>(b)
            .is_some_and(|b| b.kind == Kind::Factory)
    });
    let Some(factory) = factory else {
        tell_mob(world, overmind, Span::Warning, "Нужно стоять на фабрике!");
        return;
    };
    let Some(b) = world.get::<Blob>(factory).cloned() else {
        return;
    };
    let busy = b
        .blobbernaut
        .is_some_and(|n| world.get::<Stat>(n).is_some_and(|s| *s != Stat::Dead))
        // `is_creating_blobbernaut`.
        || crate::ghost_poll::is_polling_target(world, factory);
    if busy {
        tell_mob(
            world,
            overmind,
            Span::Warning,
            "Эта фабрика уже поддерживает блоббернаута.",
        );
        return;
    }
    let integrity = world
        .get::<crate::obj_damage::Integrity>(factory)
        .map_or(0.0, |i| i.current);
    if integrity < b.max_integrity * 0.5 {
        tell_mob(
            world,
            overmind,
            Span::Warning,
            "Эта фабрика слишком повреждена, чтобы поддерживать блоббернаута.",
        );
        return;
    }
    if !can_buy(world, overmind, BLOBBERNAUT_COST) {
        return;
    }
    tell_mob(
        world,
        overmind,
        Span::Notice,
        "Вы пытаетесь создать блоббернаута.",
    );
    let ghosts = crate::ghost_poll::ghosts(world);
    let spec = PollSpec {
        question: "Вы хотите сыграть за блоббернаута?".to_owned(),
        role_name: "блоббернаут".to_owned(),
        secs: BLOBBERNAUT_POLL_SECS,
        ignore: Some(POLL_IGNORE_BLOB),
        target: Some(factory),
        ..default()
    };
    crate::ghost_poll::poll_candidates(world, spec, ghosts, move |world, signed_up| {
        blobbernaut_polled(world, overmind, factory, signed_up);
    });
}

/// `on_poll_concluded`: a random signed-up ghost gets the blobbernaut, or the overmind its
/// resources back.
fn blobbernaut_polled(
    world: &mut World,
    overmind: Entity,
    factory: Entity,
    mut signed_up: Vec<crate::ghost_poll::Candidate>,
) {
    if world.get_entity(overmind).is_err() {
        return;
    }
    let Some(chosen) = crate::ghost_poll::pick_candidate(world, &mut signed_up) else {
        tell_mob(
            world,
            overmind,
            Span::Warning,
            "Не удалось вселить разум в блоббернаута. Ресурсы возвращены, попробуйте позже.",
        );
        add_points(world, overmind, BLOBBERNAUT_COST);
        return;
    };
    let Some(naut) =
        minions::spawn_blobbernaut(world, factory, overmind, chosen.client, chosen.mob)
    else {
        add_points(world, overmind, BLOBBERNAUT_COST);
        return;
    };
    // `assign_blobbernaut`: the factory is much frailer while it lives.
    let integrity = world
        .get::<crate::obj_damage::Integrity>(factory)
        .map_or(0.0, |i| i.current);
    let max = Kind::Factory.max_hp() * 0.25;
    if let Some(mut b) = world.get_mut::<Blob>(factory) {
        b.blobbernaut = Some(naut);
        b.max_integrity = max;
    }
    structures::set_integrity(world, factory, integrity.min(max));
    let Some(&pos) = world.get::<TilePos>(factory) else {
        return;
    };
    for m in crate::pull::visible_message(
        world,
        pos,
        ChatChannel::Warning,
        (
            Span::BoldWarning,
            "Блоббернаут прорывается наружу из фабрики!".to_owned(),
        ),
        &[],
    ) {
        world.write_message(m);
    }
}

/// `relocate_core`: the core and the node under the overmind swap places.
pub fn relocate_core(world: &mut World, overmind: Entity) {
    let Some(&tile) = world.get::<TilePos>(overmind) else {
        return;
    };
    let node = blob_at(world, tile)
        .filter(|&b| world.get::<Blob>(b).is_some_and(|b| b.kind == Kind::Node));
    let Some(node) = node else {
        tell_mob(
            world,
            overmind,
            Span::Warning,
            "Нужно стоять на узле блоба!",
        );
        return;
    };
    let core = world
        .get::<BlobOvermind>(overmind)
        .and_then(|o| o.core)
        .filter(|&c| world.get::<Blob>(c).is_some());
    let Some(core) = core else {
        tell_mob(
            world,
            overmind,
            Span::Danger,
            "У вас нет ядра, и вы вот-вот умрёте! Покойтесь с миром.",
        );
        return;
    };
    if crate::construction::is_space(world, tile) || !blobs_allowed(world, tile) {
        tell_mob(
            world,
            overmind,
            Span::Warning,
            "Сюда нельзя перенести ядро!",
        );
        return;
    }
    if !can_buy(world, overmind, RELOCATE_COST) {
        return;
    }
    let Some(&old) = world.get::<TilePos>(core) else {
        return;
    };
    world.entity_mut(core).insert(tile);
    world.entity_mut(node).insert(old);
    crate::construction::tile_changed(world, old);
    crate::construction::tile_changed(world, tile);
}

/// `strain_reroll` -> `open_reroll_menu`: six strains to pick from (kept until one is taken).
pub fn strain_reroll(world: &mut World, overmind: Entity) {
    let Some(state) = world.get::<BlobOvermind>(overmind).cloned() else {
        return;
    };
    if state.free_rerolls == 0 && state.points < REROLL_COST {
        tell_mob(
            world,
            overmind,
            Span::Warning,
            format!("Нужно как минимум {REROLL_COST} ресурсов, чтобы снова сменить штамм!"),
        );
        return;
    }
    let mut choices = state.reroll_choices.clone();
    if choices.is_empty() {
        let mut pool: Vec<Strain> = Strain::ALL
            .into_iter()
            .filter(|s| *s != state.strain)
            .collect();
        while choices.len() < REROLL_CHOICES && !pool.is_empty() {
            let i = rand_u(world, 0, pool.len() as u32 - 1) as usize;
            choices.push(pool.remove(i));
        }
        if let Some(mut o) = world.get_mut::<BlobOvermind>(overmind) {
            o.reroll_choices = choices.clone();
        }
    }
    let Some(client) = world.get::<PlayerMob>(overmind).map(|p| p.client) else {
        return;
    };
    let items = choices
        .iter()
        .map(|s| format!("{}: {}", s.def().name, s.def().analyzer_damage))
        .collect();
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            overmind,
            "Смена штамма",
            "Выберите новый штамм",
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(BlobAsk::Reroll(overmind));
}

fn take_reroll(world: &mut World, overmind: Entity, index: usize) {
    let Some(state) = world.get::<BlobOvermind>(overmind).cloned() else {
        return;
    };
    let Some(&strain) = state.reroll_choices.get(index) else {
        return;
    };
    if state.free_rerolls == 0 && !can_buy(world, overmind, REROLL_COST) {
        return;
    }
    set_strain(world, overmind, strain);
    let t = crate::tools::now(world);
    if let Some(mut o) = world.get_mut::<BlobOvermind>(overmind) {
        o.free_rerolls = o.free_rerolls.saturating_sub(1);
        o.last_reroll = t;
        o.reroll_choices.clear();
    }
}

/// `jump_to_node`: a list of the overmind's nodes.
fn jump_to_node(world: &mut World, overmind: Entity) {
    let mut nodes: Vec<(Entity, TilePos)> = world
        .query::<(Entity, &Blob, &TilePos)>()
        .iter(world)
        .filter(|(_, b, _)| b.kind == Kind::Node && b.overmind == Some(overmind))
        .map(|(e, _, p)| (e, *p))
        .collect();
    if nodes.is_empty() {
        return;
    }
    nodes.sort_by_key(|(e, _)| *e);
    let Some(client) = world.get::<PlayerMob>(overmind).map(|p| p.client) else {
        return;
    };
    let items = nodes
        .iter()
        .enumerate()
        .map(|(i, (_, p))| {
            let area = crate::teleport::area_flags(world, *p)
                .0
                .map(|a| name_of(world, a))
                .unwrap_or_default();
            format!("Узел блоба #{} ({area})", i + 1)
        })
        .collect();
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            overmind,
            "Прыжок к узлу",
            "Выберите узел",
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(BlobAsk::Node(
        overmind,
        nodes.into_iter().map(|(e, _)| e).collect(),
    ));
}

fn prompt_answers(world: &mut World, mut answers: Local<MessageCursor<PromptAnswered>>) {
    let pending: Vec<(Entity, Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.answer.clone()))
        .collect();
    for (prompt, client, answer) in pending {
        let Some(asked) = world.get::<BlobAsk>(prompt).cloned() else {
            continue;
        };
        let Some(PromptAnswer::Item(index)) = answer else {
            continue;
        };
        let overmind = match &asked {
            BlobAsk::Reroll(o) | BlobAsk::Node(o, _) => *o,
        };
        if world.get::<PlayerMob>(overmind).map(|p| p.client) != Some(client) {
            continue;
        }
        match asked {
            BlobAsk::Reroll(_) => take_reroll(world, overmind, index),
            BlobAsk::Node(_, nodes) => {
                if let Some(&node) = nodes.get(index)
                    && world.get::<Blob>(node).is_some()
                    && let Some(&pos) = world.get::<TilePos>(node)
                {
                    world.entity_mut(overmind).insert(pos);
                }
            }
        }
        refresh_buttons(world, overmind);
    }
}

/// `jump_to_core/Click`: places the core first if it is not, then goes to it.
fn jump_to_core(world: &mut World, overmind: Entity) {
    let placed = world
        .get::<BlobOvermind>(overmind)
        .is_some_and(|o| o.placed);
    if !placed {
        place_core(world, overmind, Placement::Normal, false);
    }
    let core = world.get::<BlobOvermind>(overmind).and_then(|o| o.core);
    if let Some(&pos) = core.and_then(|c| world.get::<TilePos>(c)) {
        world.entity_mut(overmind).insert(pos);
    }
}

// ---------------------------------------------------------------------------------------------
// Buttons

fn button(
    world: &World,
    id: &str,
    name: String,
    state: &str,
    maptext: Option<String>,
) -> ActionButton {
    ActionButton {
        id: id.to_owned(),
        name,
        icon: world
            .get_resource::<IconIndex>()
            .and_then(|i| i.get(HUD_DMI, state)),
        background: None,
        border: None,
        available: true,
        cooldown: 0.0,
        active: false,
        maptext,
    }
}

/// The `blob_overmind` HUD as buttons.
pub fn refresh_buttons(world: &mut World, overmind: Entity) {
    let Some(state) = world.get::<BlobOvermind>(overmind).cloned() else {
        return;
    };
    let core_health = state
        .core
        .and_then(|c| {
            let max = world.get::<Blob>(c)?.max_integrity;
            let current = world.get::<crate::obj_damage::Integrity>(c)?.current;
            Some((current / max * 100.0).round())
        })
        .map(|h| format!("{h}%"));
    let core_name = if state.placed {
        "Прыжок к ядру"
    } else {
        "Разместить ядро"
    };
    let reroll = if state.free_rerolls > 0 {
        "Сменить штамм (бесплатно)".to_owned()
    } else {
        format!("Сменить штамм ({REROLL_COST})")
    };
    let buttons = vec![
        button(
            world,
            B_POWER,
            "Ресурсы блоба".to_owned(),
            "block",
            Some(format!("{}", state.points.round())),
        ),
        button(
            world,
            B_HEALTH,
            "Здоровье ядра".to_owned(),
            "corehealth",
            core_health,
        ),
        button(world, B_CORE, core_name.to_owned(), "ui_tocore", None),
        button(world, B_NODE, "Прыжок к узлу".to_owned(), "ui_tonode", None),
        button(
            world,
            B_NAUT,
            format!("Создать блоббернаута ({BLOBBERNAUT_COST})"),
            "ui_blobbernaut",
            None,
        ),
        button(
            world,
            B_RESOURCE,
            format!("Создать ресурсный блоб ({RESOURCE_COST})"),
            "ui_resource",
            None,
        ),
        button(
            world,
            B_NODE_BLOB,
            format!("Создать узел ({NODE_COST})"),
            "ui_node",
            None,
        ),
        button(
            world,
            B_FACTORY,
            format!("Создать фабрику ({FACTORY_COST})"),
            "ui_factory",
            None,
        ),
        button(world, B_READAPT, reroll, "ui_chemswap", None),
        button(
            world,
            B_RELOCATE,
            format!("Перенести ядро ({RELOCATE_COST})"),
            "ui_swap",
            None,
        ),
    ];
    set_buttons(
        world,
        overmind,
        buttons.into_iter().map(|b| (b, 0.0)).collect(),
    );
}

fn trigger_buttons(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        if actor != source || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client) {
            continue;
        }
        if id == B_POP && world.get::<BlobHost>(actor).is_some() {
            pop(world, actor, false);
            continue;
        }
        if world.get::<BlobOvermind>(actor).is_none() {
            continue;
        }
        let Some(&tile) = world.get::<TilePos>(actor) else {
            continue;
        };
        match id.as_str() {
            B_CORE => jump_to_core(world, actor),
            B_NODE => jump_to_node(world, actor),
            B_NAUT => create_blobbernaut(world, actor),
            B_RESOURCE => {
                create_special(
                    world,
                    actor,
                    RESOURCE_COST,
                    Kind::Resource,
                    RESOURCE_MIN_DISTANCE,
                    true,
                    tile,
                );
            }
            B_NODE_BLOB => {
                create_special(
                    world,
                    actor,
                    NODE_COST,
                    Kind::Node,
                    NODE_MIN_DISTANCE,
                    false,
                    tile,
                );
            }
            B_FACTORY => {
                create_special(
                    world,
                    actor,
                    FACTORY_COST,
                    Kind::Factory,
                    FACTORY_MIN_DISTANCE,
                    true,
                    tile,
                );
            }
            B_READAPT => strain_reroll(world, actor),
            B_RELOCATE => relocate_core(world, actor),
            _ => continue,
        }
        refresh_buttons(world, actor);
    }
}

fn open_info(world: &mut World, mut acts: Local<MessageCursor<UiActed<AntagHudUi>>>) {
    let pending: Vec<(Entity, Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<AntagHudUi>>>())
        .map(|a| (a.client, a.actor, a.source))
        .collect();
    for (client, actor, source) in pending {
        if world.get::<BlobOvermind>(source).is_some() || world.get::<BlobHost>(source).is_some() {
            world.write_message(OpenUi::new::<BlobInfoWindow>(
                client, actor, source, ANTAG_NAME,
            ));
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The infection

/// `add_antag_datum(/datum/antagonist/blob/infection)` on a living crew member: the greeting
/// and the pop button, which goes off by itself in six minutes. False if it already is one.
pub fn make_host(world: &mut World, mob: Entity) -> bool {
    if world.get::<BlobHost>(mob).is_some() || world.get::<Health>(mob).is_none() {
        return false;
    }
    let client = world.get::<PlayerMob>(mob).map(|p| p.client);
    let key = client
        .and_then(|c| world.get::<ClientToken>(c))
        .map(|t| t.0.clone())
        .unwrap_or_default();
    let t = crate::tools::now(world);
    let hud_icon = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(ANTAG_HUD_DMI, "blob"));
    world.entity_mut(mob).insert((
        BlobHost {
            key,
            pop_at: t + AUTO_PLACE_SECS,
        },
        AntagHudData {
            hud_icon,
            ..default()
        },
    ));
    let pop_icon = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(structures::BLOB_DMI, "blob"));
    let mut buttons: Vec<(ActionButton, f64)> = world
        .get::<ActionButtons>(mob)
        .map(|b| b.0.iter().filter(|(b, _)| b.id != B_POP).cloned().collect())
        .unwrap_or_default();
    buttons.push((
        ActionButton {
            id: B_POP.to_owned(),
            name: "Лопнуть".to_owned(),
            icon: pop_icon,
            background: None,
            border: None,
            available: true,
            cooldown: 0.0,
            active: false,
            maptext: None,
        },
        0.0,
    ));
    set_buttons(world, mob, buttons);
    if let Some(client) = client {
        tell(world, client, Span::Bold, format!("Вы - {ANTAG_NAME}."));
        tell(
            world,
            client,
            Span::Notice,
            "Цель: достигните критической массы!",
        );
        tell(
            world,
            client,
            Span::Notice,
            "Используйте способность «Лопнуть», чтобы разместить ядро блоба! Лучше сделать это \
             подальше от остальных: вам предстоит сразиться со всем экипажем!",
        );
        tell(
            world,
            client,
            Span::Danger,
            "Вы автоматически лопнете и разместите ядро через 6 минут.",
        );
        world.write_message(OpenUi::new::<AntagHudUi>(client, mob, mob, ""));
        world.write_message(OpenUi::new::<BlobInfoWindow>(client, mob, mob, ANTAG_NAME));
    }
    info!("{mob} is a blob host");
    true
}

/// `/datum/action/innate/blobpop/Activate`: the host bursts into an overmind with its core.
/// False when it refused (an invalid tile pressed by hand).
pub fn pop(world: &mut World, host: Entity, timer: bool) -> bool {
    if world.get::<BlobHost>(host).is_none() {
        return false;
    }
    let Some(&pos) = world.get::<TilePos>(host) else {
        return false;
    };
    let mut valid = true;
    if turf_dense(world, pos) {
        tell_mob(
            world,
            host,
            Span::Warning,
            "Это место слишком плотное для ядра!",
        );
        valid = false;
    }
    if crate::construction::is_space(world, pos)
        || !blobs_allowed(world, pos)
        || !crate::events::controller::station_levels(world).contains(&pos.z)
    {
        tell_mob(world, host, Span::Warning, "Здесь нельзя разместить ядро!");
        valid = false;
    }
    if !valid && !timer {
        return false;
    }
    if !valid {
        tell_mob(
            world,
            host,
            Span::Warning,
            "Место для ядра неподходящее, а лопнуть пора: вас перенесло в случайное место!",
        );
    }
    let client = world.get::<PlayerMob>(host).map(|p| p.client);
    world.entity_mut(host).remove::<BlobHost>();
    let points = STARTING_POINTS;
    let overmind = spawn_overmind(world, pos, points);
    if let Some(client) = client
        && crate::medical::brainmob::possess(world, client, host, overmind)
    {
        take_control(world, overmind, client);
    }
    crate::health::gib(world, host);
    let placement = if valid {
        Placement::Force
    } else {
        Placement::Random
    };
    place_core(world, overmind, placement, true);
    info!("blob host {host} popped into {overmind} at {pos:?}");
    true
}

// ---------------------------------------------------------------------------------------------
// Processing

/// `SSobj` for the blob: hosts that must pop, overminds (`/mob/eye/blob/process`), their cores
/// (`/obj/structure/blob/special/core/process`) and nodes.
fn process(world: &mut World) {
    let t = crate::tools::now(world);
    let hosts: Vec<Entity> = world
        .query::<(Entity, &BlobHost)>()
        .iter(world)
        .filter(|(_, h)| t >= h.pop_at)
        .map(|(e, _)| e)
        .collect();
    for host in hosts {
        pop(world, host, true);
    }
    let due: Vec<Entity> = world
        .query::<(Entity, &BlobOvermind)>()
        .iter(world)
        .filter(|(_, o)| t >= o.next_process)
        .map(|(e, _)| e)
        .collect();
    for overmind in due {
        if let Some(mut o) = world.get_mut::<BlobOvermind>(overmind) {
            o.next_process = t + PROCESS_SECS;
        }
        process_overmind(world, overmind, t);
    }
}

fn process_overmind(world: &mut World, overmind: Entity, t: f64) {
    let Some(state) = world.get::<BlobOvermind>(overmind).cloned() else {
        return;
    };
    let core = state.core.filter(|&c| world.get::<Blob>(c).is_some());
    let Some(core) = core else {
        if state.placed {
            // "the blob is kill".
            kill_overmind(world, overmind);
            return;
        }
        if state.manual_place_at > 0.0 && t >= state.manual_place_at {
            tell_mob(
                world,
                overmind,
                Span::BoldNotice,
                "Теперь вы можете разместить ядро блоба.",
            );
            tell_mob(
                world,
                overmind,
                Span::Danger,
                format!(
                    "Ядро будет размещено автоматически через {} с.",
                    (state.auto_place_at - t).max(0.0).round()
                ),
            );
            if let Some(mut o) = world.get_mut::<BlobOvermind>(overmind) {
                o.manual_place_at = 0.0;
            }
        }
        if t >= state.auto_place_at {
            place_core(world, overmind, Placement::Random, false);
        }
        refresh_buttons(world, overmind);
        return;
    };
    let legit = legit_count(world, overmind);
    if state.victory_at.is_none() && legit >= state.win_count {
        begin_victory(world, overmind, t);
    } else if state.free_rerolls == 0 && state.last_reroll + REROLL_FREE_SECS < t {
        tell_mob(
            world,
            overmind,
            Span::BoldNotice,
            "Вы получили ещё одну бесплатную смену штамма.",
        );
        if let Some(mut o) = world.get_mut::<BlobOvermind>(overmind) {
            o.free_rerolls = 1;
        }
    }
    let announce = !state.announced
        && state
            .announce_at
            .is_some_and(|at| t >= at || legit >= ANNOUNCE_SIZE);
    if let Some(mut o) = world.get_mut::<BlobOvermind>(overmind) {
        if o.victory_at.is_none() && o.max_count < legit {
            o.max_count = legit;
        }
        if announce {
            o.announced = true;
        }
    }
    if announce {
        let station = crate::events::station_name(world);
        crate::announce::Priority::new(
            format!(
                "Вспышка биологической угрозы 5-го уровня зафиксирована на борту {station}. \
                 Всему персоналу надлежит сдержать её распространение любой ценой!"
            ),
            "Биологическая угроза",
        )
        .sound(OUTBREAK5)
        .send(world);
    }
    if state.victory_at.is_some_and(|at| t >= at) && !state.won {
        victory(world, overmind);
        return;
    }
    // The core: `core_process` (resources, regeneration), `pulse_area`, `reinforce_area`.
    let d = state.strain.def();
    add_points(
        world,
        overmind,
        strains::BASE_POINT_RATE + d.point_rate_bonus,
    );
    if let Some(current) = world
        .get::<crate::obj_damage::Integrity>(core)
        .map(|i| i.current)
    {
        structures::set_integrity(
            world,
            core,
            current + strains::BASE_CORE_REGEN + d.core_regen_bonus,
        );
    }
    structures::pulse_area(world, core, overmind);
    structures::reinforce_area(world, core, PROCESS_SECS);
    let nodes: Vec<Entity> = world
        .query::<(Entity, &Blob)>()
        .iter(world)
        .filter(|(_, b)| b.kind == Kind::Node && b.overmind == Some(overmind))
        .map(|(e, _)| e)
        .collect();
    for node in nodes {
        if world.get::<Blob>(node).is_some() {
            structures::pulse_area(world, node, overmind);
            structures::reinforce_area(world, node, PROCESS_SECS);
        }
    }
    refresh_buttons(world, overmind);
}

/// `begin_victory`: critical mass; delta; 45 s to the end.
fn begin_victory(world: &mut World, overmind: Entity, t: f64) {
    if let Some(mut o) = world.get_mut::<BlobOvermind>(overmind) {
        o.victory_at = Some(t + VICTORY_DELAY);
        o.max_points = f32::INFINITY;
        o.points = f32::INFINITY;
    }
    crate::announce::Priority::new(
        "Биоугроза достигла критической массы. Потеря станции неминуема.",
        "Биологическая тревога",
    )
    .send(world);
    crate::security_level::set_level(world, crate::security_level::SecLevel::Delta, true);
    info!("blob {overmind} reached critical mass");
}

/// `victory` + `victory_sequence`: the objective is done, every living non-blob mob in a
/// station area dies (a spore rises over it), the round ends.
fn victory(world: &mut World, overmind: Entity) {
    let name = name_of(world, overmind);
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::to_playing_players(
            ss13_mapload::sound::files::NUKE_ALARM,
            70.0,
            false,
        ),
    );
    if let Some(mut o) = world.get_mut::<BlobOvermind>(overmind) {
        o.won = true;
    }
    let clients: Vec<Entity> = world
        .query::<&PlayerMob>()
        .iter(world)
        .map(|p| p.client)
        .collect();
    world.write_message(ss13_net::chat::server::SendChat {
        clients,
        message: ChatMessage {
            channel: ChatChannel::System,
            segments: vec![ChatSegment::new(
                Span::BoldAnnounce,
                format!("{name} поглотил станцию неудержимой волной!"),
            )],
        },
    });
    let levels = crate::events::controller::station_levels(world);
    let victims: Vec<(Entity, TilePos)> = world
        .query::<(Entity, &Health, &TilePos)>()
        .iter(world)
        .map(|(e, _, p)| (e, *p))
        .collect();
    for (mob, pos) in victims {
        if !levels.contains(&pos.z) || !blobs_allowed(world, pos) {
            continue;
        }
        if is_ally(world, mob) {
            continue;
        }
        crate::medical::organs::kill(world, mob);
        minions::spawn_free_spore(world, overmind, pos);
    }
    world.resource_mut::<crate::round::Ticker>().force_ending = true;
    record(world, overmind);
    info!("blob {overmind} won");
}

/// Keeps the overmind's line for the round-end report (updated while it lives).
fn record(world: &mut World, overmind: Entity) {
    let Some(state) = world.get::<BlobOvermind>(overmind).cloned() else {
        return;
    };
    let name = name_of(world, overmind);
    let line = BlobRecord {
        overmind,
        key: state.key,
        name,
        max_count: state.max_count,
        won: state.won,
    };
    if !world.contains_resource::<BlobRecords>() {
        world.init_resource::<BlobRecords>();
    }
    let mut records = world.resource_mut::<BlobRecords>();
    match records.0.iter_mut().find(|r| r.overmind == overmind) {
        Some(r) => *r = line,
        None => records.0.push(line),
    }
}

/// The core is gone: the overmind dies (`qdel(src)`); its blobs and minions are left without it
/// (`cleanup_overmind_blobs`), its player becomes a ghost.
pub fn kill_overmind(world: &mut World, overmind: Entity) {
    record(world, overmind);
    let blobs: Vec<Entity> = world
        .query::<(Entity, &Blob)>()
        .iter(world)
        .filter(|(_, b)| b.overmind == Some(overmind))
        .map(|(e, _)| e)
        .collect();
    for blob in blobs {
        if let Some(mut b) = world.get_mut::<Blob>(blob) {
            b.overmind = None;
        }
        structures::refresh_look(world, blob);
    }
    if let Some(client) = world.get::<PlayerMob>(overmind).map(|p| p.client)
        && let Some(ghost) = crate::ghost::ghostize_client(world, client)
        && let Some(mut g) = world.get_mut::<crate::ghost::Ghost>(ghost)
    {
        g.body = None;
    }
    world.despawn(overmind);
    crate::shuttle::clear_hostile_environment(world, crate::shuttle::Blocker::Entity(overmind));
    let minions: Vec<Entity> = world
        .query::<(Entity, &minions::BlobMinion)>()
        .iter(world)
        .filter(|(_, m)| m.overmind == Some(overmind))
        .map(|(e, _)| e)
        .collect();
    for mob in minions {
        if let Some(mut m) = world.get_mut::<minions::BlobMinion>(mob) {
            m.overmind = None;
        }
        minions::strain_changed(world, mob);
    }
    info!("blob overmind {overmind} died");
}

// ---------------------------------------------------------------------------------------------
// The round end

/// `/datum/antagonist/blob/roundend_report`: each blob and, if it lost, how big it got.
pub fn roundend_report(world: &mut World, _end: RoundEnd) -> Vec<ChatMessage> {
    let alive: Vec<Entity> = world
        .query_filtered::<Entity, With<BlobOvermind>>()
        .iter(world)
        .collect();
    for overmind in alive {
        record(world, overmind);
    }
    let records = world
        .get_resource::<BlobRecords>()
        .map(|r| r.0.clone())
        .unwrap_or_default();
    if records.is_empty() {
        return Vec::new();
    }
    let mut out = vec![ChatMessage {
        channel: ChatChannel::System,
        segments: vec![ChatSegment::new(
            Span::Bold,
            format!("{ROUNDEND_CATEGORY} были:"),
        )],
    }];
    for r in records {
        let mut segments = vec![
            ChatSegment::new(Span::Bold, r.key.clone()),
            ChatSegment::new(Span::Plain, " был "),
            ChatSegment::new(Span::Bold, r.name.clone()),
        ];
        if r.won {
            segments.push(ChatSegment::new(
                Span::GreenText,
                " и достиг критической массы!",
            ));
        } else {
            segments.push(ChatSegment::new(
                Span::Plain,
                format!(" и на пике роста захватил {} тайлов.", r.max_count),
            ));
        }
        out.push(ChatMessage {
            channel: ChatChannel::System,
            segments,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_overmind_waits_a_minute_and_places_by_six() {
        let o = BlobOvermind::new(Strain::BlazingOil, STARTING_POINTS, 100.0);
        assert_eq!(o.points, 60.0);
        assert_eq!(o.manual_place_at, 160.0);
        assert_eq!(o.auto_place_at, 460.0);
        assert_eq!(o.free_rerolls, 1);
        assert_eq!(o.win_count, 400);
        assert_eq!(o.expand_cost, 4.0);
        assert!(!o.placed);
    }

    #[test]
    fn the_rulesets_take_the_blob_roles() {
        use super::super::midround::{BLOB, BLOB_INFECTION, Candidates, ruleset};
        let r = ruleset(BLOB).unwrap();
        assert_eq!(r.role, Some(ROLE_BLOB));
        assert_eq!(r.candidates, Candidates::Ghosts);
        assert_eq!(r.min_pop, 30);
        let r = ruleset(BLOB_INFECTION).unwrap();
        assert_eq!(r.role, Some(ROLE_BLOB_INFECTION));
        assert_eq!(r.candidates, Candidates::Living);
    }
}
