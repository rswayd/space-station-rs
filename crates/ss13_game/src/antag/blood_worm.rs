//! Blood worms after BandaStation's `code/modules/antagonists/blood_worm`: the mob
//! (`/mob/living/basic/blood_worm/{hatchling,juvenile,adult}`), the host handling
//! (`blood_worm_host_handling.dm`), the invade, leave host, revive host and leech actions and the
//! "Midround Blood Worm" light midround (`/datum/dynamic_ruleset/midround/from_ghosts/blood_worms`).
//!
//! - **Selection** ([`spawn_worms`], from `super::midround`): the polled ghosts become hatchlings
//!   in random unwelded vents; the "unidentified lifeforms" announcement follows in 450-750 s.
//! - **Invade** (button, then a click on an adjacent human corpse with enough blood, 5 s): the
//!   worm goes inside it (hidden, [`crate::closets::put_mob_inside`]); the host's blood counts
//!   towards growth and the worm's health is added to it as blood (`HEALTH_TO_BLOOD`). Inside, the
//!   worm's health *is* the host's blood ([`sync_health`]): capped at `max × 5.6`, the worm's regen
//!   refills it, at 10% the worm is pushed out.
//! - **Host control**: a living host is driven by the worm's client ([`possess_host`]); the host's
//!   own ghost is cut from its body while the worm is in it (tg's backseat) and gets it back when
//!   the worm leaves. The host dying hands control back to the worm inside.
//! - **Revive Host** (dead host, 3 × 2 s, 30 s cooldown): `revive()` of a body not too damaged.
//! - **Leave Host** (3 s): the worm leaves, the host's blood goes with it and the host dies.
//! - **Leech** (outside; button, then a click on an adjacent mob with blood): 1 s to bite, then
//!   every second `leech_rate` blood is drained into the worm's health and growth; a living
//!   victim takes `oxyloss_rate` oxygen damage; 5 s cooldown after.
// shortcut: no spitting, cocoon/growth into the next stage (the consumed blood is only counted),
// transfusion, blood tester, objectives and team, roundstart ruleset, AntagInfoBloodWorm panel,
// round-end report, wormspeak, grab/flooring during leeching or leeching from containers,
// host traits apart from no oxygen damage (no bleed modifier, temperature damage, HUD, hunger and
// crit immunities), changeling expel, slide under doors, pressure damage and the leave animation;
// add them with the cocoon/growth wave.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::IconIndex;
use ss13_mapload::spawner::Rng;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::server::{UiActed, UiSystems};

use super::AntagHudData;
use super::dynamic::DynamicRng;
use crate::actions::{ActionButton, ActionButtons, ActionButtonsUi, set_buttons};
use crate::click::{ClickIntercept, InterceptedClick};
use crate::construction::Job;
use crate::ghost::Ghost;
use crate::health::{DamageType, Health, Stat};
use crate::medical::blood::{BLOOD_VOLUME_NORMAL, Blood};
use crate::server::PlayerMob;
use crate::tools::{Actor, DoAfter, do_after, now, tell};

pub use super::ROLE_BLOOD_WORM_INFESTATION;
pub const HATCHLING: &str = "/mob/living/basic/blood_worm/hatchling";
pub const JUVENILE: &str = "/mob/living/basic/blood_worm/juvenile";
pub const ADULT: &str = "/mob/living/basic/blood_worm/adult";
/// `BLOOD_WORM_HEALTH_TO_BLOOD`.
pub const HEALTH_TO_BLOOD: f32 = BLOOD_VOLUME_NORMAL / 100.0;
/// `BLOOD_WORM_EJECT_THRESHOLD`.
pub const EJECT_THRESHOLD: f32 = 0.1;
/// The midround's `announce_worms` delay, seconds.
const ANNOUNCE_SECS: (i64, i64) = (450, 750);
const INVADE_DS: f32 = 50.0;
const EJECT_SECS: f64 = 3.0;
const REVIVE_STEP_SECS: f64 = 2.0;
const REVIVE_COOLDOWN: f64 = 30.0;
const LEECH_GRAB_DS: f32 = 10.0;
const LEECH_TICK_DS: f32 = 10.0;
const LEECH_COOLDOWN: f64 = 5.0;

