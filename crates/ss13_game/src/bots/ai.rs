//! Bot AI, after tg `code/datums/ai/bots/` (patrol, summon, salute subtrees, `bot_search`),
//! `bot_ai.dm` and the kinds' `*_ai.dm` and `*.bt.json`: the trees ([`tree`]), their decorator
//! conditions ([`BotCond`]) and leaves ([`BotLeaf`]), run by `crate::mobs::ai`.
//!
//! - Patrol (`bot_patrol.bt.json`): with `BOT_MODE_AUTOPATROL`, the next beacon in its chain
//!   (`next_patrol`), else the closest one on the level; walk onto it, remember it, repeat;
//!   otherwise wait 5 s.
//! - Unreachable targets go on the temporary ignore list for the kind's
//!   `BB_UNREACHABLE_LIST_COOLDOWN`. Targets are searched in range on the leaves' cadence (tg's
//!   `time_between_perform` and the parallels' repeat delays), never over the whole map.
//! - Cleanbot: decals (and blood, graffiti, trash, pests by its flags) within 5 tiles it can
//!   reach within 15, cleaned in 2 s; 5 s after a cleaning before it patrols again.
//! - Medbot: announces patients in crit on its radio (once per 3 min), tends humans within 7
//!   (1 when stationary) whose damage of its type is over the threshold, 2 s per tend; emagged it
//!   gives chloral hydrate instead; idle speech.
//! - Secbot: hunts humans whose threat (tg `assess_threat`: records, ID check, emag) is over 4,
//!   or who hit it; stuns them (8 s paralysis, 10 s cooldown), then ziptie-cuffs them (4 s).
//! - Firebot: extinguishes burning mobs and fires within 5 tiles; idle speech.
// shortcut: no pet commands, escape captivity, janitor befriending, cleanbot foam or acid,
// medbot tipping, techweb upgrades or path HUD; secbots skip the weapon check (no
// `NEEDS_PERMIT` data) and the nonhuman species and wizard hat terms.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmCleanable, DmItem};
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, Span};
use ss13_world::SpatialIndex;

use super::{
    BOT_CLEANING, BOT_HEALING, BOT_HUNT, BOT_IDLE, BOT_PATROL, BOT_PREP_ARREST, Bot, BotJob,
    BotKind, BotTask, CLEANBOT_CLEAN_BLOOD, CLEANBOT_CLEAN_DRAWINGS, CLEANBOT_CLEAN_PESTS,
    CLEANBOT_CLEAN_TRASH, FIREBOT_EXTINGUISH_FLAMES, FIREBOT_EXTINGUISH_PEOPLE,
    FIREBOT_STATIONARY_MODE, Healer, MEDBOT_STATIONARY_MODE, NavBeacons, SECBOT_CHECK_IDS,
    SECBOT_CHECK_RECORDS, SECBOT_DECLARE_ARRESTS, SECBOT_HANDCUFF_TARGET, bot_name, set_mode,
    speak,
};
use crate::health::{DEFAULT_MESSAGE_RANGE, DamageType, Health, Stat};
use crate::inventory::Inventory;
use crate::mobs::ai::behaviors::{Behavior, FIND_TARGET_RATE, Perform, can_see};
use crate::mobs::ai::bt::{
    Abort, Cond, Node, ParallelOpts, decorator, leaf, parallel, selector, sequence, subplan,
};
use crate::mobs::ai::movement::{Pathing, bot_opens, find_path, pathing_of};
use crate::mobs::ai::trees::Built;
use crate::mobs::ai::{AiController, AiCtx, Key, RUN_WHILE_UNWATCHED, Value};
use crate::security::show;
use crate::status_effects::Incap;

/// tg `BB_UNREACHABLE_LIST_COOLDOWN` of the base controller.
const UNREACHABLE_SECS: f64 = 45.0;
/// `bot_salute_authority.bt.json` cooldown.
const SALUTE_COOLDOWN: f64 = 60.0;
/// `POST_CLEAN_COOLDOWN`.
const POST_CLEAN_COOLDOWN: f64 = 5.0;
/// `announce_patient` `time_between_perform`.
const CRIT_ANNOUNCE_COOLDOWN: f64 = 180.0;
/// Firebot `ANNOUNCEMENT_TIMER`.
const FIRE_ANNOUNCE_COOLDOWN: f64 = 10.0;
/// `THREAT_ASSESS_DANGEROUS`.
const THREAT_ASSESS_DANGEROUS: i32 = 4;
/// `stun_n_cuff`: `stun_timer`, `stun_cooldown_timer`, `handcuff_timer`.
const STUN_SECS: f32 = 8.0;
const STUN_COOLDOWN: f64 = 10.0;
const CUFF_SECS: f64 = 4.0;
const ZIPTIES_USED: &str = "/obj/item/restraints/handcuffs/cable/zipties/used";
const CHLORAL: &str = "/datum/reagent/toxin/chloralhydrate";
/// Candidates a search tries to path to per perform (tg tries them all).
// shortcut: at most 3 reachability searches per perform, nearest first; tg tries every
// candidate. Raise when bots miss targets behind the nearest unreachable ones.
const MAX_REACH_TRIES: usize = 3;

/// tg `bot_mode_flag`, `bot_is_emagged`, `bot_medical_flag` decorators.
#[derive(Clone, Debug, PartialEq)]
pub enum BotCond {
    ModeFlag(u32),
    Emagged,
    MedicalFlag(u32),
}

impl BotCond {
    pub fn check(&self, ctx: &AiCtx) -> bool {
        let Some(bot) = ctx.world.get::<Bot>(ctx.pawn) else {
            return false;
        };
        match *self {
            Self::ModeFlag(flag) => bot.mode_flags & flag != 0,
            Self::Emagged => bot.emagged(),
            Self::MedicalFlag(flag) => bot.medical_flags & flag != 0,
        }
    }
}

/// The bot leaves (tg `/datum/bt_node/ai_behavior/*` of the bot trees).
#[derive(Debug)]
pub enum BotLeaf {
    /// `find_first_beacon_target`.
    FindFirstBeacon,
    /// `find_next_beacon_target` (5 s after a failure).
    FindNextBeacon,
    /// `arrive_at_beacon`.
    ArriveAtBeacon,
    /// `complete_summon_travel`.
    CompleteSummon,
    /// `wait`.
    Wait { secs: f64, waited: bool },
    /// `find_valid_authority` + `salute_authority`.
    Salute { messages: &'static [&'static str] },
    /// `update_interaction_target` with `cleanbot_cleanables` (3 s).
    AcquireCleanable,
    /// `execute_clean`.
    Clean,
    /// `update_interaction_target/medbot_patient`.
    AcquirePatient,
    /// `update_interaction_target` with `crit_patient`.
    AcquireCrit,
    /// `announce_patient`.
    AnnounceCrit,
    /// `tend_to_patient`.
    Tend,
    /// `handle_medbot_speech` (20 s).
    MedbotSpeech,
    /// `update_combat_targets` with `targeting_strategy/basic/secbot`.
    AcquireCriminal,
    /// `basic_melee_attack` through `stun_n_cuff`.
    SecAttack,
    /// The firebot's two `update_interaction_target`s (people, then hotspots).
    AcquireFire,
    /// `announce_fire_detected`.
    AnnounceFire,
    /// `bot_interact/extinguish`.
    Extinguish,
    /// `handle_firebot_speech` (20 s).
    FirebotSpeech,
}

const fn perform(delay: bool, outcome: Option<bool>) -> Perform {
    Perform { delay, outcome }
}
const SUCCEED: Perform = perform(false, Some(true));
const FAIL: Perform = perform(false, Some(false));
const RUNNING: Perform = perform(false, None);
const DELAY_SUCCEED: Perform = perform(true, Some(true));
const DELAY_FAIL: Perform = perform(true, Some(false));
const DELAY_RUNNING: Perform = perform(true, None);

