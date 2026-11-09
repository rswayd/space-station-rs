//! Xenomorphs after tg `/mob/living/carbon/alien` (`alien.dm`, `organs.dm`, `adult/*`, `larva/*`,
//! `special/*`), their antagonist (`/datum/antagonist/xeno`, `xeno.dm`) and the "Xenomorph" heavy
//! midround (`/datum/dynamic_ruleset/midround/from_ghosts/xenomorph`).
//!
//! An alien is read from the DM dump as a basic mob (`ss13_mapload::mobs`, one health pool, tox
//! and stamina immune); [`attach`] gives it [`Xeno`]: caste, plasma vessel, larva growth.
//! - **Life** ([`life`], every `SSmobs` fire): plasma regeneration and, on weeds, healing
//!   (`plasmavessel/on_life`); a larva grows a point per plasma tick plus 0.5 a second
//!   (`larva/Life`, `adjustPlasma`); what it ate is digested (`stomach/on_life`).
//! - **Hive** ([`hive_chat`], `:a`/`:ф`): [`AlienHivemind`] on every alien; ghosts read it. The
//!   queen's death ([`queen_death`]) stuns the others and cuts them from the hive for 4 minutes,
//!   during which no royal can evolve.
//! - **Evolution** ([`evolve`], `alien_evolve`): the new caste keeps the identifier and the mind;
//!   the larva chooses between hunter, sentinel and drone, the drone becomes a praetorian if no
//!   royal lives, the praetorian a queen if no queen lives, the queen promotes an adult child.
//! - [`abilities`]: the action buttons (`alien_powers.dm`). [`reproduction`]: facehuggers,
//!   embryos, weeds, eggs, nests (`facehugger.dm`, `alien_embryo.dm`, `aliens.dm`).
//! - **Selection** ([`can_spawn`], [`spawn_xenos`], from `super::midround`): the polled ghosts
//!   become larvae in random vents; the crew is told after 375-600 s.
// shortcut: an alien is a basic mob: no crit (dead at 0 health, not -100), hands (only facehuggers
// are carried, `Xeno::carried`), cuffs, pain or organs to cut out; no queen-finder HUD; no plasma
// from breathing; no hunter pounce-sprite or alien leap offsets; `captive` xenos and the egg
// delivery of the communications console are not ported; xenocommon is not given to the mob (the
// hivemind is the common way to talk); add them with carbon-based aliens.

pub mod abilities;
pub mod reproduction;

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_chat::server::{Listeners, SaySystems, Spoke};
use ss13_chat::speech::{AlienHivemind, RadioTarget};
use ss13_mapload::fidelity::is_under;
use ss13_mapload::mobs::MobDef;
use ss13_mapload::spawner::Rng;
use ss13_net::chat::server::SendChat;
use ss13_net::server::{ClientToken, SeeGroups};
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_tgui::UiInterface;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiSystems};
use ss13_world::Mover;

use super::dynamic::DynamicRng;
use super::objectives::RoundEnd;
use super::{AntagHudData, AntagHudUi, BrotherInfo, ObjectiveInfo, ROLE_ALIEN};
use crate::breath::SSMOBS_WAIT_SECS;
use crate::click::ResolveClicks;
use crate::closets::InCloset;
use crate::combat::CombatRng;
use crate::ghost::Ghost;
use crate::health::{DamageType, Health, HealthSystems, Stat};
use crate::medical::{name_of, visible};
use crate::server::{PlayerMob, Players};
use crate::status_effects::Incap;
use crate::tools::{now, tell, tell_mob};

pub const LARVA: &str = "/mob/living/carbon/alien/larva";
pub const HUNTER: &str = "/mob/living/carbon/alien/adult/hunter";
pub const SENTINEL: &str = "/mob/living/carbon/alien/adult/sentinel";
pub const DRONE: &str = "/mob/living/carbon/alien/adult/drone";
pub const PRAETORIAN: &str = "/mob/living/carbon/alien/adult/royal/praetorian";
pub const QUEEN: &str = "/mob/living/carbon/alien/adult/royal/queen";
/// `name`.
pub const ANTAG_NAME: &str = "Ксеноморф";
const ROUNDEND_CATEGORY: &str = "Чужие";
/// `XENOMORPH_MAX_GROWTH`.
pub const MAX_GROWTH: f32 = 100.0;
/// `QUEEN_DEATH_DEBUFF_DURATION`, seconds.
pub const QUEEN_DEATH_SECS: f64 = 240.0;
/// `plasmavessel` `heal_rate`.
const HEAL_RATE: f32 = 2.5;
/// `delta_time_capped`.
const LIFE_DT_CAP: f64 = 8.0;
/// `stomach/on_life`: every third `SSmobs` fire.
const DIGEST_FIRES: u32 = 3;
/// `stomach_acid_power` 75 and volume 10 on a carbon (`acid_act`, `crate::bots::cleanbot`).
const DIGEST_BRUTE: f32 = 3.75;
const DIGEST_BURN: f32 = 7.5;
/// `announce_xenos`: `rand(375, 600) SECONDS`.
const ANNOUNCE_SECS: (i64, i64) = (375, 600);
/// `SeeGroups` bit of who sees the infection images (`AddInfectionImages`).
pub const XENO_SIGHT: u32 = 1 << 7;

/// tg's alien castes: `caste`, organs and speed by [`Caste`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Caste {
    Larva,
    Hunter,
    Sentinel,
    Drone,
    Praetorian,
    Queen,
}

