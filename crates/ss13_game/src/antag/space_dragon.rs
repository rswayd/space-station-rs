//! The space dragon after tg `/datum/antagonist/space_dragon` (`space_dragon.dm`), its mob
//! (`/mob/living/basic/space_dragon`, `dragon_breath.dm`, `dragon_gust.dm`), the carp rifts
//! (`/obj/structure/carp_rift`, `carp_rift.dm`), the rift carp (`/datum/antagonist/space_carp`)
//! and the "Space Dragon" heavy midround (`/datum/dynamic_ruleset/midround/from_ghosts/space_dragon`).
//!
//! - **Selection** ([`can_spawn`], [`spawn_dragons`]): ghosts who want [`ROLE_SPACE_DRAGON`]; the
//!   dragon (400 health, 35 brute bites) appears on a carp spawn in space, the ghost takes it,
//!   names it and picks its colour (not too dark); 5-10 s later the "Неопознанная форма жизни"
//!   announcement.
//! - **Objective**: five rift areas among the station's command, engineering, science and
//!   security areas (`VALID_TERRITORY`); summon three rifts. 300 s without an open rift (the
//!   greeting's five minutes, a warning a minute before) and the abyss takes the dragon back.
//! - **Summon Rift** (button): in a chosen area without a rift, on a floor, a 10 s `do_after`;
//!   the rift charges for 300 s (a carp for ghosts every 45 s, an announcement at half charge and
//!   when charged), heals the dragon 5 brute a second next to it; charged it is indestructible,
//!   the area is spent, the dragon gets the button back, is healed and enraged (faster) for 30 s;
//!   a charged rift spawns AI carp (1.25%/s). A rift destroyed before charging: every rift goes,
//!   the dragon is slowed for good and can open none (`TRAIT_RIFT_FAILURE`). The dragon's death
//!   destroys its rifts. **Locate Rift**: the direction and distance of a chosen area.
//! - **Victory**: three charged rifts (or one and the emergency shuttle docked): the
//!   announcement and roar, a full heal and permanent rage, the rifts give ghosts carp without
//!   end.
//! - **Rift carp**: a ghost's click on a rift with carp stored asks "Стать карпом?"; the ghost
//!   becomes an advanced carp guarding it (one per ghost per rift until it has spares).
//! - **The mob** ([`dragon_clicks`]): bites the living, eats a corpse in 3 s (heals a quarter of
//!   its max health; a revived meal is spat out), tears walls in three 4 s stages (12 s for
//!   reinforced ones), pries airlocks (4 s; unpowered at once). A far click breathes fire (30
//!   burn, 20 tiles, 3 s cooldown; carp in the line are invigorated, faster for 8 s instead); a
//!   far right click is the wing buffet (1.2 s windup, everyone not a carp within 4 tiles thrown
//!   back and paralyzed for 5 s, then an endlag growing with use, five times after a rift
//!   failure). A devastating explosion takes 40-75% of its max health instead of gibbing it.
//! - [`roundend_report`]: the dragon, its objective and how often each player was its carp.
// shortcut: no ghost poll (willing ghosts are taken), no wavespeak telepathy, no gravity aura,
// the rift's light colour and drifting, the shark form and fishing, the outline glow and the
// gust/overlay sprites are not ported; the dragon pries doors in combat mode too (airlocks have
// no integrity to attack); only walls are torn (not other closed turfs); the rift's explosion
// cap (`min(EXPLODE_HEAVY)`) and telekinesis immunity are missing; ghosts are not notified of
// rifts and spare carp.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::spawner::Rng;
use ss13_mapload::{Areas, DmObj, DmPower, IconIndex};
use ss13_net::server::ClientToken;
use ss13_net::sound::server::SendSound;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::protocol::SoundEvent;
use ss13_shared::{Appearance, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiSystems};
use ss13_world::{Mover, SpatialIndex};

use super::dynamic::DynamicRng;
use super::objectives::RoundEnd;
use super::{AntagHudData, AntagHudUi, BrotherInfo, ObjectiveInfo};
use crate::actions::{ActionButton, ActionButtons, ActionButtonsUi, set_buttons};
use crate::click::{ClickResolved, RangedClick};
use crate::combat::{CLICK_CD_MELEE, NextMove};
use crate::construction::Job;
use crate::health::{DamageType, Health, Stat};
use crate::obj_damage::{INDESTRUCTIBLE, Integrity};
use crate::server::{PlayerMob, SpawnPoints};
use crate::status_effects::Incap;
use crate::tools::{Actor, DoAfter, do_after, now, tell, tell_mob};

pub use super::ROLE_SPACE_DRAGON;
/// `name` ("\improper Space Dragon").
pub const ANTAG_NAME: &str = "Космический дракон";
const ROUNDEND_CATEGORY: &str = "Космические драконы";
pub const DRAGON_MOB: &str = "/mob/living/basic/space_dragon";
pub const RIFT: &str = "/obj/structure/carp_rift";
/// `minion_to_spawn`, `ai_to_spawn`.
pub const RIFT_CARP: &str = "/mob/living/basic/carp/advanced";
pub const AI_CARP: &str = "/mob/living/basic/carp";
const SPACE_DRAGON_JOB: &str = "/datum/job/space_dragon";

/// `maxRiftTimer`, seconds (tg counts `Life` ticks; the greeting promises five minutes).
pub const MAX_RIFT_TIMER: f64 = 300.0;
/// `max_charge`, `carp_interval` (seconds), `max_integrity`.
pub const RIFT_MAX_CHARGE: f64 = 300.0;
pub const CARP_INTERVAL: f64 = 45.0;
const RIFT_INTEGRITY: f32 = 300.0;
/// `aura_healing` `simple_heal` (per second, range 1).
const RIFT_HEAL: f32 = 5.0;
/// `SPT_PROB(1.25)` AI carp per second from a charged rift.
const CHARGED_CARP_CHANCE: f32 = 1.25;
/// `summon_rift` `do_after(10 SECONDS)`, `try_eat` 3 s, `wall_tearer` 4 s ×3, `door_pryer` 4 s;
/// deciseconds.
const RIFT_DS: f32 = 100.0;
const EAT_DS: f32 = 30.0;
const TEAR_DS: f32 = 40.0;
const REINFORCED_MULT: f32 = 3.0;
const PRY_DS: f32 = 40.0;
/// `rift_empower` lasts 30 s.
const EMPOWER_SECS: f64 = 30.0;
/// `dragon_rage` (-0.5) and `dragon_depression` (+5) `multiplicative_slowdown`, seconds.
const RAGE_SPEEDUP: f32 = 0.05;
const DEPRESSION_SLOWDOWN: f32 = 0.5;
/// `fire_breath/carp`: `fire_damage`, `fire_range`, `fire_temperature`; the base action's
/// `cooldown_time`, `fire_delay`, `DRAKE_FIRE_EXPOSURE`.
const BREATH_DAMAGE: f32 = 30.0;
const BREATH_RANGE: i32 = 20;
const BREATH_TEMP: f64 = 700.0;
const BREATH_COOLDOWN: f64 = 3.0;
const BREATH_DELAY: f64 = 0.15;
const BREATH_VOLUME: f64 = 50.0;
/// `carp_invigoration` `duration`.
const INVIGORATION_SECS: f64 = 8.0;
/// `wing_buffet`: `cooldown_time`, `gust_distance`, `windup_time`, `minimum_endlag`,
/// `endlag_per_activation`, its decay per second, `exhaustion_multiplier`; the paralysis.
const BUFFET_COOLDOWN: f64 = 1.0;
const GUST_DISTANCE: u32 = 4;
const BUFFET_WINDUP: f64 = 1.2;
const MIN_ENDLAG: f64 = 0.4;
const ENDLAG_PER_USE: f64 = 0.3;
const ENDLAG_DECAY: f64 = 0.3 / 60.0;
const EXHAUSTION_MULT: f64 = 5.0;
const GUST_PARALYZE: f32 = 5.0;
const GUST_THROW: i32 = 10;
/// `devastation_damage_min/max_percentage`.
const DEVASTATION_MIN: f32 = 0.4;
const DEVASTATION_MAX: f32 = 0.75;
/// `REJECT_DARK_COLOUR_THRESHOLD` (HSV value, 0-100).
const DARK_THRESHOLD: f32 = 20.0;
/// `forge_objectives`: the rift areas and how many.
const RIFT_AREA_ROOTS: [&str; 4] = [
    "/area/station/command",
    "/area/station/engineering",
    "/area/station/science",
    "/area/station/security",
];
const RIFT_AREAS: usize = 5;
const VALID_TERRITORY: u32 = 1 << 0;

const ROAR: &str = "sound/mobs/non-humanoids/space_dragon/space_dragon_roar.ogg";
const DEMON_ATTACK: &str = "sound/effects/magic/demon_attack1.ogg";
const DEMON_DIES: &str = "sound/effects/magic/demon_dies.ogg";
const ROCKET: &str = "sound/vehicles/rocketlaunch.ogg";
const ARRIVE: &str = "sound/effects/magic/ethereal_exit.ogg";
const FIREBALL: &str = "sound/effects/magic/fireball.ogg";
const GRAVHIT: &str = "sound/effects/gravhit.ogg";
const PRYING: &str = "sound/machines/airlock/airlock_alien_prying.ogg";
const SPLAT: &str = "sound/effects/splat.ogg";
const VOMIT: &str = "/obj/effect/decal/cleanable/vomit";

const DRAGON_DMI: &str = "icons/mob/actions/actions_space_dragon.dmi";
const MAGIC_DMI: &str = "icons/effects/magic.dmi";
const BACKGROUNDS_DMI: &str = "icons/mob/actions/backgrounds.dmi";
const SUMMON_ID: &str = "dragon:summon_rift";
const LOCATE_ID: &str = "dragon:locate_rift";
const BREATH_ID: &str = "dragon:fire_breath";
const BUFFET_ID: &str = "dragon:wing_buffet";

/// `do_after` jobs of the dragon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragonJob {
    Rift,
    Eat,
    Tear,
    Pry,
}

/// The wing buffet under way.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Gust {
    Idle,
    /// Rising: the slam at `.0`.
    Rising(f64),
    /// Tired after the slam until `.0`.
    Tired(f64),
}