impl BotLeaf {
    pub fn cooldown(&self) -> f64 {
        match self {
            Self::FindNextBeacon => 5.0,
            Self::Wait { secs, .. } => *secs,
            // Failed searches for an authority wait a second (tg looks every tick).
            Self::Salute { .. } => 1.0,
            Self::AcquireCleanable => 3.0,
            Self::MedbotSpeech | Self::FirebotSpeech => 20.0,
            // `melee_attack_cooldown` of bots (`/mob/living/basic` 2 s).
            Self::SecAttack => 2.0,
            _ => 0.0,
        }
    }

    pub fn perform(&mut self, ctx: &mut AiCtx) -> Perform {
        match self {
            Self::FindFirstBeacon => find_first_beacon(ctx),
            Self::FindNextBeacon => find_next_beacon(ctx),
            Self::ArriveAtBeacon => arrive(ctx, Key::BeaconTarget, true),
            Self::CompleteSummon => arrive(ctx, Key::SummonTarget, false),
            Self::Wait { waited, .. } => {
                if *waited {
                    *waited = false;
                    SUCCEED
                } else {
                    *waited = true;
                    DELAY_RUNNING
                }
            }
            Self::Salute { messages } => salute(ctx, messages),
            Self::AcquireCleanable => acquire_cleanable(ctx),
            Self::Clean => clean(ctx),
            Self::AcquirePatient => acquire_patient(ctx),
            Self::AcquireCrit => acquire_crit(ctx),
            Self::AnnounceCrit => announce_crit(ctx),
            Self::Tend => tend(ctx),
            Self::MedbotSpeech => medbot_speech(ctx),
            Self::AcquireCriminal => acquire_criminal(ctx),
            Self::SecAttack => sec_attack(ctx),
            Self::AcquireFire => acquire_fire(ctx),
            Self::AnnounceFire => {
                if ctx
                    .bb
                    .time(Key::AnnounceCooldown)
                    .is_none_or(|t| t <= ctx.now)
                {
                    let line = pick(ctx, FIRE_DETECTED_LINES);
                    speak(ctx.world, ctx.pawn, line, false);
                    ctx.bb.set(
                        Key::AnnounceCooldown,
                        Value::Time(ctx.now + FIRE_ANNOUNCE_COOLDOWN),
                    );
                }
                SUCCEED
            }
            Self::Extinguish => extinguish(ctx),
            Self::FirebotSpeech => {
                // `SPT_PROB(3, seconds_per_tick)`.
                if !ctx.spt_prob(3.0) {
                    return FAIL;
                }
                let emagged = bot(ctx).is_some_and(Bot::emagged);
                let line = pick(
                    ctx,
                    if emagged {
                        FIREBOT_EMAGGED_LINES
                    } else {
                        FIREBOT_IDLE_LINES
                    },
                );
                speak(ctx.world, ctx.pawn, line, false);
                DELAY_SUCCEED
            }
        }
    }

    pub fn finish(&mut self, ctx: &mut AiCtx, succeeded: bool) {
        match self {
            Self::Wait { waited, .. } => *waited = false,
            Self::Clean => {
                ctx.bb.set(
                    Key::PostCleanCooldown,
                    Value::Time(ctx.now + POST_CLEAN_COOLDOWN),
                );
                let target = ctx.bb.entity(Key::CurrentTarget);
                ctx.bb.clear(Key::CurrentTarget);
                if !succeeded && let Some(t) = target {
                    blacklist(ctx, t);
                }
            }
            Self::AnnounceCrit => ctx.bb.clear(Key::PatientInCrit),
            Self::Tend => {
                let target = ctx.bb.entity(Key::CurrentTarget);
                let stationary =
                    bot(ctx).is_some_and(|b| b.medical_flags & MEDBOT_STATIONARY_MODE != 0);
                if !succeeded {
                    if let Some(t) = target
                        && !stationary
                    {
                        blacklist(ctx, t);
                    }
                    ctx.bb.clear(Key::CurrentTarget);
                    return;
                }
                if target.is_some_and(|t| healed(ctx.world, ctx.pawn, t)) {
                    let line = pick(ctx, AFTERHEAL_LINES);
                    speak(ctx.world, ctx.pawn, line, false);
                    ctx.bb.clear(Key::CurrentTarget);
                }
            }
            Self::Extinguish => {
                let target = ctx.bb.get(Key::CurrentTarget).cloned();
                ctx.bb.clear(Key::CurrentTarget);
                let emagged = bot(ctx).is_some_and(Bot::emagged);
                if let Some(Value::Entity(t)) = target
                    && (!succeeded || emagged)
                {
                    blacklist(ctx, t);
                }
            }
            _ => {}
        }
    }
}

fn bot<'a>(ctx: &'a AiCtx) -> Option<&'a Bot> {
    ctx.world.get::<Bot>(ctx.pawn)
}

fn pick(ctx: &mut AiCtx, lines: &'static [&'static str]) -> &'static str {
    if lines.is_empty() {
        return "";
    }
    lines[ctx.rng.range(0, lines.len() as u32 - 1) as usize]
}

fn dist(ctx: &AiCtx, pos: TilePos) -> Option<u32> {
    ctx.pawn_pos()?.chebyshev(pos)
}

/// The kind's `BB_UNREACHABLE_LIST_COOLDOWN`.
fn unreachable_secs(kind: BotKind) -> f64 {
    match kind {
        BotKind::Clean | BotKind::Fire => 180.0,
        BotKind::Sec => 60.0,
        _ => UNREACHABLE_SECS,
    }
}

/// `LAZYACCESS(BB_TEMPORARY_IGNORE_LIST, target)`.
fn ignored(ctx: &AiCtx, target: Entity) -> bool {
    matches!(ctx.bb.get(Key::IgnoreList), Some(Value::Attackers(list))
        if list.iter().any(|&(e, until)| e == target && until > ctx.now))
}

/// `add_to_blacklist`: ignored for the kind's unreachable cooldown.
fn blacklist(ctx: &mut AiCtx, target: Entity) {
    let secs = bot(ctx).map_or(UNREACHABLE_SECS, |b| unreachable_secs(b.kind));
    let now = ctx.now;
    let mut list = match ctx.bb.get(Key::IgnoreList) {
        Some(Value::Attackers(list)) => list.clone(),
        _ => Vec::new(),
    };
    list.retain(|&(e, until)| e != target && until > now);
    list.push((target, now + secs));
    ctx.bb.set(Key::IgnoreList, Value::Attackers(list));
}

/// `can_reach_target(target, distance)`: a path within `max` steps that ends next to the
/// target (on it with `min_dist` 0).
fn reachable(ctx: &AiCtx, to: TilePos, min_dist: u32, max: usize) -> bool {
    let Some(from) = ctx.pawn_pos() else {
        return false;
    };
    if from.chebyshev(to).is_some_and(|d| d <= min_dist) {
        return true;
    }
    let world = &*ctx.world;
    let pawn = ctx.pawn;
    let opens = |e: Entity| bot_opens(world, pawn, e);
    let base = pathing_of(world, pawn, &opens);
    let pathing = Pathing {
        opens: base.opens,
        max_path: max,
        max_nodes: base.max_nodes,
    };
    find_path(
        world.resource::<SpatialIndex>(),
        pawn,
        from,
        to,
        min_dist,
        &pathing,
    )
    .is_some()
}

/// Of `candidates` (nearest first), the first one reachable within `max` steps; unreachable
/// ones tried on the way are blacklisted unless `keep`.
fn first_reachable(
    ctx: &mut AiCtx,
    candidates: Vec<(Entity, TilePos)>,
    min_dist: u32,
    max: usize,
    blacklist_unreachable: bool,
) -> Option<Entity> {
    for (e, pos) in candidates.into_iter().take(MAX_REACH_TRIES) {
        if reachable(ctx, pos, min_dist, max) {
            return Some(e);
        }
        if blacklist_unreachable {
            blacklist(ctx, e);
        }
    }
    None
}