const INVADE_ID: &str = "bloodworm:invade";
const LEECH_ID: &str = "bloodworm:leech";
const EJECT_ID: &str = "bloodworm:eject";
const REVIVE_ID: &str = "bloodworm:revive";
const ACTIONS_DMI: &str = "icons/mob/actions/actions_blood_worm.dmi";
const BACKGROUNDS_DMI: &str = "icons/mob/actions/backgrounds.dmi";

/// A growth stage's numbers (the subtypes' vars and their actions').
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Hatchling,
    Juvenile,
    Adult,
}

impl Stage {
    pub fn of_path(path: &str) -> Option<Self> {
        match path {
            HATCHLING => Some(Self::Hatchling),
            JUVENILE => Some(Self::Juvenile),
            ADULT => Some(Self::Adult),
            _ => None,
        }
    }

    /// `regen_rate`, health per second.
    pub fn regen_rate(self) -> f32 {
        match self {
            Self::Hatchling => 0.3,
            Self::Juvenile => 0.4,
            Self::Adult => 0.5,
        }
    }

    /// The leech action's `leech_rate` (blood per second) and `oxyloss_rate`.
    pub fn leech(self) -> (f32, f32) {
        match self {
            Self::Hatchling => (BLOOD_VOLUME_NORMAL * 0.05, 11.0),
            Self::Juvenile => (BLOOD_VOLUME_NORMAL * 0.075, 15.0),
            Self::Adult => (BLOOD_VOLUME_NORMAL * 0.1, 20.0),
        }
    }

    /// `leave_host_duration`, seconds.
    pub fn leave_host_secs(self) -> f64 {
        match self {
            Self::Hatchling => 0.6,
            Self::Juvenile => 1.0,
            Self::Adult => 1.4,
        }
    }
}

/// On the worm: the antagonist and the mob's state.
#[derive(Component, Clone, Debug)]
pub struct BloodWorm {
    pub stage: Stage,
    pub host: Option<Entity>,
    /// `is_possessing_host`.
    pub possessing: bool,
    /// The host's own ghost, cut from its body while the worm is inside (tg `backseat`).
    pub backseat: Option<Entity>,
    pub consumed_blood: f32,
    /// The button armed for a click: [`INVADE_ID`] or [`LEECH_ID`].
    pub aiming: Option<&'static str>,
    pub leech_ready_at: f64,
    pub revive_ready_at: f64,
    /// `Time` the leave host channel ends at.
    pub ejecting: Option<f64>,
    /// The revive channel: (beats done, `Time` of the next beat).
    pub reviving: Option<(u8, f64)>,
}

impl BloodWorm {
    pub fn new(stage: Stage) -> Self {
        Self {
            stage,
            host: None,
            possessing: false,
            backseat: None,
            consumed_blood: 0.0,
            aiming: None,
            leech_ready_at: 0.0,
            revive_ready_at: 0.0,
            ejecting: None,
            reviving: None,
        }
    }
}

/// On a host: its worm (`TRAIT_BLOOD_WORM_HOST`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct BloodWormHost(pub Entity);

/// A blood worm's timed actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WormJob {
    Invade,
    /// The bite before leeching (`leech_grab_delay`).
    Bite,
    /// One second of leeching.
    Leech,
}

/// `Time` of the pending `announce_worms` (0: none).
#[derive(Resource, Default)]
struct PendingAnnounce(f64);

pub struct BloodWormPlugin;

impl Plugin for BloodWormPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingAnnounce>()
            .add_systems(
                Update,
                (trigger_buttons, aimed_clicks).in_set(UiSystems::Handle),
            )
            .add_systems(
                Update,
                (tick, channels, announce)
                    .chain()
                    .after(crate::health::HealthSystems),
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

fn tell_mob(world: &mut World, mob: Entity, span: Span, text: impl Into<String>) {
    crate::tools::tell_mob(world, mob, span, text);
}

/// The mob the worm's player drives: the host while possessing it, else the worm.
fn driver(world: &World, worm: Entity) -> Entity {
    match world.get::<BloodWorm>(worm) {
        Some(w) if w.possessing => w.host.unwrap_or(worm),
        _ => worm,
    }
}

fn visible(world: &mut World, mob: Entity, own: &str, others: &str, span: Span) {
    crate::chem::visible(world, mob, own, others, span);
}

fn sound(world: &mut World, at: Entity, file: &str, vol: f32) {
    if let Some(&pos) = world.get::<TilePos>(at) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, file, vol, true));
    }
}

fn health_of(world: &World, mob: Entity) -> Option<(f32, f32)> {
    world.get::<Health>(mob).map(|h| (h.health(), h.max))
}