/// On the dragon: `/datum/antagonist/space_dragon` and the mob's state.
#[derive(Component, Clone, Debug)]
pub struct SpaceDragon {
    pub key: String,
    /// `chosen_rift_areas` (areas with their names).
    pub rift_areas: Vec<(Entity, String)>,
    /// `riftTimer` in seconds; `None` is tg's -1 (a rift is open, or everything failed).
    pub rift_timer: Option<f64>,
    pub rifts: Vec<Entity>,
    pub rifts_charged: u32,
    pub objective_complete: bool,
    /// `TRAIT_RIFT_FAILURE` (and `dragon_depression`).
    pub rift_failure: bool,
    /// `rift_ability` exists.
    pub can_summon: bool,
    /// `locate_rift_ability` exists.
    pub can_locate: bool,
    /// The keys of the players who were its carp, once per carp.
    pub carp: Vec<String>,
    /// `dragon_rage` until then (`f64::INFINITY`: for good).
    pub rage_until: Option<f64>,
    /// The move delay without the dragon's modifiers.
    pub base_delay: f32,
    pub breath_ready: f64,
    /// Fire breath armed by its button: the next click aims it.
    pub aiming: bool,
    gust: Gust,
    gust_ready: f64,
    /// `additional_endlag`, seconds.
    endlag: f64,
}

/// A breath of fire travelling down its line.
#[derive(Component, Clone, Debug)]
struct FireLine {
    next: f64,
    /// Next tile last.
    tiles: Vec<TilePos>,
    hit: Vec<Entity>,
}

/// `carp_invigoration` on a carp: `dragon_rage` until `until`, its own delay before.
#[derive(Component, Clone, Copy, Debug)]
struct Invigorated {
    until: f64,
    delay: f32,
}

/// `CHARGE_ONGOING`, `CHARGE_FINALWARNING`, `CHARGE_COMPLETED`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Charge {
    Ongoing,
    FinalWarning,
    Completed,
}

/// `/obj/structure/carp_rift`.
#[derive(Component, Clone, Debug)]
pub struct CarpRift {
    pub dragon: Option<Entity>,
    pub time_charged: f64,
    pub carp_stored: u32,
    pub last_carp_inc: f64,
    pub charge: Charge,
    /// `ckey_list`.
    pub keys: Vec<String>,
}

impl CarpRift {
    fn new(dragon: Entity) -> Self {
        Self {
            dragon: Some(dragon),
            time_charged: 0.0,
            carp_stored: 1,
            last_carp_inc: 0.0,
            charge: Charge::Ongoing,
            keys: Vec::new(),
        }
    }
}

/// What a rift's charging second led to (`update_check`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct RiftEvents {
    pub carp_added: bool,
    pub final_warning: bool,
    pub charged: bool,
}

/// `process` of a charging rift for `dt` seconds, on the numbers alone.
pub fn charge_rift(rift: &mut CarpRift, dt: f64) -> RiftEvents {
    let mut out = RiftEvents::default();
    if rift.charge == Charge::Completed {
        return out;
    }
    rift.time_charged = (rift.time_charged + dt).min(RIFT_MAX_CHARGE);
    rift.last_carp_inc += dt;
    if rift.last_carp_inc >= CARP_INTERVAL {
        rift.carp_stored += 1;
        rift.last_carp_inc -= CARP_INTERVAL;
        out.carp_added = true;
    }
    if rift.time_charged >= RIFT_MAX_CHARGE {
        rift.charge = Charge::Completed;
        out.charged = true;
        return out;
    }
    if rift.charge < Charge::FinalWarning && rift.time_charged >= RIFT_MAX_CHARGE * 0.5 {
        rift.charge = Charge::FinalWarning;
        out.final_warning = true;
    }
    out
}

/// What the rift timer said this second (`rift_checks`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerTick {
    Nothing,
    Warning,
    Expired,
}

/// `riftTimer` after `dt` seconds.
pub fn tick_rift_timer(timer: &mut Option<f64>, dt: f64) -> TimerTick {
    let Some(t) = timer.as_mut() else {
        return TimerTick::Nothing;
    };
    let before = *t;
    *t = (*t + dt).min(MAX_RIFT_TIMER + 1.0);
    if *t >= MAX_RIFT_TIMER {
        return TimerTick::Expired;
    }
    let warn = MAX_RIFT_TIMER - 60.0;
    if before < warn && *t >= warn {
        return TimerTick::Warning;
    }
    TimerTick::Nothing
}

/// `rift_checks`'s victory condition.
pub fn victorious(rifts_charged: u32, shuttle_docked: bool) -> bool {
    rifts_charged == 3 || (shuttle_docked && rifts_charged > 0)
}

/// The dragon's move delay with its modifiers.
pub fn dragon_delay(base: f32, raging: bool, depressed: bool) -> f32 {
    let mut delay = base;
    if depressed {
        delay += DEPRESSION_SLOWDOWN;
    }
    if raging {
        delay -= RAGE_SPEEDUP;
    }
    delay.max(0.0)
}

/// `wing_buffet/ground_pound`: the stun after the slam and the endlag after it.
pub fn buffet_endlag(endlag: f64, rift_failure: bool) -> (f64, f64) {
    let mult = if rift_failure { EXHAUSTION_MULT } else { 1.0 };
    (MIN_ENDLAG + endlag * mult, endlag + ENDLAG_PER_USE * mult)
}

/// `select_colour`'s check: the HSV value at least [`DARK_THRESHOLD`].
pub fn colour_bright_enough(rgb: [u8; 3]) -> bool {
    let max = rgb.iter().copied().max().unwrap_or(0);
    f32::from(max) / 255.0 * 100.0 >= DARK_THRESHOLD
}

/// `#rrggbb` to bytes.
pub fn parse_hex(hex: &str) -> Option<[u8; 3]> {
    let hex = hex.strip_prefix('#')?;
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// `locate_rift`'s distance words (the heretic heart tracker's numbers).
pub fn distance_word(dist: i32) -> &'static str {
    match dist {
        0..=15 => "very near, ",
        16..=31 => "near, ",
        32..=127 => "far, ",
        _ => "very far, ",
    }
}

/// The rift carp's `/datum/antagonist/space_carp`.
#[derive(Component, Clone, Debug)]
pub struct SpaceCarp {
    pub rift: Entity,
}

/// The pending "Стать карпом?" question of a ghost.
#[derive(Component, Clone, Copy, Debug)]
struct CarpAsk {
    rift: Entity,
    ghost: Entity,
}

/// The pending name or colour question of a new dragon.
#[derive(Component, Clone, Copy, Debug)]
enum LoginAsk {
    Name(Entity),
    Colour(Entity),
}

/// The pending Locate Rift list.
#[derive(Component, Clone, Debug)]
struct LocateAsk {
    dragon: Entity,
    areas: Vec<Entity>,
}

/// `announce_space_dragon` due.
#[derive(Resource, Default)]
struct DragonAnnouncement(Option<f64>);

/// The dragon's info panel (the generic one with the objective).
pub struct DragonInfoWindow;

impl UiInterface for DragonInfoWindow {
    const NAME: &'static str = crate::antag::cult::AntagInfoGenericUi::NAME;
    type Data = ();
    type StaticData = BrotherInfo;
}

impl ServerUi for DragonInfoWindow {
    type Source = &'static SpaceDragon;
    type UserParam = ();
    type Act = ();

    fn static_data(d: &SpaceDragon) -> BrotherInfo {
        BrotherInfo {
            antag_name: ANTAG_NAME.to_owned(),
            objectives: vec![ObjectiveInfo {
                count: 1,
                explanation: objective_text(d),
                complete: d.objective_complete,
            }],
        }
    }

    fn data(_: &SpaceDragon, _: Entity, _: &()) {}
}

/// `summon_carp/update_explanation_text`.
pub fn objective_text(d: &SpaceDragon) -> String {
    let names: Vec<&str> = d.rift_areas.iter().map(|(_, n)| n.as_str()).collect();
    format!(
        "Откройте 3 разлома, чтобы заполнить всю станцию карпами. Возможные места для разломов: {}",
        english_list(&names)
    )
}

/// DM `english_list` with BandaStation's defaults ("ничего", " и ", ", ").
pub fn english_list(items: &[&str]) -> String {
    match items {
        [] => "ничего".to_owned(),
        [one] => (*one).to_owned(),
        [rest @ .., last] => format!("{} и {last}", rest.join(", ")),
    }
}

/// tg `SPT_PROB(percent, seconds)` as a percent chance.
fn spt_percent(percent: f32, secs: f32) -> f32 {
    (1.0 - (1.0 - percent / 100.0).powf(secs)) * 100.0
}

pub struct SpaceDragonPlugin;

impl Plugin for SpaceDragonPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<DragonInfoWindow>()
            .init_resource::<DragonAnnouncement>()
            .add_systems(
                Update,
                (
                    open_info,
                    trigger_buttons,
                    (carp_answers, login_answers, locate_answers).after(PromptSystems::Answer),
                )
                    .in_set(UiSystems::Handle),
            )
            .add_systems(
                Update,
                (dragon_clicks, ranged_clicks, ghost_clicks)
                    .after(crate::click::ResolveClicks)
                    .before(crate::health::HealthSystems),
            )
            .add_systems(
                Update,
                (process, fire_lines, gusts, announce).before(crate::health::HealthSystems),
            )
            .add_systems(
                Update,
                (on_death, rifts_gone, revived_meals).after(crate::health::HealthSystems),
            );
    }
}

fn with_rng<R>(world: &mut World, f: impl FnOnce(&mut Rng) -> R) -> R {
    if !world.contains_resource::<DynamicRng>() {
        world.insert_resource(DynamicRng(Rng::new(0)));
    }
    f(&mut world.resource_mut::<DynamicRng>().0)
}

fn name_of(world: &World, e: Entity) -> String {
    crate::security::name_of(world, e)
}

fn visible(world: &mut World, mob: Entity, span: Span, others: String, own: Option<String>) {
    let Some(pos) = crate::explosions::turf_of(world, mob) else {
        return;
    };
    let personal: Vec<(Entity, Span, String)> = own.map(|t| (mob, span, t)).into_iter().collect();
    for m in
        crate::pull::visible_message(world, pos, ChatChannel::Warning, (span, others), &personal)
    {
        world.write_message(m);
    }
}