/// Things within `range` of the bot it sees, nearest first, that pass `valid` and are not on
/// its ignore list.
fn search(
    ctx: &AiCtx,
    range: u32,
    valid: impl Fn(&World, Entity) -> bool,
) -> Vec<(Entity, TilePos)> {
    let Some(from) = ctx.pawn_pos() else {
        return Vec::new();
    };
    let world = &*ctx.world;
    let mut found: Vec<(u32, Entity, TilePos)> = world
        .resource::<SpatialIndex>()
        .in_radius(from, range)
        .filter(|&e| e != ctx.pawn && !ignored(ctx, e) && valid(world, e))
        .filter_map(|e| {
            let pos = *world.get::<TilePos>(e)?;
            let d = from.chebyshev(pos)?;
            can_see(world, from, pos, range).then_some((d, e, pos))
        })
        .collect();
    found.sort_by_key(|&(d, e, _)| (d, e.to_bits()));
    found.into_iter().map(|(_, e, p)| (e, p)).collect()
}

fn find_first_beacon(ctx: &mut AiCtx) -> Perform {
    let Some(from) = ctx.pawn_pos() else {
        return DELAY_FAIL;
    };
    let previous = ctx.bb.entity(Key::PreviousBeacon);
    let best = ctx
        .world
        .resource::<NavBeacons>()
        .patrol_on(from.z)
        .filter(|b| Some(b.entity) != previous)
        .filter_map(|b| Some((from.chebyshev(b.pos)?, b.entity)))
        .filter(|&(d, _)| d > 1)
        .min_by_key(|&(d, e)| (d, e.to_bits()));
    match best {
        Some((_, beacon)) => {
            ctx.bb.set(Key::BeaconTarget, Value::Entity(beacon));
            set_mode(ctx.world, ctx.pawn, BOT_PATROL);
            DELAY_SUCCEED
        }
        None => DELAY_FAIL,
    }
}

fn find_next_beacon(ctx: &mut AiCtx) -> Perform {
    let Some(from) = ctx.pawn_pos() else {
        return DELAY_FAIL;
    };
    let beacons = ctx.world.resource::<NavBeacons>();
    let next = ctx
        .bb
        .entity(Key::PreviousBeacon)
        .and_then(|prev| beacons.0.get(&prev))
        .and_then(|prev| prev.next.clone());
    let Some(next) = next else {
        ctx.bb.clear(Key::PreviousBeacon);
        return DELAY_FAIL;
    };
    let target = beacons
        .patrol_on(from.z)
        .find(|b| b.location == next)
        .map(|b| b.entity);
    match target {
        Some(beacon) => {
            ctx.bb.set(Key::PreviousBeacon, Value::Entity(beacon));
            ctx.bb.set(Key::BeaconTarget, Value::Entity(beacon));
            set_mode(ctx.world, ctx.pawn, BOT_PATROL);
            SUCCEED
        }
        None => {
            ctx.bb.clear(Key::PreviousBeacon);
            DELAY_FAIL
        }
    }
}

/// `arrive_at_beacon` / `complete_summon_travel`.
fn arrive(ctx: &mut AiCtx, key: Key, beacon: bool) -> Perform {
    let Some(target) = ctx.target_pos(key) else {
        return FAIL;
    };
    if dist(ctx, target) != Some(0) {
        return RUNNING;
    }
    if beacon {
        if let Some(e) = ctx.bb.entity(key) {
            ctx.bb.set(Key::PreviousBeacon, Value::Entity(e));
        }
    } else {
        set_mode(ctx.world, ctx.pawn, BOT_IDLE);
    }
    ctx.bb.clear(key);
    SUCCEED
}

fn salute(ctx: &mut AiCtx, messages: &'static [&'static str]) -> Perform {
    let Some(from) = ctx.pawn_pos() else {
        return DELAY_FAIL;
    };
    let world = &*ctx.world;
    let authority = world
        .resource::<SpatialIndex>()
        .in_radius(from, 7)
        .filter(|&e| e != ctx.pawn)
        .filter(|&e| world.get::<Bot>(e).is_some_and(|b| b.commissioned))
        .find(|&e| {
            world
                .get::<TilePos>(e)
                .is_some_and(|&p| can_see(world, from, p, 7))
        });
    let Some(authority) = authority else {
        return DELAY_FAIL;
    };
    let name = bot_name(ctx.world, authority);
    let text = format!("{} {name}!", pick(ctx, messages));
    crate::mobs::defense::emote(ctx.world, ctx.pawn, &text);
    ctx.bb
        .set(Key::SaluteCooldown, Value::Time(ctx.now + SALUTE_COOLDOWN));
    DELAY_SUCCEED
}

/// `BB_SALUTE_MESSAGES` of the base and cleanbot controllers.
pub const BOT_SALUTES: &[&str] = &[
    "performs an elaborate salute for",
    "nods in appreciation towards",
];
pub const CLEANBOT_SALUTES: &[&str] = &[
    "salutes",
    "nods in appreciation towards",
    "mops the dirt away in the path of",
];

// Cleanbot.

/// `cleanable_decals`.
const CLEANABLE_DECALS: [&str; 17] = [
    "/obj/effect/decal/cleanable/ants",
    "/obj/effect/decal/cleanable/ash",
    "/obj/effect/decal/cleanable/confetti",
    "/obj/effect/decal/cleanable/dirt",
    "/obj/effect/decal/cleanable/fuel_pool",
    "/obj/effect/decal/cleanable/generic",
    "/obj/effect/decal/cleanable/glitter",
    "/obj/effect/decal/cleanable/greenglow",
    "/obj/effect/decal/cleanable/insectguts",
    "/obj/effect/decal/cleanable/molten_object",
    "/obj/effect/decal/cleanable/blood/oil",
    "/obj/effect/decal/cleanable/food",
    "/obj/effect/decal/cleanable/blood/gibs/robot_debris",
    "/obj/effect/decal/cleanable/shreds",
    "/obj/effect/decal/cleanable/glass",
    "/obj/effect/decal/cleanable/vomit",
    "/obj/effect/decal/cleanable/wrapping",
];
const CLEANABLE_BLOOD: [&str; 2] = [
    "/obj/effect/decal/cleanable/blood/xeno",
    "/obj/effect/decal/cleanable/blood",
];
const CLEANABLE_DRAWINGS: [&str; 1] = ["/obj/effect/decal/cleanable/crayon"];
const HUNTABLE_TRASH: [&str; 4] = [
    "/obj/item/trash",
    "/obj/item/food/deadmouse",
    "/obj/effect/decal/remains",
    "/obj/item/cigbutt",
];
const HUNTABLE_PESTS: [&str; 2] = ["/mob/living/basic/cockroach", "/mob/living/basic/mouse"];

/// What a cleanbot with `flags` hunts (`cleanbot_cleanables`).
fn cleanable(world: &World, e: Entity, flags: u32) -> bool {
    let under_any = |path: &str, roots: &[&str]| roots.iter().any(|r| is_under(path, r));
    if let Some(c) = world.get::<DmCleanable>(e) {
        return under_any(&c.path, &CLEANABLE_DECALS)
            || (flags & CLEANBOT_CLEAN_BLOOD != 0 && under_any(&c.path, &CLEANABLE_BLOOD))
            || (flags & CLEANBOT_CLEAN_DRAWINGS != 0 && under_any(&c.path, &CLEANABLE_DRAWINGS))
            || (flags & CLEANBOT_CLEAN_TRASH != 0 && under_any(&c.path, &HUNTABLE_TRASH));
    }
    if flags & CLEANBOT_CLEAN_TRASH != 0
        && world.get::<crate::inventory::CarriedBy>(e).is_none()
        && world
            .get::<DmItem>(e)
            .is_some_and(|i| under_any(&i.path, &HUNTABLE_TRASH))
    {
        return true;
    }
    flags & CLEANBOT_CLEAN_PESTS != 0
        && world.get::<Stat>(e).is_some_and(|s| *s != Stat::Dead)
        && world
            .get::<crate::mobs::BasicMob>(e)
            .is_some_and(|b| under_any(&b.def.path, &HUNTABLE_PESTS))
}