/// A `/obj/item/organ/alien/plasmavessel` type's numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vessel {
    pub stored: f32,
    pub max: f32,
    pub rate: f32,
}

impl Caste {
    pub fn of(path: &str) -> Option<Self> {
        Some(if is_under(path, LARVA) {
            Self::Larva
        } else if is_under(path, HUNTER) {
            Self::Hunter
        } else if is_under(path, SENTINEL) {
            Self::Sentinel
        } else if is_under(path, DRONE) {
            Self::Drone
        } else if is_under(path, PRAETORIAN) {
            Self::Praetorian
        } else if is_under(path, QUEEN) {
            Self::Queen
        } else {
            return None;
        })
    }

    pub fn path(self) -> &'static str {
        match self {
            Self::Larva => LARVA,
            Self::Hunter => HUNTER,
            Self::Sentinel => SENTINEL,
            Self::Drone => DRONE,
            Self::Praetorian => PRAETORIAN,
            Self::Queen => QUEEN,
        }
    }

    /// `alien_speed`, deciseconds.
    pub fn speed(self) -> f32 {
        match self {
            Self::Larva | Self::Drone => 0.0,
            Self::Hunter => -0.3,
            Self::Sentinel => 0.2,
            Self::Praetorian => 0.5,
            Self::Queen => 2.0,
        }
    }

    pub fn adult(self) -> bool {
        self != Self::Larva
    }

    /// `isalienroyal`.
    pub fn royal(self) -> bool {
        matches!(self, Self::Praetorian | Self::Queen)
    }

    /// The caste's `plasmavessel` subtype.
    pub fn vessel(self) -> Vessel {
        let (stored, max, rate) = match self {
            Self::Larva => (100.0, 100.0, 2.5),
            Self::Hunter => (100.0, 150.0, 2.5),
            Self::Sentinel => (100.0, 250.0, 5.0),
            Self::Drone | Self::Praetorian => (200.0, 500.0, 7.5),
            Self::Queen => (200.0, 500.0, 10.0),
        };
        Vessel { stored, max, rate }
    }
}

/// What a click-activated action is waiting for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aim {
    Acid,
    Neurotoxin,
    Leap,
    Promote,
}

/// On an alien: its caste and what its organs and `/mob` vars hold.
#[derive(Component, Clone, Debug)]
pub struct Xeno {
    pub caste: Caste,
    pub plasma: f32,
    pub max_plasma: f32,
    pub plasma_rate: f32,
    /// A larva's `amount_grown`.
    pub amount_grown: f32,
    /// `identifier`, kept through evolution.
    pub identifier: u32,
    /// The player's key, for the round-end report; empty until a player takes it.
    pub key: String,
    /// `recent_queen_death` ends at this game time; 0: not under it.
    pub cut_off_until: f64,
    /// The click-activated action waiting (`click_intercept`).
    pub aim: Option<Aim>,
    /// `leap_on_click`.
    pub leap_on_click: bool,
    /// The target and landing of a leap in the air (`leaping`).
    pub leaping: Option<(Option<Entity>, TilePos)>,
    /// `pounce_cooldown`.
    pub leap_ready_at: f64,
    /// `tail sweep` `cooldown_time`.
    pub tail_ready_at: f64,
    /// The layer before hiding; a larva hiding under things.
    pub hidden_layer: Option<f32>,
    /// `TRAIT_SNEAK` with the look alpha before it.
    pub sneak_alpha: Option<u8>,
    /// Facehuggers in its claws (`xeno_allowed_items`), not on any tile.
    pub carried: Vec<Entity>,
    /// `SSmobs` fires, for the stomach's `times_fired % 3`.
    fires: u32,
}

impl Xeno {
    fn new(caste: Caste, identifier: u32) -> Self {
        let v = caste.vessel();
        Self {
            caste,
            plasma: v.stored,
            max_plasma: v.max,
            plasma_rate: v.rate,
            amount_grown: 0.0,
            identifier,
            key: String::new(),
            cut_off_until: 0.0,
            aim: None,
            leap_on_click: false,
            leaping: None,
            leap_ready_at: 0.0,
            tail_ready_at: 0.0,
            hidden_layer: None,
            sneak_alpha: None,
            carried: Vec::new(),
            fires: 0,
        }
    }

    pub fn hive_cut_off(&self, now: f64) -> bool {
        now < self.cut_off_until
    }
}

/// The antagonist panel (tg's default `AntagInfoGeneric`).
pub struct XenoInfoWindow;

impl UiInterface for XenoInfoWindow {
    const NAME: &'static str = crate::antag::cult::AntagInfoGenericUi::NAME;
    type Data = ();
    type StaticData = BrotherInfo;
}

impl ServerUi for XenoInfoWindow {
    type Source = &'static Xeno;
    type UserParam = ();
    type Act = ();

    fn static_data(_: &Xeno) -> BrotherInfo {
        BrotherInfo {
            antag_name: ANTAG_NAME.to_owned(),
            objectives: vec![ObjectiveInfo {
                count: 1,
                explanation: "Выживите и приумножьте улей.".to_owned(),
                complete: false,
            }],
        }
    }

    fn data(_: &Xeno, _: Entity, _: &()) {}
}

/// `announce_xenos` waiting: at what game time.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub(super) struct PendingAnnounce(pub f64);

pub struct XenoPlugin;