fn play(world: &mut World, pos: TilePos, sound: &str, vol: f32) {
    crate::sound::playsound(world, crate::sound::PlaySound::new(pos, sound, vol, true));
}

/// `SEND_SOUND` to the dragon's player.
fn send_sound(world: &mut World, mob: Entity, file: &str) {
    let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) else {
        return;
    };
    if world.contains_resource::<Messages<SendSound>>() {
        world.write_message(SendSound {
            client,
            sound: SoundEvent {
                file: file.into(),
                volume: 100.0,
                speed: 1.0,
                offset: None,
            },
        });
    }
}

fn announce_title() -> String {
    format!(
        "{}: Отдел Изучения Дикой Природы",
        crate::events::COMMAND_NAME
    )
}

fn area_at(world: &World, pos: TilePos) -> Option<Entity> {
    world.get_resource::<Areas>()?.area_at(pos)
}

// ---------------------------------------------------------------------------------------------
// Selection

/// `can_be_selected`: `find_space_spawn()` (a carp spawn).
pub fn can_spawn(world: &mut World) -> bool {
    world
        .get_resource::<SpawnPoints>()
        .is_some_and(|s| !s.carp.is_empty())
}

/// `create_ruleset_body` + `assign_role` for each `(client, ghost)`, then the announcement
/// timer of `execute`.
pub fn spawn_dragons(world: &mut World, ghosts: &[(Entity, Entity)]) -> Vec<Entity> {
    let mut out = Vec::new();
    for &(client, ghost) in ghosts {
        let tiles = world
            .get_resource::<SpawnPoints>()
            .map(|s| s.carp.clone())
            .unwrap_or_default();
        if tiles.is_empty() {
            break;
        }
        let i = with_rng(world, |rng| rng.range(0, tiles.len() as i64 - 1)) as usize;
        let Some(dragon) = crate::mobs::spawn::spawn_mob(world, DRAGON_MOB, tiles[i]) else {
            break;
        };
        if !crate::medical::brainmob::possess(world, client, ghost, dragon) {
            world.despawn(dragon);
            continue;
        }
        world.try_despawn(ghost).ok();
        make_dragon(world, dragon);
        play(world, tiles[i], ARRIVE, 50.0);
        ask_name(world, dragon);
        info!("{dragon} ({client}) is a space dragon at {:?}", tiles[i]);
        out.push(dragon);
    }
    if !out.is_empty() {
        let t = now(world) + with_rng(world, |rng| rng.range(5, 10)) as f64;
        world.resource_mut::<DragonAnnouncement>().0 = Some(t);
    }
    out
}

/// `announce_space_dragon`.
fn announce(world: &mut World) {
    let Some(at) = world.resource::<DragonAnnouncement>().0 else {
        return;
    };
    if now(world) < at {
        return;
    }
    world.resource_mut::<DragonAnnouncement>().0 = None;
    let station = crate::events::station_name(world);
    crate::events::priority_announce(
        world,
        "Неопознанная форма жизни",
        &format!(
            "Вблизи {station} зафиксирован большой поток органической энергии, ожидайте дальнейших указаний."
        ),
    );
}

/// `forge_objectives`: five areas of command, engineering, science and security.
fn pick_rift_areas(world: &mut World) -> Vec<(Entity, String)> {
    let Some(areas) = world.get_resource::<Areas>() else {
        return Vec::new();
    };
    let mut pool: Vec<Entity> = areas
        .entities()
        .iter()
        .copied()
        .filter(|&a| {
            world.get::<DmPower>(a).is_some_and(|p| {
                RIFT_AREA_ROOTS.iter().any(|r| is_under(&p.path, r))
                    && (p.area_flags as u32) & VALID_TERRITORY != 0
            })
        })
        .collect();
    pool.sort();
    pool.dedup();
    let mut out = Vec::new();
    while out.len() < RIFT_AREAS && !pool.is_empty() {
        let i = with_rng(world, |rng| rng.range(0, pool.len() as i64 - 1)) as usize;
        let area = pool.remove(i);
        out.push((area, crate::shuttle::area_name(world, area)));
    }
    out
}

/// `add_antag_datum(/datum/antagonist/space_dragon)`: the objective, the job, the buttons, the
/// greeting. `false` if `mob` already is one.
pub fn make_dragon(world: &mut World, mob: Entity) -> bool {
    if world.get::<SpaceDragon>(mob).is_some() || world.get_entity(mob).is_err() {
        return false;
    }
    crate::noncrew_jobs::set_assigned_role(world, mob, SPACE_DRAGON_JOB);
    let rift_areas = pick_rift_areas(world);
    let client = world.get::<PlayerMob>(mob).map(|p| p.client);
    let key = client
        .and_then(|c| world.get::<ClientToken>(c))
        .map(|t| t.0.clone())
        .unwrap_or_default();
    let base_delay = world.get::<Mover>(mob).map_or(0.0, |m| m.delay);
    world.entity_mut(mob).insert((
        SpaceDragon {
            key,
            rift_areas,
            rift_timer: Some(0.0),
            rifts: Vec::new(),
            rifts_charged: 0,
            objective_complete: false,
            rift_failure: false,
            can_summon: true,
            can_locate: true,
            carp: Vec::new(),
            rage_until: None,
            base_delay,
            breath_ready: 0.0,
            aiming: false,
            gust: Gust::Idle,
            gust_ready: 0.0,
            endlag: 0.0,
        },
        AntagHudData::default(),
    ));
    refresh_buttons(world, mob);
    if let Some(client) = client {
        tell(
            world,
            client,
            Span::Bold,
            "Мы движемся сквозь время и пространство, не смотря на их величину. Мы не помним, откуда мы явились; мы не знаем, куда мы пойдем. Весь космос принадлежит нам.\n\
             Мы являемся высшими хищниками в бездонной пустоте, и мало кто может осмелиться занять этот титул.\n\
             Но сейчас, мы лицезреем нарушителей, что борются против наших клыков с помощью немыслимой магии; их логова мелькают в глубине космоса, как маленькие огоньки.\n\
             Сегодня, мы потушим один из этих огоньков.",
        );
        tell(
            world,
            client,
            Span::BoldWarning,
            "У вас имеется пять минут, чтобы найти безопасное место для открытия первого разрыва. Если не успеете, вас вернет в бездну, из которой вы прибыли.",
        );
        let text = world
            .get::<SpaceDragon>(mob)
            .map(objective_text)
            .unwrap_or_default();
        tell(world, client, Span::Plain, format!("Задача №1: {text}"));
        world.write_message(OpenUi::new::<AntagHudUi>(client, mob, mob, ""));
        world.write_message(OpenUi::new::<DragonInfoWindow>(
            client, mob, mob, ANTAG_NAME,
        ));
    }
    send_sound(world, mob, DEMON_ATTACK);
    true
}

// ---------------------------------------------------------------------------------------------
// Login: name and colour

fn ask_name(world: &mut World, dragon: Entity) {
    let Some(client) = world.get::<PlayerMob>(dragon).map(|p| p.client) else {
        return;
    };
    let default = name_of(world, dragon);
    let prompt = ss13_tgui::prompt::ask_world(
        world,
        Ask::new(
            client,
            dragon,
            "Выбор имени",
            "Какое имя вы хотите задать?",
            PromptKind::Text {
                default,
                max_length: crate::bots::MAX_NAME_LEN,
                multiline: false,
            },
        ),
    );
    world.entity_mut(prompt).insert(LoginAsk::Name(dragon));
}

fn ask_colour(world: &mut World, dragon: Entity) {
    let Some(client) = world.get::<PlayerMob>(dragon).map(|p| p.client) else {
        return;
    };
    let prompt = ss13_tgui::prompt::ask_world(
        world,
        Ask::new(
            client,
            dragon,
            "Выбор цвета",
            "Какого цвета вы хотите быть?",
            PromptKind::Color {
                default: "#ffffff".to_owned(),
                autofocus: true,
            },
        ),
    );
    world.entity_mut(prompt).insert(LoginAsk::Colour(dragon));
}