fn acquire_cleanable(ctx: &mut AiCtx) -> Perform {
    if let Some(t) = ctx.bb.entity(Key::CurrentTarget)
        && ctx.world.get_entity(t).is_ok()
        && !ignored(ctx, t)
    {
        return DELAY_SUCCEED;
    }
    let flags = bot(ctx).map_or(0, |b| b.janitor_flags);
    let found = search(ctx, 5, |w, e| cleanable(w, e, flags));
    match first_reachable(ctx, found, 1, 15, true) {
        Some(t) => {
            ctx.bb.set(Key::CurrentTarget, Value::Entity(t));
            DELAY_SUCCEED
        }
        None => {
            ctx.bb.clear(Key::CurrentTarget);
            DELAY_FAIL
        }
    }
}

fn clean(ctx: &mut AiCtx) -> Perform {
    let Some(target) = ctx
        .bb
        .entity(Key::CurrentTarget)
        .filter(|&t| ctx.world.get_entity(t).is_ok())
    else {
        return DELAY_FAIL;
    };
    let Some(pos) = ctx.world.get::<TilePos>(target).copied() else {
        return DELAY_FAIL;
    };
    if dist(ctx, pos).is_none_or(|d| d > 1) {
        return FAIL;
    }
    // `UnarmedAttack` -> the cleaner component (`pre_clean_callback`: cleaning mode).
    if super::start_task(ctx.world, ctx.pawn, BotJob::Clean, target, 2.0) {
        set_mode(ctx.world, ctx.pawn, BOT_CLEANING);
    }
    DELAY_SUCCEED
}

// Medbot.

/// `wait_announcements`, `afterheal_announcements`, `near_death_announcements`, `idle_lines`,
/// `emagged_announcements` (BandaStation's `MEDIBOT_VOICED_*`).
const WAIT_LINES: &[&str] = &[
    "Эй! Подожди, я иду.",
    "Подожите! Я хочу помочь!",
    "Вы, кажется, были ранены!",
];
const AFTERHEAL_LINES: &[&str] = &[
    "Как новенький!",
    "Яблочко на ужин, и врач не нужен!",
    "Поправляйтесь!",
];
const NEAR_DEATH_LINES: &[&str] = &[
    "Нет! Не бросай меня!",
    "Живи, черт возьми, ЖИВИ!",
    "Я... я никогда раньше не терял пациентов. Сегодня, то есть.",
];
const MEDBOT_IDLE_LINES: &[&str] = &[
    "Восхитительно!",
    "Я так и знал! Нужно было учиться на пластического хирурга.",
    "Радар, надень маску!",
    "Всегда есть подвох, но со мной тебе нечего бояться!",
    "Что это за медицинский отдел такой? Все мрут как мухи!",
    "Почему мы все еще здесь? Просто страдать?",
];
const MEDBOT_EMAGGED_LINES: &[&str] = &[
    "Пошел нахуй.",
    "Выключи компьютер. Это не игра.",
    "Я не такой!",
    "Закрой Dreamseeker.exe немедленно. Или пожалеешь.",
    "Shindemashou.",
];

/// A human (carbon) mob: an inventory and a carbon health.
fn is_human(world: &World, e: Entity) -> bool {
    world.get::<Inventory>(e).is_some() && world.get::<Health>(e).is_some_and(|h| h.basic.is_none())
}

/// `treatable_patient`.
fn treatable(world: &World, medbot: &Bot, e: Entity) -> bool {
    if !is_human(world, e) {
        return false;
    }
    let (Some(stat), Some(health)) = (world.get::<Stat>(e), world.get::<Health>(e)) else {
        return false;
    };
    if *stat == Stat::Dead {
        return false;
    }
    if medbot.emagged() && *stat == Stat::Conscious {
        return true;
    }
    medbot.healer.damage(health) > medbot.heal_threshold
}

/// `tend_to_patient/check_if_healed`.
fn healed(world: &World, medbot: Entity, patient: Entity) -> bool {
    let (Some(bot), Some(health)) = (world.get::<Bot>(medbot), world.get::<Health>(patient)) else {
        return true;
    };
    if bot.emagged() {
        return world.get::<Stat>(patient) != Some(&Stat::Conscious);
    }
    bot.healer.damage(health) <= bot.heal_threshold
}

fn acquire_patient(ctx: &mut AiCtx) -> Perform {
    let Some(medbot) = bot(ctx).cloned() else {
        return FAIL;
    };
    if let Some(t) = ctx.bb.entity(Key::CurrentTarget)
        && ctx.world.get_entity(t).is_ok()
        && !ignored(ctx, t)
        && treatable(ctx.world, &medbot, t)
    {
        return SUCCEED;
    }
    let stationary = medbot.medical_flags & MEDBOT_STATIONARY_MODE != 0;
    let range = if stationary { 1 } else { 7 };
    let found = search(ctx, range, |w, e| treatable(w, &medbot, e));
    match first_reachable(ctx, found, 1, 20, !stationary) {
        Some(t) => {
            ctx.bb.set(Key::CurrentTarget, Value::Entity(t));
            // `on_target_found`: announce unless already next to it.
            if ctx
                .world
                .get::<TilePos>(t)
                .copied()
                .and_then(|p| dist(ctx, p))
                .is_some_and(|d| d > 1)
            {
                let line = pick(ctx, WAIT_LINES);
                speak(ctx.world, ctx.pawn, line, false);
            }
            SUCCEED
        }
        None => {
            ctx.bb.clear(Key::CurrentTarget);
            FAIL
        }
    }
}

/// `crit_patient`: a human at least in hard crit, alive, with a player.
fn in_crit(world: &World, e: Entity) -> bool {
    is_human(world, e)
        && world.get::<crate::server::PlayerMob>(e).is_some()
        && world.get::<Stat>(e) == Some(&Stat::HardCrit)
}

fn acquire_crit(ctx: &mut AiCtx) -> Perform {
    let found = search(ctx, 7, in_crit);
    match found.first() {
        Some(&(t, _)) => {
            ctx.bb.set(Key::PatientInCrit, Value::Entity(t));
            SUCCEED
        }
        None => {
            ctx.bb.clear(Key::PatientInCrit);
            FAIL
        }
    }
}

/// `announce_patient`, once per [`CRIT_ANNOUNCE_COOLDOWN`]; meanwhile it fails, so the medbot
/// goes on treating (tg's leaf stays running for its 3 min cooldown while a patient is in crit,
/// which freezes the medbot).
fn announce_crit(ctx: &mut AiCtx) -> Perform {
    let Some(patient) = ctx
        .bb
        .entity(Key::PatientInCrit)
        .filter(|&t| ctx.world.get_entity(t).is_ok())
    else {
        return FAIL;
    };
    if ctx
        .bb
        .time(Key::AnnounceCooldown)
        .is_some_and(|t| t > ctx.now)
    {
        return FAIL;
    }
    let name = bot_name(ctx.world, patient);
    let area = ctx
        .world
        .get::<TilePos>(patient)
        .map(|&p| super::area_name(ctx.world, p))
        .unwrap_or_default();
    let text = format!("Medical emergency! {name} is in critical condition at {area}!");
    speak(ctx.world, ctx.pawn, &text, true);
    ctx.bb.set(
        Key::AnnounceCooldown,
        Value::Time(ctx.now + CRIT_ANNOUNCE_COOLDOWN),
    );
    SUCCEED
}