impl Plugin for XenoPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<XenoInfoWindow>()
            .init_resource::<PendingAnnounce>()
            .add_systems(
                Update,
                (
                    open_info,
                    abilities::trigger_buttons,
                    abilities::answers.after(ss13_tgui::prompt::PromptSystems::Answer),
                )
                    .in_set(UiSystems::Handle),
            )
            .add_systems(
                Update,
                (
                    abilities::aimed_clicks,
                    xeno_clicks,
                    life,
                    leap_landings,
                    on_death,
                )
                    .chain()
                    .after(ResolveClicks)
                    .before(HealthSystems),
            )
            .add_systems(Update, (hive_chat.after(SaySystems), announce));
        reproduction::build(app);
    }
}

fn with_rng<R>(world: &mut World, f: impl FnOnce(&mut Rng) -> R) -> R {
    if !world.contains_resource::<DynamicRng>() {
        world.insert_resource(DynamicRng(Rng::new(0)));
    }
    f(&mut world.resource_mut::<DynamicRng>().0)
}

/// tg `/mob/living/carbon/alien/Initialize` over a basic mob: [`Xeno`], the crawl, the look.
pub(crate) fn attach(world: &mut World, entity: Entity, def: &MobDef) {
    let Some(caste) = Caste::of(&def.path) else {
        return;
    };
    let identifier = with_rng(world, |rng| rng.range(1, 999)) as u32;
    let base = world
        .get::<Name>(entity)
        .map_or_else(|| def.name.clone(), |n| n.as_str().to_owned());
    let mut name = format!("{base} ({identifier})");
    // `queen/set_name`: another queen alive makes this one a princess.
    if caste == Caste::Queen && exists(world, |c| c == Caste::Queen, Some(entity)) {
        name = format!("принцесса Чужих ({identifier})");
    }
    let mut e = world.entity_mut(entity);
    e.insert((
        Xeno::new(caste, identifier),
        Name::new(name),
        crate::mobs::Faction(vec![ROLE_ALIEN.to_owned()]),
        Mover::new(crate::RUN_DELAY_SECS + caste.speed() / 10.0),
        crate::combat::CombatMode(false),
    ));
    if !caste.royal() {
        e.insert(crate::ventcrawl::Ventcrawler { nude: false });
    }
}

/// `get_alien_type`: a living, played alien of a caste `want` accepts, not `ignored`.
pub fn exists(world: &mut World, want: impl Fn(Caste) -> bool, ignored: Option<Entity>) -> bool {
    let mut q = world.query_filtered::<(Entity, &Xeno, &Stat), With<PlayerMob>>();
    q.iter(world)
        .any(|(e, x, s)| Some(e) != ignored && want(x.caste) && *s != Stat::Dead)
}

fn is_xeno(world: &World, e: Entity) -> bool {
    world.get::<Xeno>(e).is_some()
}

// ---------------------------------------------------------------------------------------------
// Selection and the antagonist

/// `find_vent_spawns`: unwelded vents on station levels.
// shortcut: no minimum pipenet size or "no pipes under the floor" filter.
pub(super) fn vents(world: &mut World) -> Vec<Entity> {
    let levels = crate::events::controller::station_levels(world);
    let mut q = world.query::<(Entity, &crate::atmos_machinery::VentPump, &TilePos)>();
    let mut out: Vec<Entity> = q
        .iter(world)
        .filter(|(_, v, p)| !v.welded && levels.contains(&p.z))
        .map(|(e, ..)| e)
        .collect();
    out.sort();
    out
}

/// `can_be_selected`: a vent to crawl out of.
pub fn can_spawn(world: &mut World) -> bool {
    !vents(world).is_empty()
}

/// `create_ruleset_body` + `assign_role` for each `(client, ghost)`: a larva in a vent and the
/// announcement after 375-600 s. Each vent is used once while there are two or more.
pub fn spawn_xenos(world: &mut World, ghosts: &[(Entity, Entity)]) -> Vec<Entity> {
    let mut pool = vents(world);
    let mut out = Vec::new();
    for &(client, ghost) in ghosts {
        if pool.is_empty() {
            break;
        }
        let vent = if pool.len() >= 2 {
            let i = with_rng(world, |rng| rng.range(0, pool.len() as i64 - 1)) as usize;
            pool.swap_remove(i)
        } else {
            pool[0]
        };
        let Some(&pos) = world.get::<TilePos>(vent) else {
            continue;
        };
        let Some(larva) = crate::mobs::spawn::spawn_mob(world, LARVA, pos) else {
            break;
        };
        if !crate::medical::brainmob::possess(world, client, ghost, larva) {
            world.despawn(larva);
            continue;
        }
        world.try_despawn(ghost).ok();
        crate::ventcrawl::move_into_vent(world, larva, vent);
        make_xeno(world, larva, true);
        info!("{larva} ({client}) is a xenomorph larva in {vent}");
        out.push(larva);
    }
    if !out.is_empty() {
        let secs = with_rng(world, |rng| rng.range(ANNOUNCE_SECS.0, ANNOUNCE_SECS.1)) as f64;
        let at = now(world) + secs;
        world.resource_mut::<PendingAnnounce>().0 = at;
    }
    out
}

fn announce(world: &mut World) {
    let due = world.resource::<PendingAnnounce>().0;
    if due <= 0.0 || now(world) < due {
        return;
    }
    world.resource_mut::<PendingAnnounce>().0 = 0.0;
    let station = crate::round::station_name(world);
    crate::announce::Priority::new(
        format!(
            "На борту {station} обнаружены неопознанные формы жизни. Перекройте все внешние \
             шлюзы, включая трубы и вентиляции."
        ),
        "Неопознанные формы жизни",
    )
    .sound(crate::announce::ANNOUNCER_ALIENS)
    .send(world);
}