/// `rename_dragon` and `select_colour`: asked again until the answer is valid.
fn login_answers(world: &mut World, mut answers: Local<MessageCursor<PromptAnswered>>) {
    let pending: Vec<(Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.answer.clone()))
        .collect();
    for (prompt, answer) in pending {
        let Some(ask) = world.get::<LoginAsk>(prompt).copied() else {
            continue;
        };
        match ask {
            LoginAsk::Name(dragon) => {
                if world.get::<SpaceDragon>(dragon).is_none() {
                    continue;
                }
                let name = match answer {
                    Some(PromptAnswer::Text(t)) => {
                        crate::comms_server::reject_bad_text(&t, crate::bots::MAX_NAME_LEN as usize)
                            .map(|t| crate::bots::sanitize_name(&t))
                            .filter(|t| !t.is_empty())
                    }
                    _ => None,
                };
                let Some(name) = name else {
                    tell_mob(
                        world,
                        dragon,
                        Span::Warning,
                        "Это имя некорректно, попробуйте еще раз.",
                    );
                    ask_name(world, dragon);
                    continue;
                };
                tell_mob(
                    world,
                    dragon,
                    Span::Notice,
                    format!("Имя вам - {name}, устрашающий космический дракон."),
                );
                world.entity_mut(dragon).insert(Name::new(name));
                ask_colour(world, dragon);
            }
            LoginAsk::Colour(dragon) => {
                if world.get::<SpaceDragon>(dragon).is_none() {
                    continue;
                }
                let rgb = match answer {
                    Some(PromptAnswer::Color(c)) => parse_hex(&c),
                    _ => None,
                };
                let Some(rgb) = rgb else {
                    tell_mob(
                        world,
                        dragon,
                        Span::Warning,
                        "Этот цвет некорректен, попробуйте еще раз.",
                    );
                    ask_colour(world, dragon);
                    continue;
                };
                if !colour_bright_enough(rgb) {
                    tell_mob(
                        world,
                        dragon,
                        Span::Danger,
                        "Этот цвет некорректен - он недостаточно светлый.",
                    );
                    ask_colour(world, dragon);
                    continue;
                }
                if let Some(mut look) = world.get_mut::<Appearance>(dragon) {
                    look.color = Some([rgb[0], rgb[1], rgb[2], 255]);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Buttons

fn refresh_buttons(world: &mut World, mob: Entity) {
    let Some(d) = world.get::<SpaceDragon>(mob).cloned() else {
        return;
    };
    let icon = |world: &World, dmi: &str, state: &str| {
        world
            .get_resource::<IconIndex>()
            .and_then(|i| i.get(dmi, state))
    };
    let ours = [SUMMON_ID, LOCATE_ID, BREATH_ID, BUFFET_ID];
    let mut buttons: Vec<(ActionButton, f64)> = world
        .get::<ActionButtons>(mob)
        .map(|b| {
            b.0.iter()
                .filter(|(button, _)| !ours.contains(&button.id.as_str()))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let conscious = world.get::<Stat>(mob) == Some(&Stat::Conscious);
    let button =
        |world: &World, id: &str, name: &str, dmi: &str, state: &str, active: bool| ActionButton {
            id: id.to_owned(),
            name: name.to_owned(),
            icon: icon(world, dmi, state),
            background: icon(world, BACKGROUNDS_DMI, "bg_default"),
            border: icon(world, BACKGROUNDS_DMI, "bg_default_border"),
            available: conscious,
            cooldown: 0.0,
            active,
            maptext: None,
        };
    if d.can_summon {
        buttons.push((
            button(
                world,
                SUMMON_ID,
                "Summon Rift",
                DRAGON_DMI,
                "carp_rift",
                false,
            ),
            0.0,
        ));
    }
    if d.can_locate {
        buttons.push((
            button(
                world,
                LOCATE_ID,
                "Locate Rift",
                DRAGON_DMI,
                "locate_carp_rift",
                false,
            ),
            0.0,
        ));
    }
    buttons.push((
        button(
            world,
            BREATH_ID,
            "Fire Breath",
            MAGIC_DMI,
            "fireball",
            d.aiming,
        ),
        d.breath_ready,
    ));
    buttons.push((
        button(world, BUFFET_ID, "Wing Buffet", MAGIC_DMI, "tornado", false),
        d.gust_ready,
    ));
    set_buttons(world, mob, buttons);
}

fn trigger_buttons(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        if actor != source
            || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
            || world.get::<SpaceDragon>(actor).is_none()
            || world.get::<Stat>(actor) != Some(&Stat::Conscious)
        {
            continue;
        }
        match id.as_str() {
            SUMMON_ID => summon_rift(world, client, actor),
            LOCATE_ID => locate_rift(world, client, actor),
            BREATH_ID => {
                if let Some(mut d) = world.get_mut::<SpaceDragon>(actor) {
                    d.aiming = !d.aiming;
                }
                refresh_buttons(world, actor);
            }
            BUFFET_ID => {
                start_gust(world, actor);
            }
            _ => {}
        }
    }
}

/// `summon_rift/Activate`: the checks, then the 10 s `do_after`.
fn summon_rift(world: &mut World, client: Entity, dragon: Entity) {
    let Some(d) = world.get::<SpaceDragon>(dragon).cloned() else {
        return;
    };
    if !d.can_summon {
        return;
    }
    let Some(pos) = world.get::<TilePos>(dragon).copied() else {
        return;
    };
    let here = area_at(world, pos);
    let taken = d
        .rifts
        .iter()
        .any(|&r| world.get::<TilePos>(r).and_then(|&p| area_at(world, p)) == here);
    if taken {
        tell(world, client, Span::Warning, "здесь уже есть разлом!");
        return;
    }
    if !here.is_some_and(|a| d.rift_areas.iter().any(|(r, _)| *r == a)) {
        tell(
            world,
            client,
            Span::Warning,
            "здесь нельзя открыть разлом! вспомните цели!",
        );
        return;
    }
    let open_space = crate::construction::turf_at(world, pos)
        .and_then(|t| crate::construction::path_of(world, t))
        .is_some_and(|p| is_under(&p, "/turf/open/openspace"));
    if open_space {
        tell(world, client, Span::Warning, "нужна поверхность!");
        return;
    }
    tell(world, client, Span::Notice, "открытие разлома...");
    let actor = Actor {
        client,
        mob: dragon,
        item: None,
    };
    do_after(
        world,
        actor,
        dragon,
        RIFT_DS,
        0,
        Job::SpaceDragon(DragonJob::Rift),
    );
}

/// The end of the rift's `do_after`: no rift on the tile yet, the ability still there.
fn open_rift(world: &mut World, dragon: Entity) {
    let Some(d) = world.get::<SpaceDragon>(dragon).cloned() else {
        return;
    };
    let Some(pos) = world.get::<TilePos>(dragon).copied() else {
        return;
    };
    let rift_here = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| world.get::<CarpRift>(e).is_some());
    if !d.can_summon || rift_here || d.rift_failure {
        return;
    }
    let Some(rift) = crate::construction::spawn_obj(world, RIFT, pos) else {
        return;
    };
    world.entity_mut(rift).insert((
        CarpRift::new(dragon),
        Integrity {
            current: RIFT_INTEGRITY,
            broken: false,
        },
    ));
    play(world, pos, ROCKET, 100.0);
    if let Some(mut d) = world.get_mut::<SpaceDragon>(dragon) {
        d.rift_timer = None;
        d.rifts.push(rift);
        d.can_summon = false;
    }
    tell_mob(
        world,
        dragon,
        Span::BoldWarning,
        "Разлом был открыт. Любой ценой не допустите его уничтожения!",
    );
    refresh_buttons(world, dragon);
    info!("{dragon} (space dragon) opened a carp rift {rift} at {pos:?}");
}

/// `locate_rift/Activate`: the list of the rift areas.
fn locate_rift(world: &mut World, client: Entity, dragon: Entity) {
    let Some(d) = world.get::<SpaceDragon>(dragon).cloned() else {
        return;
    };
    let Some(pos) = world.get::<TilePos>(dragon).copied() else {
        return;
    };
    if !d.can_locate || d.rift_areas.is_empty() {
        return;
    }
    if !crate::events::controller::station_levels(world).contains(&pos.z) {
        tell(world, client, Span::Warning, "too far offstation!");
        return;
    }
    let items: Vec<String> = d.rift_areas.iter().map(|(_, n)| n.clone()).collect();
    let areas: Vec<Entity> = d.rift_areas.iter().map(|(a, _)| *a).collect();
    let prompt = ss13_tgui::prompt::ask_world(
        world,
        Ask::new(
            client,
            dragon,
            "Locate Rift",
            "Select the area you'd like to be pointed towards.",
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(LocateAsk { dragon, areas });
}

fn locate_answers(world: &mut World, mut answers: Local<MessageCursor<PromptAnswered>>) {
    let pending: Vec<(Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.answer.clone()))
        .collect();
    for (prompt, answer) in pending {
        let Some(ask) = world.get::<LocateAsk>(prompt).cloned() else {
            continue;
        };
        let Some(PromptAnswer::Item(i)) = answer else {
            continue;
        };
        let Some(&area) = ask.areas.get(i) else {
            continue;
        };
        let Some(&pos) = world.get::<TilePos>(ask.dragon) else {
            continue;
        };
        // Still one of its areas now.
        if world
            .get::<SpaceDragon>(ask.dragon)
            .is_none_or(|d| !d.rift_areas.iter().any(|(a, _)| *a == area))
        {
            continue;
        }
        if area_at(world, pos) == Some(area) {
            tell_mob(world, ask.dragon, Span::Warning, "already here!");
            continue;
        }
        let Some(turf) = random_area_turf(world, area) else {
            continue;
        };
        let mut message = if turf.z != pos.z {
            format!("{} you, ", if turf.z > pos.z { "above" } else { "below" })
        } else {
            let dist = (turf.x - pos.x).abs().max((turf.y - pos.y).abs());
            distance_word(dist).to_owned()
        };
        message += crate::pull::dir_between(pos, turf)
            .map(dir_text)
            .unwrap_or("");
        message.push('!');
        tell_mob(world, ask.dragon, Span::Notice, message);
    }
}

/// BYOND `dir2text`.
fn dir_text(dir: ss13_shared::Dir) -> &'static str {
    use ss13_shared::Dir;
    match dir {
        Dir::North => "north",
        Dir::South => "south",
        Dir::East => "east",
        Dir::West => "west",
        Dir::NorthEast => "northeast",
        Dir::NorthWest => "northwest",
        Dir::SouthEast => "southeast",
        Dir::SouthWest => "southwest",
    }
}

/// `pick(get_area_turfs(area))` on the station levels.
fn random_area_turf(world: &mut World, area: Entity) -> Option<TilePos> {
    let tiles: Vec<TilePos> = crate::events::station_tiles(world)
        .into_iter()
        .filter(|(a, _)| *a == area)
        .map(|(_, p)| p)
        .collect();
    if tiles.is_empty() {
        return None;
    }
    let i = with_rng(world, |rng| rng.range(0, tiles.len() as i64 - 1)) as usize;
    Some(tiles[i])
}

// ---------------------------------------------------------------------------------------------
// Rifts and the rift timer

/// Once a second: the rift timer and victory (`rift_checks`), the rifts' charge (`process`),
/// their healing aura, the rage ending, the carp's invigoration ending, the buffet's endlag
/// decay.
fn process(world: &mut World, mut last: Local<f64>) {
    let t = now(world);
    if t - *last < 1.0 {
        return;
    }
    let dt = if *last == 0.0 { 1.0 } else { t - *last };
    *last = t;
    let docked = world
        .get_resource::<crate::shuttle::EmergencyShuttle>()
        .is_some_and(|s| s.state.docked_idle());
    let dragons: Vec<Entity> = world
        .query_filtered::<Entity, With<SpaceDragon>>()
        .iter(world)
        .collect();
    for dragon in dragons {
        if world.get::<Stat>(dragon) == Some(&Stat::Dead) {
            continue;
        }
        let Some(mut d) = world.get::<SpaceDragon>(dragon).cloned() else {
            continue;
        };
        d.endlag = (d.endlag - ENDLAG_DECAY * dt).max(0.0);
        if d.rage_until.is_some_and(|u| t >= u) {
            d.rage_until = None;
        }
        if !d.objective_complete && victorious(d.rifts_charged, docked) {
            world.entity_mut(dragon).insert(d);
            victory(world, dragon);
            continue;
        }
        let tick = tick_rift_timer(&mut d.rift_timer, dt);
        world.entity_mut(dragon).insert(d);
        update_speed(world, dragon);
        match tick {
            TimerTick::Nothing => {}
            TimerTick::Warning => tell_mob(
                world,
                dragon,
                Span::BoldWarning,
                "У вас осталась минута, чтобы открыть разрыв! Скорее!",
            ),
            TimerTick::Expired => {
                tell_mob(
                    world,
                    dragon,
                    Span::BoldWarning,
                    "Вы не успели огкрыть разрыв! Бездна затягивает вас обратно!",
                );
                destroy_rifts(world, dragon);
                send_sound(world, dragon, DEMON_DIES);
                info!("{dragon} (space dragon) ran out of rift time");
                crate::health::gib(world, dragon);
            }
        }
    }
    let rifts: Vec<Entity> = world
        .query_filtered::<Entity, With<CarpRift>>()
        .iter(world)
        .collect();
    for rift in rifts {
        process_rift(world, rift, dt);
    }
    let invigorated: Vec<(Entity, Invigorated)> = world
        .query::<(Entity, &Invigorated)>()
        .iter(world)
        .map(|(e, i)| (e, *i))
        .collect();
    for (carp, inv) in invigorated {
        if t >= inv.until {
            world.entity_mut(carp).remove::<Invigorated>();
            if let Some(mut m) = world.get_mut::<Mover>(carp) {
                m.delay = inv.delay;
            }
        }
    }
}

fn process_rift(world: &mut World, rift: Entity, dt: f64) {
    let Some(mut r) = world.get::<CarpRift>(rift).cloned() else {
        return;
    };
    let Some(&pos) = world.get::<TilePos>(rift) else {
        return;
    };
    // `aura_healing`: the dragon within a tile.
    if let Some(dragon) = r.dragon
        && world.get::<Stat>(dragon) != Some(&Stat::Dead)
        && world
            .get::<TilePos>(dragon)
            .is_some_and(|p| p.z == pos.z && (p.x - pos.x).abs() <= 1 && (p.y - pos.y).abs() <= 1)
        && let Some(mut health) = world.get::<Health>(dragon).cloned()
        && health.brute > 0.0
    {
        health.adjust(DamageType::Brute, -RIFT_HEAL * dt as f32);
        world.entity_mut(dragon).insert(health);
    }
    if r.charge == Charge::Completed {
        let spawn = r.dragon.is_some()
            && with_rng(world, |rng| {
                rng.prob(spt_percent(CHARGED_CARP_CHANCE, dt as f32))
            });
        if spawn {
            crate::mobs::spawn::spawn_mob(world, AI_CARP, pos);
        }
        return;
    }
    let events = charge_rift(&mut r, dt);
    world.entity_mut(rift).insert(r.clone());
    let area = area_at(world, pos);
    let area_name = area
        .map(|a| crate::shuttle::area_name(world, a))
        .unwrap_or_default();
    if events.carp_added {
        crate::obj_damage::set_icon_state(world, rift, "carp_rift_carpspawn");
    }
    if events.charged {
        crate::events::priority_announce(
            world,
            &announce_title(),
            &format!(
                "Пространственный объект достиг пикового заряда энергии в {area_name}, будьте наготове."
            ),
        );
        crate::obj_damage::set_icon_state(world, rift, "carp_rift_charged");
        if let Some(mut obj) = world.get_mut::<DmObj>(rift) {
            obj.resistance_flags |= INDESTRUCTIBLE;
        }
        if let Some(dragon) = r.dragon {
            rift_charged(world, dragon, area);
        }
        return;
    }
    if events.final_warning {
        crate::events::priority_announce_sound(
            world,
            &announce_title(),
            &format!(
                "Разлом создает неествественно большой поток энергии в зоне {area_name}. Остановите его любой ценой!"
            ),
            crate::announce::ANNOUNCER_SPANOMALIES,
        );
    }
}

/// The dragon's part of a rift reaching full charge.
fn rift_charged(world: &mut World, dragon: Entity, area: Option<Entity>) {
    let t = now(world);
    let Some(mut d) = world.get::<SpaceDragon>(dragon).cloned() else {
        return;
    };
    d.rifts_charged += 1;
    if let Some(area) = area {
        d.rift_areas.retain(|(a, _)| *a != area);
    }
    let more = d.rifts_charged != 3 && !d.objective_complete;
    if more {
        d.can_summon = true;
        d.rift_timer = Some(0.0);
        d.rage_until = Some(d.rage_until.unwrap_or(0.0).max(t + EMPOWER_SECS));
    } else {
        d.can_locate = false;
    }
    world.entity_mut(dragon).insert(d);
    if more {
        // `rift_empower`.
        full_heal(world, dragon);
    }
    update_speed(world, dragon);
    refresh_buttons(world, dragon);
}

fn full_heal(world: &mut World, mob: Entity) {
    if let Some(health) = world.get::<Health>(mob).cloned() {
        world.entity_mut(mob).insert(health.healed());
    }
}

fn update_speed(world: &mut World, dragon: Entity) {
    let Some(d) = world.get::<SpaceDragon>(dragon) else {
        return;
    };
    let delay = dragon_delay(d.base_delay, d.rage_until.is_some(), d.rift_failure);
    if let Some(mut m) = world.get_mut::<Mover>(dragon)
        && m.delay != delay
    {
        m.delay = delay;
    }
}

/// `destroy_rifts`: every rift goes, the dragon is slowed for good.
pub fn destroy_rifts(world: &mut World, dragon: Entity) {
    let Some(mut d) = world.get::<SpaceDragon>(dragon).cloned() else {
        return;
    };
    if d.objective_complete {
        return;
    }
    d.rifts_charged = 0;
    d.rift_failure = true;
    d.rift_timer = None;
    let rifts = std::mem::take(&mut d.rifts);
    world.entity_mut(dragon).insert(d);
    send_sound(world, dragon, ROCKET);
    for rift in rifts {
        if let Some(mut r) = world.get_mut::<CarpRift>(rift) {
            r.dragon = None;
        }
        if let Some(&pos) = world.get::<TilePos>(rift) {
            world.despawn(rift);
            crate::construction::tile_changed(world, pos);
        }
    }
    update_speed(world, dragon);
}

/// `victory` + `permanant_empower`.
fn victory(world: &mut World, dragon: Entity) {
    let rifts = {
        let Some(mut d) = world.get_mut::<SpaceDragon>(dragon) else {
            return;
        };
        d.objective_complete = true;
        d.rage_until = Some(f64::INFINITY);
        d.rifts.clone()
    };
    full_heal(world, dragon);
    update_speed(world, dragon);
    let station = crate::events::station_name(world);
    crate::events::priority_announce(
        world,
        &announce_title(),
        &format!(
            "Огромное количество форм жизни надвигается в сторону {station} с невероятной скоростью. Оставшемуся экипажу необходимо эвакуироваться как можно скорее."
        ),
    );
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::to_playing_players(ROAR, 75.0, false),
    );
    for rift in rifts {
        if let Some(mut r) = world.get_mut::<CarpRift>(rift) {
            r.carp_stored = 999_999;
            r.time_charged = RIFT_MAX_CHARGE;
        }
    }
    info!("{dragon} (space dragon) won");
}

/// A rift gone before it charged (`/obj/structure/carp_rift/Destroy`): its dragon fails.
fn rifts_gone(world: &mut World, mut removed: RemovedComponents<CarpRift>) {
    let gone: Vec<Entity> = removed.read().collect();
    if gone.is_empty() {
        return;
    }
    let dragons: Vec<(Entity, Vec<Entity>)> = world
        .query::<(Entity, &SpaceDragon)>()
        .iter(world)
        .map(|(e, d)| (e, d.rifts.clone()))
        .collect();
    for (dragon, rifts) in dragons {
        // `destroy_rifts` empties the list first, so only a rift destroyed by others is here;
        // a charged rift is indestructible.
        if !rifts.iter().any(|r| gone.contains(r)) {
            continue;
        }
        if let Some(mut d) = world.get_mut::<SpaceDragon>(dragon) {
            d.rifts.retain(|r| !gone.contains(r));
        }
        tell_mob(
            world,
            dragon,
            Span::BoldWarning,
            "Разлом был уничтожен! Вы провалили свою задачу, и слабость одолевает вас",
        );
        destroy_rifts(world, dragon);
    }
}

// ---------------------------------------------------------------------------------------------
// Rift carp

/// A ghost's click on a rift (`attack_ghost` -> `summon_carp`): the checks, then the question.
fn ghost_clicks(world: &mut World, mut clicks: Local<MessageCursor<crate::multiz::GhostClick>>) {
    let Some(messages) = world.get_resource::<Messages<crate::multiz::GhostClick>>() else {
        return;
    };
    let pending: Vec<crate::multiz::GhostClick> = clicks.read(messages).copied().collect();
    for click in pending {
        let Some(rift) = world.get::<CarpRift>(click.target).cloned() else {
            continue;
        };
        if world.get::<PlayerMob>(click.ghost).map(|p| p.client) != Some(click.client)
            || rift.carp_stored == 0
        {
            continue;
        }
        let key = world
            .get::<ClientToken>(click.client)
            .map(|t| t.0.clone())
            .unwrap_or_default();
        if rift.keys.contains(&key) && rift.carp_stored == 1 {
            tell(
                world,
                click.client,
                Span::Warning,
                "Вы уже появлялись карпом из этого разлома! Пожалуйста, ожидайте избытка карпов или следующего разлома!",
            );
            continue;
        }
        let prompt = ss13_tgui::prompt::ask_world(
            world,
            Ask::new(
                click.client,
                click.ghost,
                "Разлом карпов",
                "Стать карпом?",
                PromptKind::Alert {
                    buttons: vec!["Да".to_owned(), "Нет".to_owned()],
                    autofocus: true,
                },
            ),
        );
        world.entity_mut(prompt).insert(CarpAsk {
            rift: click.target,
            ghost: click.ghost,
        });
    }
}

/// "Да": checked again (the rift, its carp, the ghost), then the ghost becomes its carp.
fn carp_answers(world: &mut World, mut answers: Local<MessageCursor<PromptAnswered>>) {
    let pending: Vec<(Entity, Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.answer.clone()))
        .collect();
    for (prompt, client, answer) in pending {
        let Some(ask) = world.get::<CarpAsk>(prompt).copied() else {
            continue;
        };
        if answer != Some(PromptAnswer::Button(0))
            || world.get::<PlayerMob>(ask.ghost).map(|p| p.client) != Some(client)
            || world.get::<crate::ghost::Ghost>(ask.ghost).is_none()
        {
            continue;
        }
        summon_carp(world, client, ask.ghost, ask.rift);
    }
}

/// `summon_carp` after the question.
pub fn summon_carp(
    world: &mut World,
    client: Entity,
    ghost: Entity,
    rift: Entity,
) -> Option<Entity> {
    let r = world.get::<CarpRift>(rift).cloned()?;
    let pos = *world.get::<TilePos>(rift)?;
    if r.carp_stored == 0 {
        tell(
            world,
            client,
            Span::Warning,
            "Разлом уже призвал достаточно карпов!",
        );
        return None;
    }
    let key = world
        .get::<ClientToken>(client)
        .map(|t| t.0.clone())
        .unwrap_or_default();
    if r.keys.contains(&key) && r.carp_stored == 1 {
        return None;
    }
    let dragon = r.dragon?;
    let carp = crate::mobs::spawn::spawn_mob(world, RIFT_CARP, pos)?;
    if let Some(faction) = world.get::<crate::mobs::Faction>(dragon).cloned() {
        world.entity_mut(carp).insert(faction);
    }
    if !crate::medical::brainmob::possess(world, client, ghost, carp) {
        world.despawn(carp);
        return None;
    }
    world.try_despawn(ghost).ok();
    world.entity_mut(carp).insert(SpaceCarp { rift });
    let emptied = world.get_mut::<CarpRift>(rift).is_some_and(|mut r| {
        if !r.keys.contains(&key) {
            r.keys.push(key.clone());
        }
        r.carp_stored -= 1;
        r.carp_stored == 0 && r.charge < Charge::Completed
    });
    if emptied {
        crate::obj_damage::set_icon_state(world, rift, "carp_rift");
    }
    if let Some(mut d) = world.get_mut::<SpaceDragon>(dragon) {
        d.carp.push(key);
    }
    tell(
        world,
        client,
        Span::BoldWarning,
        "Вы прибыли, чтобы помочь Космическому дракону защищать разломы. Следуйте поставленной миссии и защитите разлом любой ценой!",
    );
    tell(
        world,
        client,
        Span::Plain,
        "Задача №1: Защищайте разлом призыва карпов.",
    );
    info!("{carp} ({client}) is a rift carp of {dragon}");
    Some(carp)
}

// ---------------------------------------------------------------------------------------------
// The mob's attacks

/// `early_melee_attack`: the dragon's adjacent clicks.
fn dragon_clicks(world: &mut World, mut clicks: Local<MessageCursor<ClickResolved>>) {
    let clicks: Vec<ClickResolved> = clicks
        .read(world.resource::<Messages<ClickResolved>>())
        .copied()
        .collect();
    let t = now(world);
    for click in clicks {
        let Some(d) = world.get::<SpaceDragon>(click.mob).cloned() else {
            continue;
        };
        let (mob, target) = (click.mob, click.target);
        if d.aiming {
            breathe(world, mob, click.target_pos);
            continue;
        }
        if target == mob
            || world.get::<Stat>(mob) != Some(&Stat::Conscious)
            || world.get::<NextMove>(mob).is_some_and(|n| t < n.0)
            || d.gust != Gust::Idle
        {
            continue;
        }
        if world.get::<DoAfter>(mob).is_some() {
            tell(world, click.client, Span::Warning, "заняты!");
            continue;
        }
        let actor = Actor {
            client: click.client,
            mob,
            item: None,
        };
        if world.get::<Health>(target).is_some() {
            if world.get::<Stat>(target) == Some(&Stat::Dead) {
                tell(world, click.client, Span::Notice, "пожираем...");
                do_after(
                    world,
                    actor,
                    target,
                    EAT_DS,
                    0,
                    Job::SpaceDragon(DragonJob::Eat),
                );
            } else {
                crate::mobs::defense::melee_attack(world, mob, target);
            }
            continue;
        }
        if let Some(path) = crate::construction::path_of(world, target) {
            if is_under(&path, "/turf/closed/wall") {
                start_tear(world, actor, target, &path);
                continue;
            }
            if world.get::<crate::doors::Door>(target).is_some() {
                start_pry(world, actor, target);
                continue;
            }
            // `obj_damage` on breakable structures (the rift included).
            if world.get::<Integrity>(target).is_some() && world.get::<CarpRift>(target).is_none() {
                crate::obj_damage::take_damage(world, target, 50.0, DamageType::Brute);
                world.entity_mut(mob).insert(NextMove(t + CLICK_CD_MELEE));
            }
        }
    }
}

/// `RangedAttack` -> the fire breath; `ranged_secondary_attack` -> the wing buffet.
fn ranged_clicks(world: &mut World, mut clicks: Local<MessageCursor<RangedClick>>) {
    let clicks: Vec<RangedClick> = clicks
        .read(world.resource::<Messages<RangedClick>>())
        .copied()
        .collect();
    for click in clicks {
        if world.get::<SpaceDragon>(click.mob).is_none() {
            continue;
        }
        if click.modifiers.right {
            start_gust(world, click.mob);
        } else {
            breathe(world, click.mob, click.target_pos);
        }
    }
}

/// `wall_tearer/rip_and_tear`: one stage of `tear_time / 3`.
fn start_tear(world: &mut World, actor: Actor, wall: Entity, path: &str) {
    let reinforced = is_under(path, "/turf/closed/wall/r_wall");
    let time = if reinforced {
        TEAR_DS * REINFORCED_MULT
    } else {
        TEAR_DS
    } / 3.0;
    let (user, thing) = (name_of(world, actor.mob), name_of(world, wall));
    visible(
        world,
        actor.mob,
        Span::Warning,
        format!(
            "{} начинает выламывать {thing}!",
            ss13_shared::capitalize(&user)
        ),
        None,
    );
    if let Some(&pos) = world.get::<TilePos>(actor.mob) {
        play(world, pos, PRYING, 100.0);
    }
    tell(world, actor.client, Span::Notice, "выламываем...");
    do_after(
        world,
        actor,
        wall,
        time,
        0,
        Job::SpaceDragon(DragonJob::Tear),
    );
}

/// `door_pryer/open_door`: unpowered at once, else 4 s.
fn start_pry(world: &mut World, actor: Actor, door: Entity) {
    let powered = world.get::<crate::power::Unpowered>(door).is_none();
    let Some(d) = world.get::<crate::doors::Door>(door) else {
        return;
    };
    if d.kind != ss13_mapload::DoorKind::Airlock || d.state != crate::doors::DoorState::Closed {
        return;
    }
    if d.locked || d.welded || d.seal.is_some() {
        tell(world, actor.client, Span::Warning, "it's sealed!");
        return;
    }
    let has_power = d.has_power(powered);
    let name = name_of(world, door);
    let user = name_of(world, actor.mob);
    if !has_power {
        visible(
            world,
            actor.mob,
            Span::Warning,
            format!("{user} forces the {name} to open."),
            None,
        );
        force_open(world, door);
        return;
    }
    visible(
        world,
        actor.mob,
        Span::Warning,
        format!("{user} starts forcing the {name} open!"),
        None,
    );
    if let Some(&pos) = world.get::<TilePos>(door) {
        play(world, pos, PRYING, 100.0);
    }
    tell(world, actor.client, Span::Notice, "prying...");
    do_after(
        world,
        actor,
        door,
        PRY_DS,
        0,
        Job::SpaceDragon(DragonJob::Pry),
    );
}

fn force_open(world: &mut World, door: Entity) {
    crate::doors::with_door(world, door, |door, appearance, ctx| {
        if door.locked || door.welded || door.seal.is_some() {
            return ((), false);
        }
        let opened = ctx.open_bypass(door, appearance);
        ((), opened)
    });
}

/// The end of a dragon `do_after`, checked again.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: DragonJob) {
    if world.get::<SpaceDragon>(mob).is_none() || world.get::<Stat>(mob) != Some(&Stat::Conscious) {
        return;
    }
    let target = action.target;
    match job {
        DragonJob::Rift => open_rift(world, mob),
        DragonJob::Eat => {
            if world.get::<Stat>(target) == Some(&Stat::Dead)
                && world.get::<crate::closets::InCloset>(target).is_none()
            {
                eat(world, mob, target);
            }
        }
        DragonJob::Tear => {
            let Some(path) = crate::construction::path_of(world, target) else {
                return;
            };
            if !is_under(&path, "/turf/closed/wall") {
                return;
            }
            crate::animation::do_attack_animation(world, mob, target);
            let fell = crate::weapons::rip_and_tear::torn_wall(world, target, action.target_pos);
            // `UnarmedAttack` again: the next stage.
            if !fell {
                let actor = Actor {
                    client: action.client,
                    mob,
                    item: None,
                };
                start_tear(world, actor, target, &path);
            }
        }
        DragonJob::Pry => {
            let user = name_of(world, mob);
            let name = name_of(world, target);
            if world
                .get::<crate::doors::Door>(target)
                .is_none_or(|d| d.locked)
            {
                return;
            }
            visible(
                world,
                mob,
                Span::Warning,
                format!("{user} forces the {name} to open."),
                None,
            );
            force_open(world, target);
        }
    }
}

/// `eat`: a quarter of the meal's max health healed, the meal inside.
fn eat(world: &mut World, dragon: Entity, food: Entity) {
    let max = world.get::<Health>(food).map_or(0.0, |h| h.max);
    if let Some(mut health) = world.get::<Health>(dragon).cloned() {
        health.adjust(DamageType::Brute, -(max * 0.25).round());
        world.entity_mut(dragon).insert(health);
    }
    if let Some(&pos) = world.get::<TilePos>(dragon) {
        play(world, pos, DEMON_ATTACK, 60.0);
    }
    let (user, meal) = (name_of(world, dragon), name_of(world, food));
    visible(
        world,
        dragon,
        Span::BoldWarning,
        format!(
            "{} поглощает {meal} целиком!",
            ss13_shared::capitalize(&user)
        ),
        None,
    );
    crate::fire::extinguish_mob(world, food);
    crate::closets::put_mob_inside(world, food, dragon);
    info!("{dragon} (space dragon) ate {food}");
}

/// `eaten_stat_changed`: a meal brought back to life is spat out and paralyzed.
fn revived_meals(world: &mut World) {
    let revived: Vec<(Entity, Entity)> = world
        .query::<(Entity, Ref<Stat>, &crate::closets::InCloset)>()
        .iter(world)
        .filter(|(_, s, _)| s.is_changed() && **s != Stat::Dead)
        .map(|(e, _, c)| (e, c.closet))
        .collect();
    for (meal, dragon) in revived {
        if world.get::<SpaceDragon>(dragon).is_none() {
            continue;
        }
        let Some(pos) = crate::explosions::turf_of(world, dragon) else {
            continue;
        };
        crate::construction::spawn_obj(world, VOMIT, pos);
        play(world, pos, SPLAT, 50.0);
        let (user, name) = (name_of(world, dragon), name_of(world, meal));
        visible(
            world,
            dragon,
            Span::Danger,
            format!("{} срыгивает {name}!", ss13_shared::capitalize(&user)),
            None,
        );
        world.entity_mut(meal).insert(pos);
        crate::closets::let_out(world, meal);
        crate::status_effects::apply(world, meal, Incap::Paralyze, 5.0);
    }
}

/// `fire_breath/carp/Activate` toward `toward`.
fn breathe(world: &mut World, dragon: Entity, toward: TilePos) {
    let t = now(world);
    let Some(d) = world.get::<SpaceDragon>(dragon).cloned() else {
        return;
    };
    if d.aiming
        && let Some(mut d) = world.get_mut::<SpaceDragon>(dragon)
    {
        d.aiming = false;
    }
    let Some(&from) = world.get::<TilePos>(dragon) else {
        return;
    };
    if world.get::<Stat>(dragon) != Some(&Stat::Conscious)
        || t < d.breath_ready
        || d.gust != Gust::Idle
        || world.get::<FireLine>(dragon).is_some()
        || toward == from
    {
        refresh_buttons(world, dragon);
        return;
    }
    play(world, from, FIREBALL, 200.0);
    let index = world.resource::<SpatialIndex>();
    let end = crate::mobs::megafauna::drake::ranged_target_turf(
        |p| index.in_bounds(p),
        from,
        toward,
        BREATH_RANGE,
        0.0,
    );
    let mut tiles: Vec<TilePos> = crate::mobs::megafauna::drake::get_line(from, end)
        .into_iter()
        .filter(|&p| p != from)
        .collect();
    tiles.reverse();
    world.entity_mut(dragon).insert(FireLine {
        next: t,
        tiles,
        hit: vec![dragon],
    });
    if let Some(mut d) = world.get_mut::<SpaceDragon>(dragon) {
        d.breath_ready = t + BREATH_COOLDOWN;
    }
    world
        .entity_mut(dragon)
        .insert(NextMove(t + BREATH_COOLDOWN.min(CLICK_CD_MELEE)));
    refresh_buttons(world, dragon);
}

/// `is_blocked_turf(exclude_mobs = TRUE)`.
fn breath_blocked(world: &World, pos: TilePos) -> bool {
    let index = world.resource::<SpatialIndex>();
    if !index.in_bounds(pos) {
        return true;
    }
    let closed = crate::construction::turf_at(world, pos)
        .and_then(|t| crate::construction::path_of(world, t))
        .is_some_and(|p| is_under(&p, "/turf/closed"));
    closed
        || index
            .dense_on_tile(pos)
            .iter()
            .any(|&e| world.get::<Health>(e).is_none())
}

/// `progressive_fire_line`: a tile every 0.15 s until a blocked one.
fn fire_lines(world: &mut World) {
    let t = now(world);
    let due: Vec<(Entity, FireLine)> = world
        .query::<(Entity, &FireLine)>()
        .iter(world)
        .filter(|(_, l)| t >= l.next)
        .map(|(e, l)| (e, l.clone()))
        .collect();
    for (dragon, mut line) in due {
        let pos = line.tiles.pop();
        let stop = world.get::<Stat>(dragon) == Some(&Stat::Dead)
            || pos.is_none_or(|p| breath_blocked(world, p));
        if stop {
            world.entity_mut(dragon).remove::<FireLine>();
            continue;
        }
        let pos = pos.unwrap_or_default();
        burn_turf(world, dragon, pos, &mut line.hit);
        if line.tiles.is_empty() {
            world.entity_mut(dragon).remove::<FireLine>();
            continue;
        }
        line.next = t + BREATH_DELAY;
        world.entity_mut(dragon).insert(line);
    }
}

/// `burn_turf` + `on_burn_mob`: allies are invigorated, the rest burn.
fn burn_turf(world: &mut World, dragon: Entity, pos: TilePos, hit: &mut Vec<Entity>) {
    if let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() {
        atmos.hotspot_expose(pos, BREATH_TEMP, BREATH_VOLUME, true);
    }
    let source = name_of(world, dragon);
    let victims: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| world.get::<Health>(e).is_some() && world.get::<Stat>(e).is_some())
        .collect();
    for victim in victims {
        if hit.contains(&victim) {
            continue;
        }
        hit.push(victim);
        if crate::mobs::same_faction(world, dragon, victim) {
            tell_mob(
                world,
                victim,
                Span::Notice,
                format!("Огненное дыхание {source} наполняет вас энергией!"),
            );
            invigorate(world, victim);
            continue;
        }
        tell_mob(
            world,
            victim,
            Span::Danger,
            format!("Вас подожгло огненное дыхание {source}!"),
        );
        crate::fire::fire_act(world, victim, BREATH_TEMP);
        crate::mobs::lavaland::hurt(world, victim, DamageType::Burn, BREATH_DAMAGE, None);
    }
}