fn tend(ctx: &mut AiCtx) -> Perform {
    let Some(patient) = ctx
        .bb
        .entity(Key::CurrentTarget)
        .filter(|&t| ctx.world.get_entity(t).is_ok())
    else {
        return DELAY_FAIL;
    };
    if ctx
        .world
        .get::<Stat>(patient)
        .is_none_or(|s| *s == Stat::Dead)
    {
        return DELAY_FAIL;
    }
    let Some(pos) = ctx.world.get::<TilePos>(patient).copied() else {
        return DELAY_FAIL;
    };
    if dist(ctx, pos).is_none_or(|d| d > 1) {
        // "We technically failed, but we want to try again so succeed."
        return SUCCEED;
    }
    if healed(ctx.world, ctx.pawn, patient) {
        return DELAY_SUCCEED;
    }
    if ctx.world.get::<Stat>(patient) == Some(&Stat::HardCrit) && ctx.prob(5.0) {
        let line = pick(ctx, NEAR_DEATH_LINES);
        speak(ctx.world, ctx.pawn, line, false);
    }
    // `melee_attack` -> `medicate_patient`: a 2 s `do_after`, once at a time.
    if super::start_task(ctx.world, ctx.pawn, BotJob::Tend, patient, 2.0) {
        set_mode(ctx.world, ctx.pawn, BOT_HEALING);
        let (name, patient_name) = (bot_name(ctx.world, ctx.pawn), bot_name(ctx.world, patient));
        show(
            ctx.world,
            pos,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Info,
            (
                Span::Plain,
                format!("{name} is trying to tend the wounds of {patient_name}"),
            ),
            &[(
                patient,
                Span::BoldWarning,
                format!("{name} is trying to tend your wounds!"),
            )],
        );
    }
    DELAY_SUCCEED
}

/// `medicate_patient` after its `do_after`.
fn medicate(world: &mut World, medbot: Entity, patient: Entity) {
    let Some(bot) = world.get::<Bot>(medbot).cloned() else {
        return;
    };
    set_mode(world, medbot, BOT_IDLE);
    if !bot.emagged()
        && world
            .get::<Health>(patient)
            .is_none_or(|h| bot.healer.damage(h) <= bot.heal_threshold)
    {
        return;
    }
    if bot.emagged() {
        let Some(chem) = world
            .get_resource::<crate::chem::Chem>()
            .map(|c| c.0.clone())
        else {
            return;
        };
        if let Some(id) = chem.id(CHLORAL)
            && let Some(mut reagents) = world.get_mut::<crate::chem::MobReagents>(patient)
        {
            reagents.blood.add_reagent(
                &chem,
                id,
                5.0,
                crate::breath::BODYTEMP_NORMAL as f32,
                None,
                None,
                false,
                false,
            );
        }
    } else {
        heal(world, patient, bot.healer, bot.heal_amount);
    }
    let (name, patient_name) = (bot_name(world, medbot), bot_name(world, patient));
    if let Some(&pos) = world.get::<TilePos>(patient) {
        show(
            world,
            pos,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Info,
            (
                Span::Notice,
                format!("{name} tends the wounds of {patient_name}!"),
            ),
            &[(
                patient,
                Span::GreenText,
                format!("{name} tends your wounds!"),
            )],
        );
    }
}

/// `heal_damage_type` / `heal_ordered_damage(brute, burn, tox, oxy)`, then `update_stat`.
pub fn heal(world: &mut World, patient: Entity, healer: Healer, amount: f32) {
    let Some(mut health) = world.get::<Health>(patient).cloned() else {
        return;
    };
    match healer {
        Healer::Type(kind) => health.adjust(kind, -amount),
        Healer::All => {
            let mut left = amount;
            for kind in [
                DamageType::Brute,
                DamageType::Burn,
                DamageType::Tox,
                DamageType::Oxy,
            ] {
                let had = Healer::Type(kind).damage(&health);
                let healed = had.min(left);
                if healed > 0.0 {
                    health.adjust(kind, -healed);
                    left -= healed;
                }
            }
        }
    }
    set_health(world, patient, health);
}

/// The new health and, when it changed, the stat (tg `updatehealth` -> `update_stat`).
fn set_health(world: &mut World, mob: Entity, health: Health) {
    let mut stat = world.get::<Stat>(mob).copied().unwrap_or_default();
    let changed = crate::health::update_stat(&health, &mut stat).is_some();
    let mut e = world.entity_mut(mob);
    e.insert(health);
    if changed {
        e.insert(stat);
    }
}

fn medbot_speech(ctx: &mut AiCtx) -> Perform {
    let Some(medbot) = bot(ctx).cloned() else {
        return FAIL;
    };
    let chance = if medbot.emagged() { 15.0 } else { 5.0 };
    if !ctx.spt_prob(chance) {
        return FAIL;
    }
    let lines = if medbot.emagged() {
        MEDBOT_EMAGGED_LINES
    } else if medbot.mode == BOT_IDLE {
        MEDBOT_IDLE_LINES
    } else {
        return DELAY_FAIL;
    };
    let line = pick(ctx, lines);
    speak(ctx.world, ctx.pawn, line, false);
    DELAY_SUCCEED
}

// Secbot.

/// tg `/mob/living/carbon/human/assess_threat` with the secbot's `judgement_criteria`.
pub fn assess_threat(world: &World, secbot: &Bot, target: Entity) -> i32 {
    if secbot.emagged() {
        return 10;
    }
    let mut threat = 0;
    let name = world
        .get::<Name>(target)
        .map(|n| n.as_str().to_owned())
        .unwrap_or_default();
    let has_id = world
        .get::<Inventory>(target)
        .and_then(|i| i.id_card(world))
        .is_some();
    if secbot.security_flags & SECBOT_CHECK_IDS != 0 && !has_id && name == "Unknown" {
        threat += 4;
    }
    if secbot.security_flags & SECBOT_CHECK_RECORDS != 0
        && let Some(record) = world
            .get_resource::<crate::records::manifest::Manifest>()
            .and_then(|m| m.find(&name))
    {
        use crate::records::{WANTED_ARREST, WANTED_PAROLE, WANTED_PRISONER, WANTED_SUSPECT};
        threat += match record.wanted_status.as_str() {
            WANTED_ARREST => 5,
            WANTED_PRISONER | WANTED_SUSPECT | WANTED_PAROLE => 2,
            _ => 0,
        };
    }
    if crate::antag::revolution::server::mindshielded(world, target) {
        threat -= 1;
    }
    threat
}

fn paralyzed(world: &World, e: Entity) -> bool {
    world
        .get::<crate::status_effects::StatusEffects>(e)
        .is_some_and(|s| s.remaining(Incap::Paralyze) > 0.0)
}

fn retaliating_against(ctx: &AiCtx, e: Entity) -> bool {
    matches!(ctx.bb.get(Key::RetaliateList), Some(Value::Attackers(list)) if list.iter().any(|&(a, _)| a == e))
}

/// `targeting_strategy/basic/secbot`: the threat, or `None` when not a target.
fn criminal_threat(ctx: &AiCtx, secbot: &Bot, e: Entity) -> Option<i32> {
    if !is_human(ctx.world, e) || ignored(ctx, e) {
        return None;
    }
    if ctx.world.get::<crate::restraints::Handcuffed>(e).is_some()
        || ctx.world.get::<Stat>(e) != Some(&Stat::Conscious)
    {
        return None;
    }
    if retaliating_against(ctx, e) {
        return Some(secbot.threat.max(5));
    }
    if paralyzed(ctx.world, e) && secbot.security_flags & SECBOT_HANDCUFF_TARGET == 0 {
        return None;
    }
    let threat = assess_threat(ctx.world, secbot, e);
    (threat > THREAT_ASSESS_DANGEROUS).then_some(threat)
}