/// `get_eject_volume_threshold`.
pub fn eject_threshold(max_health: f32) -> f32 {
    max_health * HEALTH_TO_BLOOD * EJECT_THRESHOLD
}

// ---------------------------------------------------------------------------------------------
// Selection

/// `can_be_selected`: a vent to crawl out of.
pub fn can_spawn(world: &mut World) -> bool {
    !super::xeno::vents(world).is_empty()
}

/// `create_ruleset_body` + `assign_role` for each `(client, ghost)`: hatchlings in vents, then
/// the announcement timer (`execute`).
pub fn spawn_worms(world: &mut World, ghosts: &[(Entity, Entity)]) -> Vec<Entity> {
    let mut pool = super::xeno::vents(world);
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
        let Some(worm) = crate::mobs::spawn::spawn_mob(world, HATCHLING, pos) else {
            break;
        };
        if !crate::medical::brainmob::possess(world, client, ghost, worm) {
            world.despawn(worm);
            continue;
        }
        world.try_despawn(ghost).ok();
        crate::ventcrawl::move_into_vent(world, worm, vent);
        make_worm(world, worm, Stage::Hatchling);
        info!("{worm} ({client}) is a blood worm in {vent}");
        out.push(worm);
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

/// `add_antag_datum(/datum/antagonist/blood_worm)`: the greeting and the buttons.
pub fn make_worm(world: &mut World, mob: Entity, stage: Stage) -> bool {
    if world.get::<BloodWorm>(mob).is_some() || world.get_entity(mob).is_err() {
        return false;
    }
    world
        .entity_mut(mob)
        .insert((BloodWorm::new(stage), AntagHudData::default()));
    refresh_buttons(world, mob);
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        tell(world, client, Span::Bold, "Вы - Кровяной червь.");
        tell(
            world,
            client,
            Span::Bold,
            "Разновидность космических пиявок огромных размеров, известных своей свирепостью при \
             охоте. Большинству неизвестно ваше происхождение, но для некоторых вы являетесь одним \
             из их величайших творений. Неудачный проект биологического оружия Синдиката, закрытый \
             покровителями из-за 'отсутствия результатов', и всё же... Вы обнаруживаете себя на \
             космической станции Нанотрейзен. Прекрасная среда обитания, не правда ли?",
        );
        tell(
            world,
            client,
            Span::BoldWarning,
            "УБЕЙ, ПОГЛОТИ, РАЗМНОЖЬСЯ, ЗАХВАТИ.",
        );
        sound(world, mob, "sound/effects/magic/exit_blood.ogg", 60.0);
    }
    true
}

// ---------------------------------------------------------------------------------------------
// Buttons