/// `mind_initialize` -> `add_antag_datum(/datum/antagonist/xeno)` and the job: no station job,
/// the hive, the buttons, the panel. `greet`: the first body (evolving keeps it).
pub fn make_xeno(world: &mut World, mob: Entity, greet: bool) {
    if world.get::<Xeno>(mob).is_none() {
        return;
    }
    crate::noncrew_jobs::set_assigned_role(world, mob, "/datum/job/xenomorph");
    let client = world.get::<PlayerMob>(mob).map(|p| p.client);
    let key = client
        .and_then(|c| world.get::<ClientToken>(c))
        .map(|t| t.0.clone())
        .unwrap_or_default();
    if let Some(mut x) = world.get_mut::<Xeno>(mob) {
        x.key = key;
    }
    world
        .entity_mut(mob)
        .insert((AlienHivemind, AntagHudData::default()));
    if let Some(client) = client {
        crate::hallucination::see_groups(world, client, XENO_SIGHT, true);
        world.write_message(OpenUi::new::<AntagHudUi>(client, mob, mob, ""));
        if greet {
            world.write_message(OpenUi::new::<XenoInfoWindow>(client, mob, mob, ANTAG_NAME));
            tell(
                world,
                client,
                Span::Bold,
                "Вы - личинка ксеноморфа. Прячьтесь, пока не сможете эволюционировать. \
                 Используйте :a для общения с ульем.",
            );
        }
    }
    abilities::refresh_buttons(world, mob);
}

// ---------------------------------------------------------------------------------------------
// Life

/// `Life` of aliens: plasma and healing (`plasmavessel/on_life`), larva growth, the stomach, the
/// hive's return after the queen's death.
fn life(world: &mut World, mut clock: Local<f64>) {
    let t = now(world);
    if t < *clock {
        return;
    }
    let dt = (t - (*clock - SSMOBS_WAIT_SECS)).clamp(SSMOBS_WAIT_SECS, LIFE_DT_CAP) as f32;
    *clock = t + SSMOBS_WAIT_SECS;
    let mobs: Vec<Entity> = world
        .query_filtered::<Entity, With<Xeno>>()
        .iter(world)
        .collect();
    for mob in mobs {
        let Some(stat) = world.get::<Stat>(mob).copied() else {
            continue;
        };
        if stat == Stat::Dead {
            continue;
        }
        tick_vessel(world, mob, dt);
        tick_hive(world, mob, t);
        digest(world, mob);
        abilities::refresh_buttons(world, mob);
    }
    sync_sight(world);
}

fn tick_vessel(world: &mut World, mob: Entity, dt: f32) {
    let Some(x) = world.get::<Xeno>(mob).cloned() else {
        return;
    };
    let on_weeds = crate::explosions::turf_of(world, mob)
        .is_some_and(|p| reproduction::weed_at(world, p).is_some());
    let hurt = world.get::<Health>(mob).is_some_and(|h| h.health() < h.max);
    let plasma = if !on_weeds {
        0.1 * x.plasma_rate * dt
    } else if !hurt {
        x.plasma_rate * dt
    } else {
        0.5 * x.plasma_rate * dt
    };
    adjust_plasma(world, mob, plasma);
    if on_weeds && hurt {
        heal(world, mob, HEAL_RATE * dt);
    }
    if x.caste == Caste::Larva {
        grow(world, mob, 0.5 * dt);
    }
}

/// `owner.adjustPlasma`; a larva's growth counts every positive change.
pub fn adjust_plasma(world: &mut World, mob: Entity, amount: f32) {
    let Some(mut x) = world.get_mut::<Xeno>(mob) else {
        return;
    };
    x.plasma = (x.plasma + amount).clamp(0.0, x.max_plasma);
    if x.caste == Caste::Larva && amount > 0.0 {
        x.amount_grown = (x.amount_grown + 1.0).min(MAX_GROWTH);
    }
}

fn heal(world: &mut World, mob: Entity, amount: f32) {
    let Some(mut health) = world.get::<Health>(mob).cloned() else {
        return;
    };
    health.adjust(DamageType::Brute, -amount);
    world.entity_mut(mob).insert(health);
}

/// `amount_grown` and the sprite of the new size (`larva/update_icons`).
pub fn grow(world: &mut World, mob: Entity, amount: f32) {
    let Some(mut x) = world.get_mut::<Xeno>(mob) else {
        return;
    };
    x.amount_grown = (x.amount_grown + amount).min(MAX_GROWTH);
    let grown = x.amount_grown;
    let state = larva_state(grown);
    let Some(mut basic) = world.get_mut::<crate::mobs::BasicMob>(mob) else {
        return;
    };
    let living = format!("larva{state}");
    if basic.icon_living == living {
        return;
    }
    basic.icon_living = living;
    basic.icon_dead = format!("larva{state}_dead");
    let dead = world.get::<Stat>(mob) == Some(&Stat::Dead);
    crate::mobs::set_look(world, mob, dead);
}

/// `larva/update_icons`: the size by `amount_grown`.
pub fn larva_state(grown: f32) -> u32 {
    if grown > 80.0 {
        2
    } else if grown > 50.0 {
        1
    } else {
        0
    }
}