/// `update_combat_targets` (`vision_range` 9, kept up to 16 tiles while valid).
fn acquire_criminal(ctx: &mut AiCtx) -> Perform {
    let Some(secbot) = bot(ctx).cloned() else {
        return FAIL;
    };
    if let Some(t) = ctx.bb.entity(Key::CurrentTarget)
        && ctx.world.get_entity(t).is_ok()
        && let Some(p) = ctx.world.get::<TilePos>(t).copied()
        && dist(ctx, p).is_some_and(|d| d <= 16)
        && criminal_threat(ctx, &secbot, t).is_some()
    {
        return SUCCEED;
    }
    let found = search(ctx, 9, |_, _| true);
    let target = found
        .into_iter()
        .find_map(|(e, _)| criminal_threat(ctx, &secbot, e).map(|t| (e, t)));
    let Some((target, threat)) = target else {
        if ctx.bb.entity(Key::CurrentTarget).is_some() {
            ctx.bb.clear(Key::CurrentTarget);
            set_mode(ctx.world, ctx.pawn, BOT_IDLE);
        }
        return FAIL;
    };
    ctx.bb.set(Key::CurrentTarget, Value::Entity(target));
    if let Some(mut b) = ctx.world.get_mut::<Bot>(ctx.pawn) {
        b.threat = threat;
    }
    // `on_target_set`.
    speak(
        ctx.world,
        ctx.pawn,
        &format!("Level {threat} infraction alert!"),
        false,
    );
    set_mode(ctx.world, ctx.pawn, BOT_HUNT);
    SUCCEED
}

/// `basic_melee_attack` -> `stun_n_cuff/on_unarmed_attack`: cuffs a paralyzed target, stuns
/// the others.
fn sec_attack(ctx: &mut AiCtx) -> Perform {
    let Some(target) = ctx
        .bb
        .entity(Key::CurrentTarget)
        .filter(|&t| ctx.world.get_entity(t).is_ok())
    else {
        return FAIL;
    };
    let Some(pos) = ctx.world.get::<TilePos>(target).copied() else {
        return FAIL;
    };
    if dist(ctx, pos).is_none_or(|d| d > 1) || !is_human(ctx.world, target) {
        return DELAY_FAIL;
    }
    if paralyzed(ctx.world, target) {
        if ctx
            .world
            .get::<crate::restraints::Handcuffed>(target)
            .is_some()
        {
            return DELAY_FAIL;
        }
        if super::start_task(ctx.world, ctx.pawn, BotJob::Cuff, target, CUFF_SECS) {
            let (name, victim) = (bot_name(ctx.world, ctx.pawn), bot_name(ctx.world, target));
            set_mode(ctx.world, ctx.pawn, super::BOT_ARREST);
            show(
                ctx.world,
                pos,
                DEFAULT_MESSAGE_RANGE,
                ChatChannel::Combat,
                (
                    Span::Danger,
                    format!("{name} is trying to put zipties on {victim}!"),
                ),
                &[(
                    target,
                    Span::Danger,
                    format!("{name} is trying to put zipties on you!"),
                )],
            );
        }
        return DELAY_SUCCEED;
    }
    stun(ctx.world, ctx.pawn, target, ctx.now);
    DELAY_SUCCEED
}