fn refresh_buttons(world: &mut World, worm: Entity) {
    let Some(state) = world.get::<BloodWorm>(worm).cloned() else {
        return;
    };
    let t = now(world);
    let icon = |world: &World, dmi: &str, s: &str| {
        world
            .get_resource::<IconIndex>()
            .and_then(|i| i.get(dmi, s))
    };
    let button =
        |world: &World, id: &str, name: &str, state_name: &str, active: bool| ActionButton {
            id: id.to_owned(),
            name: name.to_owned(),
            icon: icon(world, ACTIONS_DMI, state_name),
            background: icon(world, BACKGROUNDS_DMI, "bg_demon"),
            border: icon(world, BACKGROUNDS_DMI, "bg_demon_border"),
            available: true,
            cooldown: 0.0,
            active,
            maptext: None,
        };
    let ours = [INVADE_ID, LEECH_ID, EJECT_ID, REVIVE_ID];
    let keep = |world: &World, mob: Entity| -> Vec<(ActionButton, f64)> {
        world
            .get::<ActionButtons>(mob)
            .map(|b| {
                b.0.iter()
                    .filter(|(b, _)| !ours.contains(&b.id.as_str()))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut worm_buttons = keep(world, worm);
    match state.host {
        None => {
            worm_buttons.push((
                button(
                    world,
                    INVADE_ID,
                    "Захват трупа",
                    "invade_corpse",
                    state.aiming == Some(INVADE_ID),
                ),
                0.0,
            ));
            worm_buttons.push((
                button(
                    world,
                    LEECH_ID,
                    "Высасывание крови",
                    "leech_blood",
                    state.aiming == Some(LEECH_ID),
                ),
                state.leech_ready_at.max(t),
            ));
            set_buttons(world, worm, worm_buttons);
        }
        Some(host) => {
            let host_actions = vec![
                (
                    button(world, EJECT_ID, "Покинуть носителя", "leave_host", false),
                    0.0,
                ),
                (
                    button(world, REVIVE_ID, "Оживить носителя", "revive_host", false),
                    state.revive_ready_at.max(t),
                ),
            ];
            let mut host_buttons = keep(world, host);
            if state.possessing {
                host_buttons.extend(host_actions);
            } else {
                worm_buttons.extend(host_actions);
            }
            set_buttons(world, worm, worm_buttons);
            set_buttons(world, host, host_buttons);
        }
    }
}

/// The worm a button press or a click of `actor` belongs to.
fn worm_of(world: &World, actor: Entity) -> Option<Entity> {
    if world.get::<BloodWorm>(actor).is_some() {
        return Some(actor);
    }
    let worm = world.get::<BloodWormHost>(actor)?.0;
    world
        .get::<BloodWorm>(worm)
        .is_some_and(|w| w.possessing)
        .then_some(worm)
}

fn set_aim(world: &mut World, worm: Entity, aim: Option<&'static str>) {
    if let Some(mut w) = world.get_mut::<BloodWorm>(worm) {
        w.aiming = aim;
    }
    if aim.is_some() {
        world.entity_mut(worm).insert(ClickIntercept);
    } else {
        world.entity_mut(worm).remove::<ClickIntercept>();
    }
    refresh_buttons(world, worm);
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
        let Some(worm) = worm_of(world, actor) else {
            continue;
        };
        let Some(state) = world.get::<BloodWorm>(worm).cloned() else {
            continue;
        };
        match id.as_str() {
            INVADE_ID | LEECH_ID if state.host.is_none() => {
                if id == LEECH_ID && now(world) < state.leech_ready_at {
                    continue;
                }
                let aim = if id == INVADE_ID { INVADE_ID } else { LEECH_ID };
                let next = (state.aiming != Some(aim)).then_some(aim);
                set_aim(world, worm, next);
            }
            EJECT_ID if state.host.is_some() => start_eject(world, worm),
            REVIVE_ID if state.host.is_some() => start_revive(world, worm),
            _ => {}
        }
    }
}

/// `InterceptClickOn` of the armed invade or leech action.
fn aimed_clicks(world: &mut World, mut clicks: Local<MessageCursor<InterceptedClick>>) {
    let clicks: Vec<InterceptedClick> = clicks
        .read(world.resource::<Messages<InterceptedClick>>())
        .copied()
        .collect();
    for click in clicks {
        let Some(state) = world.get::<BloodWorm>(click.mob).cloned() else {
            continue;
        };
        let Some(aim) = state.aiming else {
            continue;
        };
        if state.host.is_some()
            || world.get::<Stat>(click.mob) != Some(&Stat::Conscious)
            || world.get::<DoAfter>(click.mob).is_some()
        {
            continue;
        }
        let target_name = name_of(world, click.target);
        if click.mob_pos.chebyshev(click.target_pos) != Some(1) && click.mob_pos != click.target_pos
        {
            tell(world, click.client, Span::Warning, "слишком далеко!");
            continue;
        }
        let actor = Actor {
            client: click.client,
            mob: click.mob,
            item: None,
        };
        set_aim(world, click.mob, None);
        if aim == INVADE_ID {
            if let Err(why) = invade_check(world, click.mob, click.target) {
                tell(world, click.client, Span::Warning, why);
                continue;
            }
            let user = name_of(world, click.mob);
            visible(
                world,
                click.mob,
                &format!("Вы начинаете проникать в {target_name}."),
                &format!("{user} начинает проникать в {target_name}!"),
                Span::Danger,
            );
            do_after(
                world,
                actor,
                click.target,
                INVADE_DS,
                0,
                Job::BloodWorm(WormJob::Invade),
            );
        } else {
            if let Err(why) = leech_check(world, click.target) {
                tell(world, click.client, Span::Warning, why);
                continue;
            }
            let user = name_of(world, click.mob);
            visible(
                world,
                click.mob,
                &format!("Вы пытаетесь вгрызться в {target_name}!"),
                &format!("{user} пытается вгрызться в {target_name}!"),
                Span::Danger,
            );
            do_after(
                world,
                actor,
                click.target,
                LEECH_GRAB_DS,
                0,
                Job::BloodWorm(WormJob::Bite),
            );
        }
    }
}

/// `invade_check`: a dead human with blood that is not a host yet and enough of it together
/// with the worm's health to stay above the eject threshold.
pub fn invade_check(world: &World, worm: Entity, victim: Entity) -> Result<(), &'static str> {
    if world.get::<BloodWormHost>(victim).is_some() {
        return Err("уже носитель!");
    }
    if world.get::<crate::human::HumanLook>(victim).is_none() {
        return Err("не гуманоид!");
    }
    if world.get::<Stat>(victim) != Some(&Stat::Dead) {
        return Err("всё ещё жив!");
    }
    let Some(blood) = world.get::<Blood>(victim) else {
        return Err("нет крови!");
    };
    let Some((health, max)) = health_of(world, worm) else {
        return Err("нет крови!");
    };
    if blood.volume + health * HEALTH_TO_BLOOD <= eject_threshold(max) {
        return Err("недостаточно крови, чтобы взять под контроль!");
    }
    Ok(())
}

/// `leech_living_start_check`.
fn leech_check(world: &World, target: Entity) -> Result<(), &'static str> {
    if world.get::<Blood>(target).is_none_or(|b| b.volume <= 0.0) {
        return Err("нет крови!");
    }
    if world.get::<BloodWormHost>(target).is_some() {
        return Err("занято нашим сородичем!");
    }
    Ok(())
}