/// `carp_invigoration`: `dragon_rage` for 8 s.
fn invigorate(world: &mut World, mob: Entity) {
    let until = now(world) + INVIGORATION_SECS;
    if world.get::<SpaceDragon>(mob).is_some() {
        return;
    }
    let delay = match world.get::<Invigorated>(mob) {
        Some(i) => i.delay,
        None => world.get::<Mover>(mob).map_or(0.0, |m| m.delay),
    };
    world.entity_mut(mob).insert(Invigorated { until, delay });
    if let Some(mut m) = world.get_mut::<Mover>(mob) {
        m.delay = (delay - RAGE_SPEEDUP).max(0.0);
    }
}

/// `wing_buffet/Activate`: rises for 1.2 s, held in place.
fn start_gust(world: &mut World, dragon: Entity) {
    let t = now(world);
    let Some(d) = world.get::<SpaceDragon>(dragon).cloned() else {
        return;
    };
    if d.gust != Gust::Idle
        || t < d.gust_ready
        || world.get::<Stat>(dragon) != Some(&Stat::Conscious)
    {
        return;
    }
    if let Some(mut d) = world.get_mut::<SpaceDragon>(dragon) {
        d.gust = Gust::Rising(t + BUFFET_WINDUP);
        d.gust_ready = t + BUFFET_COOLDOWN;
    }
    crate::status_effects::apply(world, dragon, Incap::Immobilize, BUFFET_WINDUP as f32);
    refresh_buttons(world, dragon);
}