fn tick_hive(world: &mut World, mob: Entity, t: f64) {
    let Some(x) = world.get::<Xeno>(mob) else {
        return;
    };
    if x.cut_off_until == 0.0 || t < x.cut_off_until {
        return;
    }
    if let Some(mut x) = world.get_mut::<Xeno>(mob) {
        x.cut_off_until = 0.0;
    }
    world.entity_mut(mob).insert(AlienHivemind);
    tell_mob(
        world,
        mob,
        Span::Notice,
        "Боль от смерти королевы ослабевает. Вы снова чувствуете связь с ульем.",
    );
}

/// `stomach/on_life`: what is inside is burned a little every third fire; a corpse is gone
/// (`content_died`).
fn digest(world: &mut World, mob: Entity) {
    let Some(mut x) = world.get_mut::<Xeno>(mob) else {
        return;
    };
    x.fires = x.fires.wrapping_add(1);
    if x.fires % DIGEST_FIRES != 0 {
        return;
    }
    let inside: Vec<Entity> = world
        .query_filtered::<(Entity, &InCloset), With<Health>>()
        .iter(world)
        .filter(|(_, c)| c.closet == mob)
        .map(|(e, _)| e)
        .collect();
    for prey in inside {
        if world.get::<Stat>(prey) == Some(&Stat::Dead) {
            crate::closets::let_out(world, prey);
            crate::health::gib(world, prey);
            continue;
        }
        crate::bots::hurt_zone(
            world,
            prey,
            crate::zone::BodyZone::Chest,
            DamageType::Brute,
            DIGEST_BRUTE,
        );
        crate::bots::hurt_zone(
            world,
            prey,
            crate::zone::BodyZone::Chest,
            DamageType::Burn,
            DIGEST_BURN,
        );
    }
}