/// The end of a worm's `do_after`.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: WormJob) {
    let Some(state) = world.get::<BloodWorm>(mob).cloned() else {
        return;
    };
    if state.host.is_some() {
        return;
    }
    let target = action.target;
    match job {
        WormJob::Invade => {
            if invade_check(world, mob, target).is_ok() {
                enter_host(world, mob, target);
            }
        }
        WormJob::Bite | WormJob::Leech => {
            if leech_check(world, target).is_err() {
                end_leech(world, mob);
                return;
            }
            if job == WormJob::Bite {
                let (user, victim) = (name_of(world, mob), name_of(world, target));
                visible(
                    world,
                    mob,
                    &format!("Вы вгрызаетесь в {victim}!"),
                    &format!("{user} жадно вгрызается в {victim}!"),
                    Span::Danger,
                );
                sound(world, target, "sound/items/weapons/bite.ogg", 80.0);
                sound(world, target, "sound/effects/wounds/pierce3.ogg", 100.0);
            } else {
                let (rate, oxy) = state.stage.leech();
                let taken = world
                    .get_mut::<Blood>(target)
                    .map_or(0.0, |mut b| -b.adjust(-rate, 0.0, f32::MAX));
                consume_blood(world, mob, taken, true);
                if world.get::<Stat>(target) != Some(&Stat::Dead)
                    && let Some(mut h) = world.get_mut::<Health>(target)
                {
                    h.adjust(DamageType::Oxy, oxy);
                }
                sound(world, target, "sound/effects/wounds/splatter.ogg", 80.0);
            }
            let actor = Actor {
                client: action.client,
                mob,
                item: None,
            };
            if !do_after(
                world,
                actor,
                target,
                LEECH_TICK_DS,
                0,
                Job::BloodWorm(WormJob::Leech),
            ) {
                end_leech(world, mob);
            }
        }
    }
}

/// The leech loop ended (`StartCooldown`).
// shortcut: an interrupted leech (the worm moved) does not start the cooldown, until do_after
// has an interrupt hook for worms.
fn end_leech(world: &mut World, worm: Entity) {
    let t = now(world);
    if let Some(mut w) = world.get_mut::<BloodWorm>(worm) {
        w.leech_ready_at = t + LEECH_COOLDOWN;
    }
    refresh_buttons(world, worm);
}

/// `consume_blood`: growth progress and, when `heal`, as much health.
pub fn consume_blood(world: &mut World, worm: Entity, amount: f32, heal: bool) {
    if amount <= 0.0 {
        return;
    }
    if let Some(mut w) = world.get_mut::<BloodWorm>(worm) {
        w.consumed_blood += amount;
    }
    if heal {
        adjust_worm_health(world, worm, amount / HEALTH_TO_BLOOD);
    }
}