/// `stun_n_cuff/stun_target` and the secbot's `post_stun`.
pub fn stun(world: &mut World, secbot: Entity, target: Entity, now: f64) {
    let Some(bot) = world.get::<Bot>(secbot).cloned() else {
        return;
    };
    if now < bot.stun_ready_at {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(target) else {
        return;
    };
    crate::status_effects::apply(world, target, Incap::Paralyze, STUN_SECS);
    let (name, victim) = (bot_name(world, secbot), bot_name(world, target));
    show(
        world,
        pos,
        DEFAULT_MESSAGE_RANGE,
        ChatChannel::Combat,
        (Span::Danger, format!("{name} stuns {victim}!")),
        &[(target, Span::BoldWarning, format!("{name} stuns you!"))],
    );
    if let Some(mut b) = world.get_mut::<Bot>(secbot) {
        b.stun_ready_at = now + STUN_COOLDOWN;
    }
    if bot.security_flags & SECBOT_DECLARE_ARRESTS != 0 {
        let verb = if bot.security_flags & SECBOT_HANDCUFF_TARGET != 0 {
            "Arresting"
        } else {
            "Detaining"
        };
        let threat = if bot.threat > 0 { bot.threat } else { 5 };
        let area = super::area_name(world, pos);
        speak(
            world,
            secbot,
            &format!("{verb} level {threat} scumbag {victim} in {area}."),
            true,
        );
    }
    set_mode(world, secbot, BOT_PREP_ARREST);
}

/// `stun_n_cuff/cuff_target` after its `do_after`: new used zipties onto the target.
fn cuff(world: &mut World, secbot: Entity, target: Entity) {
    set_mode(world, secbot, BOT_IDLE);
    if world.get::<crate::restraints::Handcuffed>(target).is_some() {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(target) else {
        return;
    };
    let Some(cuffs) = crate::construction::spawn_obj(world, ZIPTIES_USED, pos) else {
        return;
    };
    crate::restraints::apply_cuffs(world, cuffs, target);
}

// Firebot.

/// `FIREBOT_VOICED_*` lines (BandaStation's translations).
const FIRE_DETECTED_LINES: &[&str] = &[
    "Обнаружен пожар!",
    "Остановитесь, упадите и катайтесь на месте!",
    "Тушение!",
];
const FIREBOT_IDLE_LINES: &[&str] = &[
    "Пожаров не обнаружено.",
    "Только вы можете предотвратить пожары на станции.",
    "Температура в норме.",
    "Не парьтесь.",
];
const FIREBOT_EMAGGED_LINES: &[&str] = &[
    "Держите свечи рядом с занавесками для уютного ночного освещения!",
    "Держите полные ведра воды рядом с розетками на случай пожара от короткого замыкания!",
    "Подливая топливо в огонь, вы ускоряете его горение!",
];

fn acquire_fire(ctx: &mut AiCtx) -> Perform {
    let Some(firebot) = bot(ctx).cloned() else {
        return FAIL;
    };
    let stationary = firebot.firebot_flags & FIREBOT_STATIONARY_MODE != 0;
    let range = if stationary { 1 } else { 5 };
    if firebot.firebot_flags & FIREBOT_EXTINGUISH_PEOPLE != 0 {
        let emagged = firebot.emagged();
        let found = search(ctx, range, |w, e| {
            w.get::<Stat>(e).is_some()
                && w.get::<Health>(e).is_some_and(|h| h.basic.is_none())
                && (emagged || crate::fire::on_fire(w, e))
        });
        if let Some(t) = first_reachable(ctx, found, 1, 30, !stationary) {
            ctx.bb.set(Key::CurrentTarget, Value::Entity(t));
            return SUCCEED;
        }
    }
    if firebot.firebot_flags & FIREBOT_EXTINGUISH_FLAMES != 0
        && let Some(from) = ctx.pawn_pos()
    {
        let r = range as i32;
        let mut fires: Vec<(u32, TilePos)> = Vec::new();
        if let Some(atmos) = ctx.world.get_resource::<ss13_atmos::Atmos>() {
            for dy in -r..=r {
                for dx in -r..=r {
                    let p = TilePos::new(from.x + dx, from.y + dy, from.z);
                    if atmos.hotspot(p).is_some() {
                        fires.push((from.chebyshev(p).unwrap_or(u32::MAX), p));
                    }
                }
            }
        }
        fires.sort_by_key(|&(d, p)| (d, p.x, p.y));
        for (_, p) in fires.into_iter().take(MAX_REACH_TRIES) {
            if reachable(ctx, p, 1, 30) {
                ctx.bb.set(Key::CurrentTarget, Value::Tile(p));
                return SUCCEED;
            }
        }
    }
    FAIL
}

fn extinguish(ctx: &mut AiCtx) -> Perform {
    let target = ctx.bb.get(Key::CurrentTarget).cloned();
    let pos = match target {
        Some(Value::Entity(e)) if ctx.world.get_entity(e).is_ok() => {
            ctx.world.get::<TilePos>(e).copied()
        }
        Some(Value::Tile(p)) => Some(p),
        _ => None,
    };
    let Some(pos) = pos else {
        return DELAY_FAIL;
    };
    if dist(ctx, pos).is_none_or(|d| d > 1) {
        return RUNNING;
    }
    let emagged = bot(ctx).is_some_and(Bot::emagged);
    match target {
        // The extinguisher's water (`/datum/reagent/water/expose_mob`), or the emagged
        // firebot's chlorine trifluoride.
        Some(Value::Entity(e)) => {
            if emagged {
                crate::fire::adjust_fire_stacks(ctx.world, e, 3.0);
                crate::fire::ignite_mob(ctx.world, e);
            } else {
                let stacks = ctx
                    .world
                    .get::<crate::fire::FireStacks>(e)
                    .map_or(0.0, |f| f.stacks);
                if stacks > 0.0 {
                    crate::fire::adjust_fire_stacks(ctx.world, e, -stacks);
                }
            }
        }
        // `/datum/reagent/water/expose_turf` on a hotspot: the air cools
        // (`cooling_temperature` 2) and the fire goes out.
        _ => {
            if let Some(mut atmos) = ctx.world.get_resource_mut::<ss13_atmos::Atmos>() {
                atmos.modify_air(pos, |air| {
                    let t = air.temperature;
                    air.temperature = (t - 2000.0).min(t / 2.0).max(ss13_atmos::gas::TCMB);
                });
            }
        }
    }
    DELAY_SUCCEED
}

/// A bot task ended: its effect.
pub fn task_done(world: &mut World, mob: Entity, task: BotTask) {
    match task.job {
        BotJob::Clean => {
            set_mode(world, mob, BOT_IDLE);
            clean_target(world, mob, task.target);
        }
        BotJob::Tend => medicate(world, mob, task.target),
        BotJob::Cuff => cuff(world, mob, task.target),
    }
}

/// A bot task was interrupted (the bot or the target moved).
pub fn task_interrupted(world: &mut World, mob: Entity, task: BotTask) {
    let _ = task;
    set_mode(world, mob, BOT_IDLE);
}

/// The cleaner component's `clean` (decals), the mop on pests, acid on trash.
fn clean_target(world: &mut World, mob: Entity, target: Entity) {
    if world.get_entity(target).is_err() {
        return;
    }
    if world.get::<DmCleanable>(target).is_some() {
        crate::janitor::wash(world, target, crate::janitor::CLEAN_SCRUB);
        return;
    }
    let name = bot_name(world, mob);
    let what = bot_name(world, target);
    let Some(&pos) = world.get::<TilePos>(target) else {
        return;
    };
    if world.get::<crate::mobs::BasicMob>(target).is_some() {
        // `our_mop.melee_attack_chain`: the mop's `force` 8.
        if let Some(mut health) = world.get::<Health>(target).cloned() {
            health.adjust(DamageType::Brute, 8.0);
            set_health(world, target, health);
        }
        show(
            world,
            pos,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Combat,
            (Span::Danger, format!("{name} бьёт {what} шваброй!")),
            &[],
        );
        return;
    }
    // `acid_act` on trash: it melts away.
    show(
        world,
        pos,
        DEFAULT_MESSAGE_RANGE,
        ChatChannel::Combat,
        (
            Span::Danger,
            format!("{name} sprays hydrofluoric acid at {what}!"),
        ),
        &[],
    );
    world.despawn(target);
}

/// tg `COMSIG_BOT_RESET` -> `reset_bot`: the plan is cancelled and the reset keys cleared.
pub fn reset(world: &mut World, mob: Entity) {
    if let Some(mut controller) = world.get_mut::<AiController>(mob) {
        for key in [
            Key::CurrentTarget,
            Key::BeaconTarget,
            Key::PreviousBeacon,
            Key::SummonTarget,
        ] {
            controller.bb.clear(key);
        }
        controller.cancel = true;
        controller.movement.stop();
    }
}

// Trees.

fn par(children: Vec<Node>, repeat_delay: f64, finish_on_primary: bool) -> Node {
    parallel(
        children,
        ParallelOpts {
            repeat_secondary: true,
            repeat_secondary_delay: repeat_delay,
            finish_on_primary,
            ..ParallelOpts::default()
        },
    )
}

fn bot_leaf(l: BotLeaf) -> Node {
    leaf(Behavior::Bot(l))
}

fn move_to(key: Key, dist: u32) -> Node {
    leaf(Behavior::MoveToTarget {
        key,
        dist,
        finish_on_arrival: true,
    })
}

/// `bot_respond_to_summon.bt.json`.
fn respond_to_summon() -> Node {
    decorator(
        Cond::KeySet(Key::SummonTarget),
        false,
        Abort::None,
        sequence(vec![
            move_to(Key::SummonTarget, 0),
            bot_leaf(BotLeaf::CompleteSummon),
        ]),
    )
}

/// `bot_salute_authority.bt.json`.
fn salute_authority(messages: &'static [&'static str]) -> Node {
    decorator(
        Cond::KeyOffCooldown(Key::SaluteCooldown),
        false,
        Abort::None,
        bot_leaf(BotLeaf::Salute { messages }),
    )
}

/// `bot_patrol.bt.json`.
fn patrol() -> Node {
    selector(vec![
        decorator(
            Cond::KeyOffCooldown(Key::BeaconCooldown),
            false,
            Abort::Lower,
            decorator(
                Cond::Bot(BotCond::ModeFlag(super::MODE_AUTOPATROL)),
                false,
                Abort::Lower,
                sequence(vec![
                    selector(vec![
                        decorator(
                            Cond::KeySet(Key::PreviousBeacon),
                            false,
                            Abort::None,
                            bot_leaf(BotLeaf::FindNextBeacon),
                        ),
                        bot_leaf(BotLeaf::FindFirstBeacon),
                    ]),
                    decorator(
                        Cond::KeySet(Key::BeaconTarget),
                        false,
                        Abort::None,
                        sequence(vec![
                            move_to(Key::BeaconTarget, 0),
                            bot_leaf(BotLeaf::ArriveAtBeacon),
                        ]),
                    ),
                ]),
            ),
        ),
        bot_leaf(BotLeaf::Wait {
            secs: 5.0,
            waited: false,
        }),
    ])
}

/// `bot.bt.json`.
fn base() -> Node {
    selector(vec![
        respond_to_summon(),
        salute_authority(BOT_SALUTES),
        patrol(),
    ])
}

/// `cleanbot.bt.json` (its unemagged branch; an emagged cleanbot only patrols).
fn cleanbot() -> Node {
    let work = par(
        vec![
            selector(vec![
                decorator(
                    Cond::KeySet(Key::CurrentTarget),
                    false,
                    Abort::Both,
                    sequence(vec![
                        move_to(Key::CurrentTarget, 1),
                        bot_leaf(BotLeaf::Clean),
                    ]),
                ),
                decorator(
                    Cond::KeyOffCooldown(Key::PostCleanCooldown),
                    false,
                    Abort::None,
                    patrol(),
                ),
            ]),
            bot_leaf(BotLeaf::AcquireCleanable),
            salute_authority(CLEANBOT_SALUTES),
        ],
        FIND_TARGET_RATE,
        true,
    );
    selector(vec![
        respond_to_summon(),
        decorator(Cond::Bot(BotCond::Emagged), true, Abort::None, work),
        patrol(),
    ])
}

/// `medbot.bt.json` with `medbot_find_and_announce_crit` and `medbot_treat_patient`.
fn medbot() -> Node {
    let treat = decorator(
        Cond::KeySet(Key::CurrentTarget),
        false,
        Abort::Both,
        sequence(vec![
            move_to(Key::CurrentTarget, 1),
            bot_leaf(BotLeaf::Tend),
        ]),
    );
    let announce = decorator(
        Cond::Bot(BotCond::MedicalFlag(super::MEDBOT_DECLARE_CRIT)),
        false,
        Abort::Both,
        decorator(
            Cond::KeySet(Key::PatientInCrit),
            false,
            Abort::Both,
            bot_leaf(BotLeaf::AnnounceCrit),
        ),
    );
    // tg runs these searches every tick (`repeat_secondary` off); every
    // `BASIC_MOB_FIND_TARGET_RATE` here, the delay its json names.
    let inner = par(
        vec![
            selector(vec![announce, treat, patrol()]),
            bot_leaf(BotLeaf::AcquireCrit),
            bot_leaf(BotLeaf::AcquirePatient),
        ],
        FIND_TARGET_RATE,
        false,
    );
    selector(vec![
        respond_to_summon(),
        par(
            vec![
                inner,
                salute_authority(BOT_SALUTES),
                decorator(
                    Cond::Bot(BotCond::MedicalFlag(super::MEDBOT_SPEAK_MODE)),
                    false,
                    Abort::None,
                    bot_leaf(BotLeaf::MedbotSpeech),
                ),
            ],
            1.0,
            false,
        ),
    ])
}

/// `secbot.bt.json`.
fn secbot() -> Node {
    let combat = parallel(
        vec![
            subplan(bot_leaf(BotLeaf::SecAttack), true, true, 0.0),
            leaf(Behavior::MoveToTarget {
                key: Key::CurrentTarget,
                dist: 1,
                finish_on_arrival: false,
            }),
        ],
        ParallelOpts {
            repeat_secondary: true,
            finish_on_primary: true,
            ..ParallelOpts::default()
        },
    );
    selector(vec![
        respond_to_summon(),
        par(
            vec![
                selector(vec![
                    decorator(Cond::KeySet(Key::CurrentTarget), false, Abort::Both, combat),
                    patrol(),
                ]),
                bot_leaf(BotLeaf::AcquireCriminal),
            ],
            FIND_TARGET_RATE,
            true,
        ),
    ])
}

/// `firebot.bt.json`.
fn firebot() -> Node {
    par(
        vec![
            selector(vec![
                respond_to_summon(),
                par(
                    vec![
                        selector(vec![
                            decorator(
                                Cond::KeySet(Key::CurrentTarget),
                                false,
                                Abort::Both,
                                sequence(vec![
                                    bot_leaf(BotLeaf::AnnounceFire),
                                    move_to(Key::CurrentTarget, 1),
                                    bot_leaf(BotLeaf::Extinguish),
                                ]),
                            ),
                            patrol(),
                        ]),
                        bot_leaf(BotLeaf::AcquireFire),
                    ],
                    FIND_TARGET_RATE,
                    true,
                ),
            ]),
            selector(vec![
                bot_leaf(BotLeaf::FirebotSpeech),
                salute_authority(BOT_SALUTES),
            ]),
        ],
        1.0,
        true,
    )
}

/// The tree of a bot controller (`/datum/ai_controller/basic_controller/bot/*`).
pub fn tree(path: &str) -> Built {
    let root = |p: &str| format!("/datum/ai_controller/basic_controller/bot{p}");
    let roots = if is_under(path, &root("/cleanbot")) {
        cleanbot()
    } else if is_under(path, &root("/medbot")) {
        medbot()
    } else if path == root("/secbot") {
        secbot()
    } else if is_under(path, &root("/firebot")) {
        firebot()
    } else if is_under(path, &root("/mulebot")) {
        // `mulebot.bt.json` without its deliveries: MULEbots do not patrol.
        // shortcut: no deliveries (loading crates, delivery beacons, the `Mule` window); add
        // with the MULEbot's cargo and PDA control.
        selector(vec![respond_to_summon(), salute_authority(BOT_SALUTES)])
    } else {
        // shortcut: the repairbot, honkbot, hygienebot, vibebot, ED-209 and dedbot trees are
        // not ported; they summon, salute and patrol like the base bot.
        base()
    };
    Built {
        roots: vec![roots],
        blackboard: Vec::new(),
        traits: RUN_WHILE_UNWATCHED,
        retaliates: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secbot(flags: u32) -> Bot {
        Bot {
            kind: BotKind::Sec,
            mode_flags: super::super::MODE_ON,
            cover: 0,
            mode: BOT_IDLE,
            req_one_access: Vec::new(),
            initial_access: Vec::new(),
            access: Vec::new(),
            radio: None,
            base_icon: "secbot".into(),
            commissioned: false,
            security_flags: flags,
            medical_flags: 0,
            heal_amount: 0.0,
            heal_threshold: 0.0,
            healer: Healer::All,
            skin: String::new(),
            janitor_flags: 0,
            firebot_flags: 0,
            robot_arm: String::new(),
            stun_ready_at: 0.0,
            threat: 0,
            medkit: String::new(),
            task: None,
        }
    }

    #[test]
    fn threat_follows_judgement_criteria() {
        let mut world = World::new();
        let anon = world.spawn(Name::new("Unknown")).id();
        let named = world.spawn(Name::new("John")).id();
        let ids = secbot(SECBOT_CHECK_IDS);
        assert_eq!(assess_threat(&world, &ids, anon), 4, "no ID, unknown face");
        assert_eq!(assess_threat(&world, &ids, named), 0);
        assert_eq!(
            assess_threat(&world, &secbot(0), anon),
            0,
            "IDs not checked"
        );
        let mut emagged = secbot(0);
        emagged.cover = super::super::COVER_EMAGGED;
        assert_eq!(assess_threat(&world, &emagged, named), 10, "everyone");
    }

    #[test]
    fn cleanbot_hunts_by_its_flags() {
        let mut world = World::new();
        let dirt = world
            .spawn(DmCleanable {
                path: "/obj/effect/decal/cleanable/dirt".into(),
            })
            .id();
        let blood = world
            .spawn(DmCleanable {
                path: "/obj/effect/decal/cleanable/blood/drip".into(),
            })
            .id();
        let crayon = world
            .spawn(DmCleanable {
                path: "/obj/effect/decal/cleanable/crayon".into(),
            })
            .id();
        assert!(cleanable(&world, dirt, 0));
        assert!(!cleanable(&world, blood, 0));
        assert!(cleanable(&world, blood, CLEANBOT_CLEAN_BLOOD));
        assert!(!cleanable(&world, crayon, CLEANBOT_CLEAN_BLOOD));
        assert!(cleanable(&world, crayon, CLEANBOT_CLEAN_DRAWINGS));
    }

    #[test]
    fn heal_all_goes_in_order() {
        let mut world = World::new();
        let mut h = Health::default();
        h.adjust(DamageType::Brute, 3.0);
        h.adjust(DamageType::Tox, 10.0);
        let p = world.spawn((h, Stat::Conscious)).id();
        heal(&mut world, p, Healer::All, 5.0);
        let h = world.get::<Health>(p).unwrap();
        assert_eq!((h.brute, h.tox), (0.0, 8.0));
        heal(&mut world, p, Healer::Type(DamageType::Tox), 2.5);
        assert_eq!(world.get::<Health>(p).unwrap().tox, 5.5);
    }
}