/// `ground_pound` and `complete_ability`.
fn gusts(world: &mut World) {
    let t = now(world);
    let due: Vec<(Entity, SpaceDragon)> = world
        .query::<(Entity, &SpaceDragon)>()
        .iter(world)
        .filter(|(_, d)| match d.gust {
            Gust::Idle => false,
            Gust::Rising(at) | Gust::Tired(at) => t >= at,
        })
        .map(|(e, d)| (e, d.clone()))
        .collect();
    for (dragon, d) in due {
        match d.gust {
            Gust::Rising(_) => {
                let (stun, endlag) = buffet_endlag(d.endlag, d.rift_failure);
                if let Some(mut d) = world.get_mut::<SpaceDragon>(dragon) {
                    d.gust = Gust::Tired(t + stun);
                    d.endlag = endlag;
                }
                crate::status_effects::apply(world, dragon, Incap::Immobilize, stun as f32);
                ground_pound(world, dragon);
            }
            _ => {
                if let Some(mut d) = world.get_mut::<SpaceDragon>(dragon) {
                    d.gust = Gust::Idle;
                }
            }
        }
    }
}

/// Everyone not of its faction within 4 tiles is thrown back and paralyzed.
fn ground_pound(world: &mut World, dragon: Entity) {
    let Some(&pos) = world.get::<TilePos>(dragon) else {
        return;
    };
    if world.get::<Stat>(dragon) == Some(&Stat::Dead) {
        return;
    }
    play(world, pos, GRAVHIT, 100.0);
    let around: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(pos, GUST_DISTANCE)
        .filter(|&e| e != dragon && world.get::<Stat>(e).is_some())
        .collect();
    for victim in around {
        if crate::mobs::same_faction(world, dragon, victim) {
            continue;
        }
        let Some(&at) = world.get::<TilePos>(victim) else {
            continue;
        };
        if at.z != pos.z {
            continue;
        }
        let name = name_of(world, victim);
        visible(
            world,
            dragon,
            Span::BoldWarning,
            format!("{name} is knocked back by the gust!"),
            None,
        );
        tell_mob(
            world,
            victim,
            Span::Danger,
            "You're knocked back by the gust!",
        );
        let (dx, dy) = ((at.x - pos.x).signum(), (at.y - pos.y).signum());
        let to = TilePos::new(at.x + dx * GUST_THROW, at.y + dy * GUST_THROW, at.z);
        if (dx, dy) != (0, 0) {
            crate::throw::throw_at(world, victim, to, GUST_THROW as u32, 1.0);
        }
        crate::status_effects::apply(world, victim, Incap::Paralyze, GUST_PARALYZE);
    }
}