/// `AddInfectionImages`: the clients of aliens see the hosts' images (other clients lose it).
fn sync_sight(world: &mut World) {
    let clients: Vec<Entity> = world
        .query_filtered::<&PlayerMob, With<Xeno>>()
        .iter(world)
        .map(|p| p.client)
        .collect();
    for client in clients {
        if world
            .get::<SeeGroups>(client)
            .is_none_or(|g| g.0 & XENO_SIGHT == 0)
        {
            crate::hallucination::see_groups(world, client, XENO_SIGHT, true);
        }
    }
    let seeing: Vec<Entity> = world
        .query::<(Entity, &SeeGroups)>()
        .iter(world)
        .filter(|(_, g)| g.0 & XENO_SIGHT != 0)
        .map(|(e, _)| e)
        .collect();
    for client in seeing {
        let mob = world.get_resource::<Players>().and_then(|p| p.mob(client));
        if !mob.is_some_and(|m| is_xeno(world, m)) {
            crate::hallucination::see_groups(world, client, XENO_SIGHT, false);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Hive

/// `alien_talk`: every conscious alien in the hive and every ghost read it; the queen's is big.
fn hive_chat(
    mut spoken: MessageReader<Spoke>,
    members: Query<(Entity, &Stat), With<AlienHivemind>>,
    ghosts: Query<Entity, With<Ghost>>,
    xenos: Query<&Xeno>,
    listeners: Res<Listeners>,
    mut chat: MessageWriter<SendChat>,
) {
    for spoke in spoken.read() {
        if spoke.radio != Some(RadioTarget::Hive) {
            continue;
        }
        let queen = xenos
            .get(spoke.speaker)
            .is_ok_and(|x| x.caste == Caste::Queen);
        let clients: Vec<Entity> = members
            .iter()
            .filter(|(_, s)| !matches!(s, Stat::HardCrit | Stat::Dead))
            .map(|(e, _)| e)
            .chain(ghosts.iter())
            .filter_map(|e| listeners.client_of(e))
            .collect();
        info!(target: "say", "xeno hive {}: {}", spoke.name, spoke.text);
        chat.write(SendChat {
            clients,
            message: hive_message(&spoke.name, &spoke.text, queen),
        });
    }
}

/// `alien_talk`'s line; the span is the changeling's, the closest chat style.
pub fn hive_message(name: &str, text: &str, queen: bool) -> ChatMessage {
    let span = if queen {
        Span::CultLarge
    } else {
        Span::Changeling
    };
    ChatMessage {
        channel: ChatChannel::Radio,
        segments: vec![
            ChatSegment::new(span, format!("Связь улья, {name}: ")),
            ChatSegment::new(span, text),
        ],
    }
}

/// `hivenode/queen_death` on the others.
fn queen_death(world: &mut World, dead_queen: Entity) {
    let t = now(world);
    let others: Vec<Entity> = world
        .query_filtered::<Entity, With<Xeno>>()
        .iter(world)
        .filter(|&e| e != dead_queen)
        .collect();
    for mob in others {
        if world.get::<Stat>(mob) == Some(&Stat::Dead) {
            continue;
        }
        tell_mob(
            world,
            mob,
            Span::BoldWarning,
            "Ваша Королева была повержена!",
        );
        tell_mob(
            world,
            mob,
            Span::Danger,
            "Вас охватывает невероятная агония! Вы растеряны, и ваша связь с ульем прервана.",
        );
        crate::status_effects::apply(world, mob, Incap::Stun, 20.0);
        world.entity_mut(mob).remove::<AlienHivemind>();
        if let Some(mut x) = world.get_mut::<Xeno>(mob) {
            x.cut_off_until = t + QUEEN_DEATH_SECS;
        }
        abilities::refresh_buttons(world, mob);
    }
}

// ---------------------------------------------------------------------------------------------
// Evolution

/// `alien_evolve`: the new body of `caste` takes the mind; the old one is gone.
pub fn evolve(world: &mut World, old: Entity, caste: Caste) -> Option<Entity> {
    let pos = crate::explosions::turf_of(world, old)?;
    let identifier = world.get::<Xeno>(old)?.identifier;
    let carried = world.get::<Xeno>(old)?.carried.clone();
    let new = crate::mobs::spawn::spawn_mob(world, caste.path(), pos)?;
    let facing = world.get::<ss13_shared::Facing>(old).copied();
    if let Some(f) = facing {
        world.entity_mut(new).insert(f);
    }
    let base = world
        .get::<Name>(new)
        .map(|n| {
            n.as_str()
                .rsplit_once(" (")
                .map_or(n.as_str(), |(b, _)| b)
                .to_owned()
        })
        .unwrap_or_default();
    world
        .entity_mut(new)
        .insert(Name::new(format!("{base} ({identifier})")));
    if let Some(mut x) = world.get_mut::<Xeno>(new) {
        x.identifier = identifier;
    }
    if !crate::medical::brainmob::transfer_mind(world, old, new) {
        world.despawn(new);
        return None;
    }
    let contents: Vec<Entity> = world
        .query::<(Entity, &InCloset)>()
        .iter(world)
        .filter(|(_, c)| c.closet == old)
        .map(|(e, _)| e)
        .collect();
    for mob in contents {
        crate::closets::let_out(world, mob);
        crate::closets::put_mob_inside(world, mob, new);
    }
    for hugger in carried {
        reproduction::drop_hugger(world, hugger, pos);
    }
    make_xeno(world, new, false);
    world.despawn(old);
    crate::construction::tile_changed(world, pos);
    Some(new)
}

// ---------------------------------------------------------------------------------------------
// Clicks: `attack_alien`, `attack_larva` and the structures

/// A conscious alien's click on what is next to it.
fn xeno_clicks(world: &mut World, mut cursor: Local<MessageCursor<crate::click::ClickResolved>>) {
    let clicks: Vec<crate::click::ClickResolved> = cursor
        .read(world.resource::<Messages<crate::click::ClickResolved>>())
        .copied()
        .collect();
    let t = now(world);
    for click in clicks {
        let (mob, target) = (click.mob, click.target);
        let Some(caste) = world.get::<Xeno>(mob).map(|x| x.caste) else {
            continue;
        };
        if target == mob
            || world.get::<Stat>(mob) != Some(&Stat::Conscious)
            || world
                .get::<crate::combat::NextMove>(mob)
                .is_some_and(|n| t < n.0)
            || world.get::<crate::tools::DoAfter>(mob).is_some()
            || world.get::<InCloset>(mob).is_some()
        {
            continue;
        }
        if world.get::<Health>(target).is_some() {
            attack_living(world, mob, target, caste, click.modifiers.right);
        } else {
            abilities::structure_click(world, click.client, mob, target, caste);
        }
    }
}

/// The attack cooldown, in seconds, of an unarmed hit (`CLICK_CD_MELEE`).
fn set_cooldown(world: &mut World, mob: Entity, t: f64) {
    world
        .entity_mut(mob)
        .insert(crate::combat::NextMove(t + crate::combat::CLICK_CD_MELEE));
}

fn combat(world: &World, mob: Entity) -> bool {
    world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0)
}

/// `attack_alien` / `attack_larva` on a living target; `Sentinels` and hive sisters are licked
/// rather than clawed.
fn attack_living(world: &mut World, mob: Entity, target: Entity, caste: Caste, right: bool) {
    let t = now(world);
    set_cooldown(world, mob, t);
    if is_xeno(world, target) {
        sister_click(world, mob, target);
        return;
    }
    let (user, victim) = (name_of(world, mob), name_of(world, target));
    if caste == Caste::Larva {
        larva_bite(world, mob, target, &user, &victim);
        return;
    }
    if right && world.get::<crate::inventory::Inventory>(target).is_some() {
        disarm(world, mob, target, &user, &victim);
        return;
    }
    if !combat(world, mob) {
        if abilities::try_hug_or_devour(world, mob, target) {
            return;
        }
        let own = format!("Вы поглаживаете {victim} своей косоподобной рукой.");
        let others = format!("{user} поглаживает {victim} своей косоподобной рукой.");
        visible(world, mob, &own, &others, Span::Notice);
        return;
    }
    slash(world, mob, target, &user, &victim);
}

/// `attack_alien` of alien on alien: a wake-up or a nip.
fn sister_click(world: &mut World, mob: Entity, target: Entity) {
    let (user, victim) = (name_of(world, mob), name_of(world, target));
    if combat(world, mob) {
        if world.get::<Stat>(target) == Some(&Stat::Dead) {
            tell_mob(
                world,
                mob,
                Span::Warning,
                format!("{victim} слишком ранен для этого."),
            );
            return;
        }
        let own = format!("Вы кусаете {victim}!");
        let others = format!("{user} кусает {victim}!");
        visible(world, mob, &own, &others, Span::Danger);
        heal(world, target, -1.0);
        return;
    }
    for kind in [
        Incap::Stun,
        Incap::Knockdown,
        Incap::Immobilize,
        Incap::Paralyze,
        Incap::Unconscious,
    ] {
        crate::status_effects::adjust(world, target, kind, -6.0);
    }
    crate::status_effects::adjust(world, target, Incap::Sleeping, -10.0);
    let others = format!("{user} трётся головой об {victim}, пытаясь разбудить!");
    visible(world, mob, &others, &others, Span::Notice);
}

/// `larva/attack_larva`: 1-3 brute, which feeds the larva's growth.
fn larva_bite(world: &mut World, mob: Entity, target: Entity, user: &str, victim: &str) {
    let damage = world.resource_mut::<CombatRng>().range(1, 3) as f32;
    let own = format!("Вы кусаете {victim}!");
    let others = format!("{user} кусает {victim}!");
    visible(world, mob, &own, &others, Span::Danger);
    crate::mobs::defense::hurt(
        world,
        target,
        DamageType::Brute,
        damage,
        Some(crate::zone::BodyZone::Chest),
    );
    if world.get::<Stat>(target) != Some(&Stat::Dead) {
        grow(world, mob, damage);
    }
}

/// `carbon/human/attack_alien` with a combat hit: 90% `melee_damage_lower..upper`.
fn slash(world: &mut World, mob: Entity, target: Entity, user: &str, victim: &str) {
    let (low, high) = world
        .get::<crate::mobs::BasicMob>(mob)
        .map_or((20.0, 20.0), |b| {
            (b.def.melee_damage_lower, b.def.melee_damage_upper)
        });
    let hit = world.resource_mut::<CombatRng>().chance(90.0);
    if !hit {
        let own = format!("Вы бросаетесь на {victim}!");
        let others = format!("{user} бросается на {victim}!");
        visible(world, mob, &own, &others, Span::Danger);
        return;
    }
    let damage = world
        .resource_mut::<CombatRng>()
        .range(low as u32, high as u32) as f32;
    let zone = [
        crate::zone::BodyZone::Chest,
        crate::zone::BodyZone::LArm,
        crate::zone::BodyZone::RArm,
        crate::zone::BodyZone::LLeg,
        crate::zone::BodyZone::RLeg,
    ][world.resource_mut::<CombatRng>().range(0, 4) as usize];
    let own = format!("Вы режете {victim}!");
    let others = format!("{user} режет {victim}!");
    visible(world, mob, &own, &others, Span::Danger);
    crate::mobs::defense::hurt(world, target, DamageType::Brute, damage, Some(zone));
    if let Some(&pos) = world.get::<TilePos>(target) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, "sound/items/weapons/slice.ogg", 25.0, true),
        );
    }
}