/// `adjust_worm_health`: the host's blood while inside one, else brute damage.
pub fn adjust_worm_health(world: &mut World, worm: Entity, amount: f32) {
    let host = world.get::<BloodWorm>(worm).and_then(|w| w.host);
    match host {
        Some(host) => {
            if let Some(mut b) = world.get_mut::<Blood>(host) {
                b.adjust(amount * HEALTH_TO_BLOOD, 0.0, f32::MAX);
            }
        }
        None => {
            if let Some(mut h) = world.get_mut::<Health>(worm) {
                let heal = amount.min(h.brute);
                if heal != 0.0 {
                    h.adjust(DamageType::Brute, -heal);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The host

/// `enter_host`.
pub fn enter_host(world: &mut World, worm: Entity, host: Entity) {
    let (user, victim) = (name_of(world, worm), name_of(world, host));
    sound(world, worm, "sound/effects/magic/enter_blood.ogg", 60.0);
    visible(
        world,
        worm,
        &format!("Вы проникаете в {victim}."),
        &format!("{user} проникает в {victim}!"),
        Span::BoldWarning,
    );
    // The host's own ghost may not come back while the worm is in its body.
    let backseat = world
        .query::<(Entity, &Ghost)>()
        .iter(world)
        .find(|(_, g)| g.body == Some(host))
        .map(|(e, _)| e);
    if let Some(ghost) = backseat
        && let Some(mut g) = world.get_mut::<Ghost>(ghost)
    {
        g.body = None;
    }
    let (health, _) = health_of(world, worm).unwrap_or((0.0, 0.0));
    let volume = world.get::<Blood>(host).map_or(0.0, |b| b.volume);
    if let Some(mut w) = world.get_mut::<BloodWorm>(worm) {
        w.host = Some(host);
        w.backseat = backseat;
        w.aiming = None;
    }
    world.entity_mut(worm).remove::<ClickIntercept>();
    world.entity_mut(host).insert(BloodWormHost(worm));
    consume_blood(world, worm, volume, false);
    if let Some(mut b) = world.get_mut::<Blood>(host) {
        b.volume = volume + health * HEALTH_TO_BLOOD;
    }
    crate::closets::put_mob_inside(world, worm, host);
    sync_health(world, worm);
    info!("{worm} (blood worm) entered {host}");
    if world
        .get::<BloodWorm>(worm)
        .is_some_and(|w| w.host.is_some())
        && world.get::<Stat>(host) != Some(&Stat::Dead)
    {
        possess_host(world, worm);
    }
    refresh_buttons(world, worm);
}

/// `leave_host` + `unregister_host`: the worm is out, the host bloodless and dead.
pub fn leave_host(world: &mut World, worm: Entity) {
    let Some(state) = world.get::<BloodWorm>(worm).cloned() else {
        return;
    };
    let Some(host) = state.host else {
        return;
    };
    possess_worm(world, worm);
    if world.get_entity(host).is_ok() {
        let (user, victim) = (name_of(world, worm), name_of(world, host));
        visible(
            world,
            worm,
            &format!("Вы покидаете {victim}."),
            &format!("{user} выходит из {victim}!"),
            Span::BoldWarning,
        );
    }
    if let Some(mut w) = world.get_mut::<BloodWorm>(worm) {
        w.host = None;
        w.backseat = None;
        w.ejecting = None;
        w.reviving = None;
    }
    if let Some(&pos) = world.get::<TilePos>(host) {
        world.entity_mut(worm).insert(pos);
    }
    crate::closets::let_out(world, worm);
    if let Ok(mut h) = world.get_entity_mut(host) {
        h.remove::<BloodWormHost>();
    }
    if let Some(mut b) = world.get_mut::<Blood>(host) {
        b.volume = 0.0;
    }
    crate::medical::organs::kill(world, host);
    if let Some(ghost) = state.backseat
        && let Some(mut g) = world.get_mut::<Ghost>(ghost)
    {
        g.body = Some(host);
    }
    sound(world, worm, "sound/effects/magic/exit_blood.ogg", 60.0);
    let t = now(world) + state.stage.leave_host_secs();
    world.entity_mut(worm).insert(crate::combat::NextMove(t));
    refresh_buttons(world, worm);
    if let Ok(mut h) = world.get_entity_mut(host) {
        h.remove::<ActionButtons>();
    }
    info!("{worm} (blood worm) left {host}");
}

/// `possess_host`: the worm's player drives the living host.
fn possess_host(world: &mut World, worm: Entity) {
    let Some(host) = world
        .get::<BloodWorm>(worm)
        .filter(|w| !w.possessing)
        .and_then(|w| w.host)
    else {
        return;
    };
    if let Some(mut w) = world.get_mut::<BloodWorm>(worm) {
        w.possessing = true;
    }
    if world.get::<PlayerMob>(worm).is_some() {
        crate::medical::brainmob::transfer_mind(world, worm, host);
    }
    refresh_buttons(world, worm);
}

/// `possess_worm`: the player is back in the worm inside the host.
fn possess_worm(world: &mut World, worm: Entity) {
    let Some(host) = world
        .get::<BloodWorm>(worm)
        .filter(|w| w.possessing)
        .and_then(|w| w.host)
    else {
        return;
    };
    if let Some(mut w) = world.get_mut::<BloodWorm>(worm) {
        w.possessing = false;
    }
    if world.get::<PlayerMob>(host).is_some() {
        crate::medical::brainmob::transfer_mind(world, host, worm);
    }
    refresh_buttons(world, worm);
}

/// `sync_health`: the host's blood capped at the worm's maximum, the worm's health from it, out
/// below the eject threshold.
pub fn sync_health(world: &mut World, worm: Entity) {
    let Some(host) = world.get::<BloodWorm>(worm).and_then(|w| w.host) else {
        return;
    };
    let Some((_, max)) = health_of(world, worm) else {
        return;
    };
    let host_max = max * HEALTH_TO_BLOOD;
    let Some(volume) = world.get_mut::<Blood>(host).map(|mut b| {
        if b.volume > host_max {
            b.volume = host_max;
        }
        b.volume
    }) else {
        leave_host(world, worm);
        return;
    };
    if let Some(mut h) = world.get_mut::<Health>(worm) {
        let target = max * (1.0 - volume / host_max);
        let delta = target - h.brute;
        if delta.abs() > 0.01 {
            h.adjust(DamageType::Brute, delta);
        }
    }
    if volume <= eject_threshold(max) {
        let who = driver(world, worm);
        tell_mob(
            world,
            who,
            Span::BoldWarning,
            "У вас закончилась кровь для управления носителем!",
        );
        leave_host(world, worm);
    }
}

/// Every frame: regeneration (`Life`/`on_host_life`), the host's oxygen damage removed, health
/// synced, the worm kept on the host's tile, control following the host's life.
fn tick(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs();
    let worms: Vec<(Entity, BloodWorm)> = world
        .query::<(Entity, &BloodWorm)>()
        .iter(world)
        .map(|(e, w)| (e, w.clone()))
        .collect();
    for (worm, state) in worms {
        let worm_stat = world.get::<Stat>(worm).copied();
        let Some(host) = state.host else {
            if worm_stat.is_some_and(|s| s != Stat::Dead) {
                adjust_worm_health(world, worm, state.stage.regen_rate() * dt);
            }
            continue;
        };
        if world.get_entity(host).is_err() {
            // `on_host_qdel`.
            world.entity_mut(worm).despawn();
            continue;
        }
        adjust_worm_health(world, worm, state.stage.regen_rate() * dt);
        if let Some(mut h) = world.get_mut::<Health>(host)
            && h.oxy > 0.0
        {
            let oxy = h.oxy;
            h.adjust(DamageType::Oxy, -oxy);
        }
        sync_health(world, worm);
        if world.get::<BloodWorm>(worm).and_then(|w| w.host) != Some(host) {
            continue;
        }
        // `set_stat`: an unconscious or critical worm falls out.
        if world
            .get::<Stat>(worm)
            .is_some_and(|s| *s != Stat::Conscious)
        {
            leave_host(world, worm);
            continue;
        }
        if let Some(&pos) = world.get::<TilePos>(host)
            && world.get::<TilePos>(worm) != Some(&pos)
        {
            world.entity_mut(worm).insert(pos);
        }
        // `on_host_stat_changed`.
        let host_dead = world.get::<Stat>(host) == Some(&Stat::Dead);
        if host_dead && state.possessing {
            possess_worm(world, worm);
        } else if !host_dead && !state.possessing {
            possess_host(world, worm);
        }
    }
}

fn start_eject(world: &mut World, worm: Entity) {
    let Some(state) = world.get::<BloodWorm>(worm).cloned() else {
        return;
    };
    let Some(host) = state.host else {
        return;
    };
    if state.ejecting.is_some() {
        return;
    }
    let victim = name_of(world, host);
    let who = driver(world, worm);
    tell_mob(
        world,
        who,
        Span::Danger,
        "Вы начинаете покидать своего носителя...",
    );
    visible(
        world,
        host,
        &format!("{victim} падает и начинает безудержно трястись!"),
        &format!("{victim} падает и начинает безудержно трястись!"),
        Span::Danger,
    );
    let at = now(world) + EJECT_SECS;
    if let Some(mut w) = world.get_mut::<BloodWorm>(worm) {
        w.ejecting = Some(at);
    }
}

/// `revive/run_checks`: a dead host whose body can live again.
fn revive_check(world: &World, host: Entity) -> Result<(), &'static str> {
    if world.get::<Stat>(host) != Some(&Stat::Dead) {
        return Err("ещё жив!");
    }
    if world
        .get::<Health>(host)
        .is_none_or(|h| h.health() <= crate::health::HEALTH_THRESHOLD_DEAD)
    {
        return Err("тело слишком повреждено!");
    }
    Ok(())
}

fn start_revive(world: &mut World, worm: Entity) {
    let Some(state) = world.get::<BloodWorm>(worm).cloned() else {
        return;
    };
    let Some(host) = state.host else {
        return;
    };
    let t = now(world);
    if state.reviving.is_some() || t < state.revive_ready_at {
        return;
    }
    let who = driver(world, worm);
    if let Err(why) = revive_check(world, host) {
        tell_mob(world, who, Span::Warning, why);
        return;
    }
    let victim = name_of(world, host);
    tell_mob(
        world,
        who,
        Span::Notice,
        format!("Вы начинаете восстанавливать циркуляцию крови {victim}..."),
    );
    if let Some(mut w) = world.get_mut::<BloodWorm>(worm) {
        w.reviving = Some((0, t + REVIVE_STEP_SECS));
    }
}

/// The leave host and revive channels (tg `do_after`s on the host).
fn channels(world: &mut World) {
    let t = now(world);
    let worms: Vec<(Entity, BloodWorm)> = world
        .query::<(Entity, &BloodWorm)>()
        .iter(world)
        .filter(|(_, w)| w.ejecting.is_some() || w.reviving.is_some())
        .map(|(e, w)| (e, w.clone()))
        .collect();
    for (worm, state) in worms {
        let Some(host) = state.host else {
            continue;
        };
        if state.ejecting.is_some_and(|at| t >= at) {
            leave_host(world, worm);
            continue;
        }
        let Some((beats, next)) = state.reviving else {
            continue;
        };
        if t < next {
            continue;
        }
        let who = driver(world, worm);
        if let Err(why) = revive_check(world, host) {
            tell_mob(world, who, Span::Warning, why);
            if let Some(mut w) = world.get_mut::<BloodWorm>(worm) {
                w.reviving = None;
            }
            continue;
        }
        let victim = name_of(world, host);
        sound(world, host, "sound/effects/singlebeat.ogg", 50.0);
        visible(
            world,
            host,
            &format!("{victim} безудержно трясется!"),
            &format!("{victim} безудержно трясется!"),
            Span::Danger,
        );
        if beats + 1 < 3 {
            if let Some(mut w) = world.get_mut::<BloodWorm>(worm) {
                w.reviving = Some((beats + 1, t + REVIVE_STEP_SECS));
            }
            continue;
        }
        if let Some(mut w) = world.get_mut::<BloodWorm>(worm) {
            w.reviving = None;
        }
        if !crate::medical::defib::revive(world, host) {
            tell_mob(world, who, Span::Warning, "не удалось оживить!");
            continue;
        }
        if let Some(mut w) = world.get_mut::<BloodWorm>(worm) {
            w.revive_ready_at = t + REVIVE_COOLDOWN;
        }
        visible(
            world,
            host,
            &format!("Вы успешно оживили {victim}!"),
            &format!("{victim} восстает из мёртвых!"),
            Span::Danger,
        );
        refresh_buttons(world, worm);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eject_threshold_is_a_tenth_of_full_blood() {
        // A hatchling (80 health) holds 448 blood in a host and is pushed out at 44.8.
        assert!((eject_threshold(80.0) - 44.8).abs() < 0.01);
        assert!((80.0 * HEALTH_TO_BLOOD - 448.0).abs() < 0.01);
    }

    #[test]
    fn stages_follow_the_subtypes() {
        assert_eq!(Stage::of_path(HATCHLING), Some(Stage::Hatchling));
        assert_eq!(Stage::of_path(ADULT), Some(Stage::Adult));
        assert_eq!(Stage::Juvenile.leech(), (42.0, 15.0));
    }
}