/// `on_exploded`: a devastating blast takes 40-75% of the max health instead of gibbing.
pub fn on_exploded(world: &mut World, mob: Entity) -> bool {
    let is_dragon = world
        .get::<crate::mobs::BasicMob>(mob)
        .is_some_and(|b| is_under(&b.def.path, DRAGON_MOB));
    if !is_dragon {
        return false;
    }
    // DM `rand(0.4, 0.75)`: a real number in between.
    let unit = with_rng(world, |rng| {
        (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }) as f32;
    let coefficient = DEVASTATION_MIN + (DEVASTATION_MAX - DEVASTATION_MIN) * unit;
    if let Some(mut health) = world.get::<Health>(mob).cloned() {
        let max = world
            .get::<crate::mobs::BasicMob>(mob)
            .map_or(health.max, |b| b.def.max_health);
        health.adjust(DamageType::Brute, max * coefficient);
        world.entity_mut(mob).insert(health);
    }
    true
}

// ---------------------------------------------------------------------------------------------
// Death, info and the round end

/// `COMSIG_LIVING_DEATH` -> `destroy_rifts`; what it ate comes out (`content_barfer`).
fn on_death(world: &mut World) {
    let died: Vec<Entity> = world
        .query_filtered::<(Entity, Ref<Stat>), With<SpaceDragon>>()
        .iter(world)
        .filter(|(_, s)| s.is_changed() && **s == Stat::Dead)
        .map(|(e, _)| e)
        .collect();
    for dragon in died {
        destroy_rifts(world, dragon);
        if let Some(mut d) = world.get_mut::<SpaceDragon>(dragon) {
            d.aiming = false;
        }
        world.entity_mut(dragon).remove::<FireLine>();
        super::morph::barf(world, dragon);
        refresh_buttons(world, dragon);
    }
}

fn open_info(world: &mut World, mut acts: Local<MessageCursor<UiActed<AntagHudUi>>>) {
    let pending: Vec<(Entity, Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<AntagHudUi>>>())
        .map(|a| (a.client, a.actor, a.source))
        .collect();
    for (client, actor, source) in pending {
        if world.get::<SpaceDragon>(source).is_some() {
            world.write_message(OpenUi::new::<DragonInfoWindow>(
                client, actor, source, ANTAG_NAME,
            ));
        }
    }
}

/// `/datum/antagonist/space_dragon/roundend_report`.
pub fn roundend_report(world: &mut World, _end: RoundEnd) -> Vec<ChatMessage> {
    let mut all: Vec<(Entity, SpaceDragon)> = world
        .query::<(Entity, &SpaceDragon)>()
        .iter(world)
        .map(|(e, d)| (e, d.clone()))
        .collect();
    if all.is_empty() {
        return Vec::new();
    }
    all.sort_by_key(|(e, _)| *e);
    let line = |span: Span, text: String| ChatMessage {
        channel: ChatChannel::System,
        segments: vec![ChatSegment::new(span, text)],
    };
    let mut out = vec![line(Span::Bold, format!("{ROUNDEND_CATEGORY} были:"))];
    for (mob, d) in all {
        if d.objective_complete {
            out.push(line(
                Span::RedText,
                format!(
                    "{ANTAG_NAME} - успех! Космические карпы вернули контроль над территорией расположения станции!"
                ),
            ));
        }
        out.push(super::traitor::print_player(world, mob, &d.key));
        let (span, verdict) = if d.objective_complete {
            (Span::GreenText, "Успех!")
        } else {
            (Span::RedText, "Провал.")
        };
        out.push(line(
            span,
            format!("Задача №1: {} {verdict}", objective_text(&d)),
        ));
        out.push(if d.objective_complete {
            line(Span::GreenText, format!("{ANTAG_NAME} преуспел!"))
        } else {
            line(Span::RedText, format!("{ANTAG_NAME} провалился!"))
        });
        if !d.carp.is_empty() {
            out.push(line(Span::Bold, format!("Помощники {ANTAG_NAME}:")));
            let mut counted: Vec<(String, u32)> = Vec::new();
            for key in &d.carp {
                match counted.iter_mut().find(|(k, _)| k == key) {
                    Some((_, n)) => *n += 1,
                    None => counted.push((key.clone(), 1)),
                }
            }
            for (key, n) in counted {
                out.push(line(
                    Span::Plain,
                    format!("{key}, был космическим карпом {n} раз."),
                ));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rift() -> CarpRift {
        CarpRift::new(Entity::PLACEHOLDER)
    }

    #[test]
    fn a_rift_charges_carp_warns_at_half_and_completes() {
        let mut r = rift();
        let mut carp = 0;
        let mut warned = 0;
        let mut charged = 0;
        for _ in 0..400 {
            let e = charge_rift(&mut r, 1.0);
            carp += u32::from(e.carp_added);
            warned += u32::from(e.final_warning);
            charged += u32::from(e.charged);
        }
        // 45, 90, ... 270 (300 completes first): 6 carp on top of the first one.
        assert_eq!(carp, 6);
        assert_eq!(r.carp_stored, 7);
        assert_eq!((warned, charged), (1, 1));
        assert_eq!(r.charge, Charge::Completed);
        assert_eq!(r.time_charged, RIFT_MAX_CHARGE);
        // Charged: nothing more.
        assert_eq!(charge_rift(&mut r, 1.0), RiftEvents::default());
    }

    #[test]
    fn the_rift_timer_warns_a_minute_before_and_expires() {
        let mut timer = Some(0.0);
        let mut ticks = Vec::new();
        for _ in 0..300 {
            ticks.push(tick_rift_timer(&mut timer, 1.0));
        }
        assert_eq!(ticks[239], TimerTick::Warning);
        assert_eq!(
            ticks.iter().filter(|t| **t == TimerTick::Warning).count(),
            1
        );
        assert_eq!(ticks[299], TimerTick::Expired);
        assert_eq!(ticks[298], TimerTick::Nothing);
        // A rift is open: the timer stands.
        let mut none = None;
        assert_eq!(tick_rift_timer(&mut none, 1000.0), TimerTick::Nothing);
    }

    #[test]
    fn victory_speed_and_endlag() {
        assert!(victorious(3, false));
        assert!(!victorious(2, false));
        assert!(victorious(1, true));
        assert!(!victorious(0, true));
        assert_eq!(dragon_delay(0.0, true, false), 0.0);
        assert!((dragon_delay(0.1, true, false) - 0.05).abs() < 1e-6);
        assert!((dragon_delay(0.0, false, true) - 0.5).abs() < 1e-6);
        let (stun, next) = buffet_endlag(0.0, false);
        assert!((stun - 0.4).abs() < 1e-9 && (next - 0.3).abs() < 1e-9);
        let (stun, next) = buffet_endlag(0.3, true);
        assert!((stun - 1.9).abs() < 1e-9 && (next - 1.8).abs() < 1e-9);
    }

    #[test]
    fn colours_names_and_directions() {
        assert_eq!(parse_hex("#ff8000"), Some([255, 128, 0]));
        assert_eq!(parse_hex("ff8000"), None);
        assert_eq!(parse_hex("#ff80"), None);
        assert!(colour_bright_enough([255, 0, 0]));
        assert!(!colour_bright_enough([40, 40, 40]));
        assert!(colour_bright_enough([51, 0, 0]));
        assert_eq!(distance_word(15), "very near, ");
        assert_eq!(distance_word(16), "near, ");
        assert_eq!(distance_word(127), "far, ");
        assert_eq!(distance_word(128), "very far, ");
    }

    fn dragon_state(rift: Entity) -> SpaceDragon {
        SpaceDragon {
            key: "k".to_owned(),
            rift_areas: Vec::new(),
            rift_timer: None,
            rifts: vec![rift],
            rifts_charged: 0,
            objective_complete: false,
            rift_failure: false,
            can_summon: false,
            can_locate: true,
            carp: Vec::new(),
            rage_until: None,
            base_delay: 0.1,
            breath_ready: 0.0,
            aiming: false,
            gust: Gust::Idle,
            gust_ready: 0.0,
            endlag: 0.0,
        }
    }

    /// A charged rift heals and enrages the dragon and gives the button back; a lost rift takes
    /// the others and slows it for good.
    #[test]
    fn charging_empowers_and_a_lost_rift_fails_everything() {
        let mut world = World::new();
        world.init_resource::<Time>();
        let pos = TilePos::new(2, 2, 1);
        let rift = world.spawn((pos, rift())).id();
        let mut health = Health::basic(400.0, [1.0; 5]);
        health.adjust(DamageType::Brute, 100.0);
        let dragon = world
            .spawn((dragon_state(rift), health, Mover::new(0.1), Stat::Conscious))
            .id();
        world.get_mut::<CarpRift>(rift).unwrap().dragon = Some(dragon);
        rift_charged(&mut world, dragon, None);
        let d = world.get::<SpaceDragon>(dragon).unwrap().clone();
        assert_eq!(d.rifts_charged, 1);
        assert!(d.can_summon);
        assert_eq!(d.rift_timer, Some(0.0));
        assert!(d.rage_until.is_some());
        assert_eq!(world.get::<Health>(dragon).unwrap().brute, 0.0);
        assert!((world.get::<Mover>(dragon).unwrap().delay - 0.05).abs() < 1e-6);
        destroy_rifts(&mut world, dragon);
        let d = world.get::<SpaceDragon>(dragon).unwrap().clone();
        assert_eq!(d.rifts_charged, 0);
        assert!(d.rift_failure && d.rifts.is_empty() && d.rift_timer.is_none());
        assert!(world.get_entity(rift).is_err());
        assert!((world.get::<Mover>(dragon).unwrap().delay - 0.55).abs() < 1e-6);
        // The third charged rift: no new button, the locator goes.
        let mut d = dragon_state(rift);
        d.rifts_charged = 2;
        world.entity_mut(dragon).insert(d);
        rift_charged(&mut world, dragon, None);
        let d = world.get::<SpaceDragon>(dragon).unwrap();
        assert_eq!(d.rifts_charged, 3);
        assert!(!d.can_summon && !d.can_locate);
    }

    #[test]
    fn english_list_is_russian() {
        assert_eq!(english_list(&[]), "ничего");
        assert_eq!(english_list(&["a"]), "a");
        assert_eq!(english_list(&["a", "b", "c"]), "a, b и c");
    }

    #[test]
    fn the_ruleset_takes_the_dragon_role() {
        let r = super::super::midround::ruleset(super::super::midround::SPACE_DRAGON).unwrap();
        assert_eq!(r.role, Some(ROLE_SPACE_DRAGON));
        assert_eq!(r.weight, [0, 3, 5, 5]);
        assert_eq!(r.min_pop, 30);
    }
}