/// `human/attack_alien` with a right click: the held item out of the hand, or a shove (35
/// stamina, 4 tiles of flight), or the floor for 5 s when the victim is incapacitated.
fn disarm(world: &mut World, mob: Entity, target: Entity, user: &str, victim: &str) {
    let Some(&pos) = world.get::<TilePos>(target) else {
        return;
    };
    let held = world
        .get::<crate::inventory::Inventory>(target)
        .and_then(|i| i.active_item());
    if let Some(item) = held
        && !crate::inventory::no_drop(world, item)
    {
        if let Some(slot) = world
            .get::<crate::inventory::Inventory>(target)
            .and_then(|i| i.slot_of(item))
        {
            crate::inventory::unequip(world, target, pos, slot);
        }
        crate::inventory::put_down(world, item, pos);
        let own = format!("Вы обезоруживаете {victim}!");
        let others = format!("{user} обезоруживает {victim}!");
        visible(world, mob, &own, &others, Span::Danger);
        return;
    }
    if world
        .get::<crate::status_effects::Incapacitation>(target)
        .is_some_and(|i| i.incapacitated)
    {
        crate::status_effects::apply(world, target, Incap::Paralyze, 5.0);
        let own = format!("Вы впечатываете {victim} в пол под вами!");
        let others = format!("{user} впечатывает {victim} в пол!");
        visible(world, mob, &own, &others, Span::Danger);
        return;
    }
    crate::status_effects::adjust_stamina(world, target, 35.0);
    if let Some(&from) = world.get::<TilePos>(mob) {
        abilities::fling(world, target, pos, push_dir(from, pos), 4);
    }
    let own = format!("Вы толкаете {victim} с огромной силой!");
    let others = format!("{user} сбивает с ног {victim}!");
    visible(world, mob, &own, &others, Span::Danger);
}

/// A unit step from `from` towards `to`, as a `(dx, dy)`.
fn push_dir(from: TilePos, to: TilePos) -> (i32, i32) {
    ((to.x - from.x).signum(), (to.y - from.y).signum())
}

/// The leap's end (`hunter/throw_impact`): a living target hit is floored, a wall stuns the
/// hunter.
fn leap_landings(world: &mut World) {
    let leaping: Vec<(Entity, Option<Entity>, TilePos)> = world
        .query_filtered::<(Entity, &Xeno), Without<crate::throw::Thrown>>()
        .iter(world)
        .filter_map(|(e, x)| x.leaping.map(|(t, p)| (e, t, p)))
        .collect();
    let t = now(world);
    for (mob, target, landing) in leaping {
        if let Some(mut x) = world.get_mut::<Xeno>(mob) {
            x.leaping = None;
            x.leap_ready_at = t + abilities::POUNCE_COOLDOWN;
        }
        let Some(&pos) = world.get::<TilePos>(mob) else {
            continue;
        };
        if let Some(victim) = target.filter(|&v| world.get::<Health>(v).is_some()) {
            let reached = world
                .get::<TilePos>(victim)
                .and_then(|p| p.chebyshev(pos))
                .is_some_and(|d| d <= 1);
            if reached {
                pounce(world, mob, victim);
                continue;
            }
        }
        if pos != landing
            && !world
                .resource::<ss13_world::SpatialIndex>()
                .dense_on_tile(pos.step(abilities::dir_towards(pos, landing)))
                .is_empty()
        {
            let user = name_of(world, mob);
            visible(
                world,
                mob,
                "Вы врезаетесь в препятствие!",
                &format!("{user} врезается в препятствие!"),
                Span::Danger,
            );
            crate::status_effects::apply(world, mob, Incap::Paralyze, 4.0);
        }
    }
}

/// `hunter/throw_impact` on a living target: 5 s on the floor, the leap is switched off.
fn pounce(world: &mut World, mob: Entity, victim: Entity) {
    let (user, target) = (name_of(world, mob), name_of(world, victim));
    visible(
        world,
        mob,
        &format!("Вы набрасываетесь на {target}!"),
        &format!("{user} набрасывается на {target}!"),
        Span::Danger,
    );
    crate::status_effects::apply(world, victim, Incap::Paralyze, 5.0);
    abilities::set_leap(world, mob, false);
}

fn on_death(world: &mut World) {
    let died: Vec<Entity> = world
        .query_filtered::<(Entity, Ref<Stat>), With<Xeno>>()
        .iter(world)
        .filter(|(_, s)| s.is_changed() && **s == Stat::Dead)
        .map(|(e, _)| e)
        .collect();
    for mob in died {
        let Some(x) = world.get::<Xeno>(mob).cloned() else {
            continue;
        };
        if x.aim.is_some() {
            abilities::set_aim(world, mob, None);
        }
        crate::antag::morph::barf(world, mob);
        if let Some(pos) = crate::explosions::turf_of(world, mob) {
            for hugger in x.carried {
                reproduction::drop_hugger(world, hugger, pos);
            }
        }
        if let Some(mut x) = world.get_mut::<Xeno>(mob) {
            x.carried.clear();
        }
        if x.caste == Caste::Queen {
            queen_death(world, mob);
        }
    }
}

fn open_info(world: &mut World, mut acts: Local<MessageCursor<UiActed<AntagHudUi>>>) {
    let pending: Vec<(Entity, Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<AntagHudUi>>>())
        .map(|a| (a.client, a.actor, a.source))
        .collect();
    for (client, actor, source) in pending {
        if world.get::<Xeno>(source).is_some() {
            world.write_message(OpenUi::new::<XenoInfoWindow>(
                client, actor, source, ANTAG_NAME,
            ));
        }
    }
}

/// Each alien and whether it lived.
pub fn roundend_report(world: &mut World, _end: RoundEnd) -> Vec<ChatMessage> {
    let mut query = world.query::<(Entity, &Xeno)>();
    let mut all: Vec<(Entity, String)> = query
        .iter(world)
        .filter(|(_, x)| !x.key.is_empty())
        .map(|(e, x)| (e, x.key.clone()))
        .collect();
    if all.is_empty() {
        return Vec::new();
    }
    all.sort_by_key(|(e, _)| *e);
    let mut out = vec![ChatMessage {
        channel: ChatChannel::System,
        segments: vec![ChatSegment::new(
            Span::Bold,
            format!("{ROUNDEND_CATEGORY} были:"),
        )],
    }];
    for (mob, key) in all {
        out.push(super::traitor::print_player(world, mob, &key));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn castes_follow_the_types() {
        assert_eq!(Caste::of(LARVA), Some(Caste::Larva));
        assert_eq!(Caste::of(QUEEN), Some(Caste::Queen));
        assert_eq!(Caste::of(PRAETORIAN), Some(Caste::Praetorian));
        assert_eq!(Caste::of("/mob/living/carbon/human"), None);
        assert!(Caste::Praetorian.royal() && Caste::Queen.royal());
        assert!(!Caste::Drone.royal());
        assert!(!Caste::Larva.adult() && Caste::Hunter.adult());
    }

    #[test]
    fn vessels_match_the_organ_types() {
        assert_eq!(Caste::Larva.vessel().max, 100.0);
        assert_eq!(Caste::Hunter.vessel().max, 150.0);
        assert_eq!(Caste::Sentinel.vessel().rate, 5.0);
        assert_eq!(Caste::Drone.vessel().stored, 200.0);
        assert_eq!(Caste::Queen.vessel().rate, 10.0);
    }

    #[test]
    fn a_larva_grows_with_plasma_and_time() {
        let mut world = World::new();
        let mob = world.spawn(Xeno::new(Caste::Larva, 7)).id();
        adjust_plasma(&mut world, mob, 5.0);
        assert_eq!(world.get::<Xeno>(mob).unwrap().amount_grown, 1.0);
        grow(&mut world, mob, 60.0);
        assert_eq!(world.get::<Xeno>(mob).unwrap().amount_grown, 61.0);
        assert_eq!(larva_state(61.0), 1);
        assert_eq!(larva_state(90.0), 2);
        assert_eq!(larva_state(10.0), 0);
        grow(&mut world, mob, 500.0);
        assert_eq!(world.get::<Xeno>(mob).unwrap().amount_grown, MAX_GROWTH);
    }

    #[test]
    fn plasma_is_clamped_to_the_vessel() {
        let mut world = World::new();
        let mob = world.spawn(Xeno::new(Caste::Hunter, 7)).id();
        adjust_plasma(&mut world, mob, 1000.0);
        assert_eq!(world.get::<Xeno>(mob).unwrap().plasma, 150.0);
        adjust_plasma(&mut world, mob, -1000.0);
        assert_eq!(world.get::<Xeno>(mob).unwrap().plasma, 0.0);
    }
}
