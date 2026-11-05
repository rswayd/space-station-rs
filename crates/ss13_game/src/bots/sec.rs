//! The secbot family's own parts and the smaller kinds (server), after tg
//! `code/modules/mob/living/basic/bots/` `ed209/`, `honkbots/`, `secbot/super_beepsky*.dm`,
//! `hygienebot/` and `dedbot.dm`:
//!
//! - ED-209 (`ed209.bt.json`): shoots its target (a disabler beam, a laser once emagged; 3 s
//!   apart, up to 9 tiles, keeping 2–3 tiles away); a downed target is stunned and cuffed with
//!   zipties like a securitron.
//! - Honkbot (`honkbot.bt.json`): hunts like a securitron, with an air horn and fake zipties;
//!   celebrates with clowns (5 s search, a honk); someone stepping onto it while it hunts slips
//!   (70%: 6 s knockdown, 3 s paralysis, "Хонк...").
//! - General Beepsky (`super_beepsky`): hunts like a securitron but cuts with its energy swords
//!   (30 brute, a random zone), never arrests; its target stepping onto it is cut too.
//! - Securitrons trip a mob that steps onto them while their target is out of reach.
//! - Hygienebot (`hygienebot.bt.json`): finds someone to wash within 5 tiles (5 s), chases them
//!   (trash talk from the 5th try, 4 s apart), sprays its tile and theirs for 3 s: water puts out
//!   fire, an emagged one sets people alight.
//! - Dedbot (`dedbot.bt.json`): hunts the living within 2 tiles that are not of its faction and
//!   slashes everything around it (18 brute each, 0.5 s).
// shortcut: no ED-209 tackle (an emagged sentient one's ability), hats or sheriff lines; no
// honkbot slip-dragging (`honkbot_slip`) or deafness; General Beepsky blocks
// nothing; hygienebots wash from the next tile (bots do not pass mobs here) and only an emagged
// one finds targets (mobs and clothing carry no blood here); no dedbot gibs.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, Span};
use ss13_world::{Moved, SpatialIndex, WorldSystems};

use super::ai::{
    BotCond, BotLeaf, DELAY_FAIL, DELAY_SUCCEED, FAIL, RUNNING, SUCCEED, blacklist, bot, bot_leaf,
    dist, ignored, is_human, move_to, par, paralyzed, patrol, pick, respond_to_summon,
    salute_authority, search,
};
use super::{BOT_HUNT, Bot, BotKind, HONKBOT_MODE_SLIP, bot_name, speak};
use crate::health::{DEFAULT_MESSAGE_RANGE, DamageType, Health, Stat};
use crate::mobs::ai::behaviors::{Behavior, FIND_TARGET_RATE, Perform, Strategy, can_see};
use crate::mobs::ai::bt::{Abort, Cond, Node, decorator, leaf, selector, sequence, subplan};
use crate::mobs::ai::movement::{Target, can_step, dir_towards};
use crate::mobs::ai::{AiCtx, Key, Value};
use crate::security::show;
use crate::status_effects::Incap;
use crate::zone::BodyZone;

pub const HONK_LAUGH: &str = "sound/mobs/non-humanoids/honkbot/honkbot_evil_laugh.ogg";
pub const BIKEHORN: &str = "sound/items/bikehorn.ogg";
pub const AIRHORN: &str = "sound/items/airhorn/airhorn.ogg";
pub const EGLOVES: &str = "sound/items/weapons/egloves.ogg";
const LASER: &str = "sound/items/weapons/laser.ogg";
const BLADE: &str = "sound/items/weapons/blade1.ogg";
const BLADESLICE: &str = "sound/items/weapons/bladeslice.ogg";
const DRILL: &str = "sound/items/weapons/drill.ogg";
/// The guns whose first shot is the ED-209's (`projectile_type`, `emagged_projectile_type`).
const DISABLER: &str = "/obj/item/gun/energy/disabler";
const LASER_GUN: &str = "/obj/item/gun/energy/laser";
/// `ranged_attacks` `cooldown_time`, `basic_ranged_attack` `max_range`.
const SHOOT_COOLDOWN: f64 = 3.0;
const SHOOT_RANGE: u32 = 9;
/// `BB_RANGED_SKIRMISH_MIN_DISTANCE`, `_MAX_DISTANCE`.
const SKIRMISH_MIN: u32 = 2;
const SKIRMISH_MAX: u32 = 3;
/// The energy saber's `force` lit.
const SABER_FORCE: f32 = 30.0;
/// Honkbot slippery component: `knockdown`, `paralyze`.
const SLIP_KNOCKDOWN: f32 = 6.0;
const SLIP_PARALYZE: f32 = 3.0;
/// Hygienebot: `WASH_PERIOD`, `BOT_ANGER_THRESHOLD`, `BOT_FRUSTRATION_LIMIT`.
const WASH_SECS: f64 = 3.0;
const ANGER_THRESHOLD: f64 = 5.0;
const FRUSTRATION_LIMIT: f64 = 8.0;
/// Dedbot: `BB_AGGRO_RANGE`, the exenterate `damage_dealt` and cooldown.
const DED_AGGRO: u32 = 2;
const DED_DAMAGE: f32 = 18.0;
const DED_COOLDOWN: f64 = 0.5;

/// The leaves of these kinds.
#[derive(Debug)]
pub enum SecLeaf {
    /// `basic_ranged_attack` of the ED-209.
    Shoot,
    /// `maintain_distance` (2–3 tiles).
    KeepDistance,
    /// `update_interaction_target` of `BB_CLOWN_FRIEND` (5 s).
    FindClown,
    /// `play_with_clown`.
    PlayWithClown,
    /// `hygiene_wash` (5 s).
    FindUnwashed,
    /// `wash_target`.
    Wash,
    /// `commence_trashtalk`.
    TrashTalk,
    /// The dedbot's `update_combat_targets` (aggro range 2).
    FindVictim,
    /// `exenterate`.
    Slash,
}

impl SecLeaf {
    pub fn cooldown(&self) -> f64 {
        match self {
            Self::Shoot => SHOOT_COOLDOWN,
            Self::FindClown | Self::FindUnwashed => 5.0,
            Self::Slash => DED_COOLDOWN,
            _ => 0.0,
        }
    }

    pub fn perform(&mut self, ctx: &mut AiCtx) -> Perform {
        match self {
            Self::Shoot => shoot(ctx),
            Self::KeepDistance => keep_distance(ctx),
            Self::FindClown => find_clown(ctx),
            Self::PlayWithClown => play_with_clown(ctx),
            Self::FindUnwashed => find_unwashed(ctx),
            Self::Wash => wash_target(ctx),
            Self::TrashTalk => trash_talk(ctx),
            Self::FindVictim => find_victim(ctx),
            Self::Slash => slash(ctx),
        }
    }

    pub fn finish(&mut self, ctx: &mut AiCtx, succeeded: bool) {
        match self {
            Self::PlayWithClown => {
                if let Some(t) = ctx.bb.entity(Key::FriendTarget) {
                    blacklist(ctx, t);
                }
                ctx.bb.clear(Key::FriendTarget);
            }
            Self::Wash => {
                ctx.bb.clear(Key::CurrentTarget);
                let frustration = frustration(ctx);
                ctx.bb.set(Key::WashFrustration, Value::Time(0.0));
                if succeeded && frustration > ANGER_THRESHOLD {
                    let line = pick(ctx, &super::voice::HYGIENE_DONE);
                    speak(ctx.world, ctx.pawn, line, false);
                }
            }
            _ => {}
        }
    }
}

pub(super) fn build(app: &mut App) {
    app.add_systems(Update, stepped_onto.after(WorldSystems::Movement));
}

/// The bot kinds whose controllers are here.
pub fn tree(path: &str) -> Option<Node> {
    let root = |p: &str| format!("/datum/ai_controller/basic_controller/bot{p}");
    Some(if is_under(path, &root("/ed209")) {
        ed209()
    } else if is_under(path, &root("/honkbot")) {
        honkbot()
    } else if is_under(path, &root("/secbot/super_beepsky")) {
        super::ai::secbot_tree()
    } else if is_under(path, &root("/hygienebot")) {
        hygienebot()
    } else if is_under(path, &root("/dedbot")) {
        dedbot()
    } else {
        return None;
    })
}

fn sec_leaf(l: SecLeaf) -> Node {
    bot_leaf(BotLeaf::Sec(l))
}

/// `ed209.bt.json`.
fn ed209() -> Node {
    let stunned = decorator(
        Cond::Bot(BotCond::TargetDown),
        false,
        Abort::Both,
        crate::mobs::ai::bt::parallel(
            vec![
                subplan(bot_leaf(BotLeaf::SecAttack), true, true, 0.0),
                move_to(Key::CurrentTarget, 1),
            ],
            crate::mobs::ai::bt::ParallelOpts {
                repeat_secondary: true,
                finish_on_primary: true,
                ..crate::mobs::ai::bt::ParallelOpts::default()
            },
        ),
    );
    let shooting = crate::mobs::ai::bt::parallel(
        vec![
            subplan(sec_leaf(SecLeaf::Shoot), true, true, 0.0),
            sec_leaf(SecLeaf::KeepDistance),
        ],
        crate::mobs::ai::bt::ParallelOpts {
            repeat_secondary: true,
            finish_on_primary: true,
            ..crate::mobs::ai::bt::ParallelOpts::default()
        },
    );
    selector(vec![
        respond_to_summon(),
        par(
            vec![
                selector(vec![
                    decorator(
                        Cond::KeySet(Key::CurrentTarget),
                        false,
                        Abort::Both,
                        selector(vec![stunned, shooting]),
                    ),
                    patrol(),
                ]),
                bot_leaf(BotLeaf::AcquireCriminal),
            ],
            FIND_TARGET_RATE,
            true,
        ),
    ])
}

/// `honkbot.bt.json` without `honkbot_slip`.
fn honkbot() -> Node {
    let friend = decorator(
        Cond::KeySet(Key::FriendTarget),
        false,
        Abort::Both,
        sequence(vec![
            move_to(Key::FriendTarget, 1),
            sec_leaf(SecLeaf::PlayWithClown),
        ]),
    );
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
                                super::ai::sec_combat(),
                            ),
                            friend,
                            patrol(),
                        ]),
                        selector(vec![
                            bot_leaf(BotLeaf::AcquireCriminal),
                            sec_leaf(SecLeaf::FindClown),
                        ]),
                    ],
                    FIND_TARGET_RATE,
                    true,
                ),
            ]),
            salute_authority(super::ai::BOT_SALUTES),
        ],
        1.0,
        true,
    )
}

/// `hygienebot.bt.json`.
fn hygienebot() -> Node {
    let wash = decorator(
        Cond::KeySet(Key::CurrentTarget),
        false,
        Abort::Both,
        par(
            vec![
                crate::mobs::ai::bt::parallel(
                    vec![
                        sec_leaf(SecLeaf::Wash),
                        leaf(Behavior::MoveToTarget {
                            key: Key::CurrentTarget,
                            dist: 1,
                            finish_on_arrival: false,
                        }),
                    ],
                    crate::mobs::ai::bt::ParallelOpts {
                        repeat_secondary: true,
                        finish_on_primary: true,
                        ..crate::mobs::ai::bt::ParallelOpts::default()
                    },
                ),
                decorator(
                    Cond::KeyOffCooldown(Key::TrashTalkCooldown),
                    false,
                    Abort::None,
                    sec_leaf(SecLeaf::TrashTalk),
                ),
            ],
            1.0,
            true,
        ),
    );
    selector(vec![
        respond_to_summon(),
        par(
            vec![
                selector(vec![wash, patrol()]),
                sec_leaf(SecLeaf::FindUnwashed),
                salute_authority(super::ai::BOT_SALUTES),
            ],
            FIND_TARGET_RATE,
            true,
        ),
    ])
}

/// `dedbot.bt.json`.
fn dedbot() -> Node {
    par(
        vec![
            selector(vec![
                decorator(
                    Cond::KeySet(Key::CurrentTarget),
                    false,
                    Abort::Both,
                    sequence(vec![
                        move_to(Key::CurrentTarget, 1),
                        sec_leaf(SecLeaf::Slash),
                    ]),
                ),
                respond_to_summon(),
                patrol(),
            ]),
            sec_leaf(SecLeaf::FindVictim),
        ],
        FIND_TARGET_RATE,
        true,
    )
}

/// `is_target_stunned`: paralyzed, or down in stamina crit (a disabler's work).
pub fn target_down(world: &World, target: Entity) -> bool {
    paralyzed(world, target)
        || world
            .get::<crate::status_effects::StatusEffects>(target)
            .is_some_and(|s| s.stamcrit)
}

fn target(ctx: &AiCtx, key: Key) -> Option<(Entity, TilePos)> {
    let t = ctx
        .bb
        .entity(key)
        .filter(|&t| ctx.world.get_entity(t).is_ok())?;
    Some((t, ctx.world.get::<TilePos>(t).copied()?))
}

/// The first shot of `gun`'s prototype: what the ED-209 fires.
fn shot_of(world: &World, gun: &str) -> Option<ss13_mapload::CasingData> {
    let proto = world.get_resource::<ss13_proto::Prototypes>()?.get(gun)?;
    proto.get::<ss13_mapload::DmGun>()?.shots.first().cloned()
}

/// `basic_ranged_attack` -> `ranged_attacks/fire_ranged_attack` of the ED-209.
fn shoot(ctx: &mut AiCtx) -> Perform {
    let Some((t, pos)) = target(ctx, Key::CurrentTarget) else {
        return FAIL;
    };
    let Some(from) = ctx.pawn_pos() else {
        return FAIL;
    };
    if from.chebyshev(pos).is_none_or(|d| d > SHOOT_RANGE)
        || !can_see(ctx.world, from, pos, SHOOT_RANGE)
    {
        return DELAY_FAIL;
    }
    let emagged = bot(ctx).is_some_and(Bot::emagged);
    let Some(casing) = shot_of(ctx.world, if emagged { LASER_GUN } else { DISABLER }) else {
        return DELAY_FAIL;
    };
    super::play(ctx.world, ctx.pawn, LASER, 50.0);
    crate::projectile::fire(
        ctx.world,
        crate::projectile::Shot {
            data: &casing.projectile,
            firer: ctx.pawn,
            from,
            target: Some(t),
            target_pos: pos,
            zone: BodyZone::Chest,
            hit_prone: true,
            pixel: [0, 0],
            deviation: 0.0,
            fired_from: None,
        },
    );
    DELAY_SUCCEED
}

/// `maintain_distance`: closer than 2 steps back, further than 3 closes in.
fn keep_distance(ctx: &mut AiCtx) -> Perform {
    let Some((t, pos)) = target(ctx, Key::CurrentTarget) else {
        return FAIL;
    };
    let Some(from) = ctx.pawn_pos() else {
        return FAIL;
    };
    let d = from.chebyshev(pos).unwrap_or(u32::MAX);
    if d > SKIRMISH_MAX {
        ctx.movement.start(Target::Entity(t), SKIRMISH_MAX);
    } else if d < SKIRMISH_MIN {
        ctx.movement.stop();
        let away = dir_towards(pos, from)
            .filter(|&dir| can_step(ctx.world.resource::<SpatialIndex>(), ctx.pawn, from, dir));
        if let Some(dir) = away
            && ctx
                .world
                .get::<ss13_world::Mover>(ctx.pawn)
                .is_some_and(|m| m.ready_at() <= ctx.now)
        {
            ctx.world.write_message(ss13_world::MoveRequest {
                entity: ctx.pawn,
                dir,
            });
        }
    } else {
        ctx.movement.stop();
    }
    RUNNING
}

/// `targeting_strategy/clown_friend`: a conscious clown.
fn is_clown(world: &World, e: Entity) -> bool {
    if world.get::<Stat>(e) != Some(&Stat::Conscious) {
        return false;
    }
    let Some(job) = world.get::<crate::jobs::PlayerJob>(e) else {
        return false;
    };
    world
        .get_resource::<crate::jobs::Jobs>()
        .and_then(|j| j.data.jobs.get(job.0))
        .is_some_and(|j| j.title == "Clown")
}

fn find_clown(ctx: &mut AiCtx) -> Perform {
    if ctx.truthy(Key::FriendTarget) {
        return DELAY_SUCCEED;
    }
    let found = search(ctx, 5, is_clown);
    match super::ai::first_reachable(ctx, found, 1, 10, true) {
        Some(t) => {
            ctx.bb.set(Key::FriendTarget, Value::Entity(t));
            DELAY_SUCCEED
        }
        None => DELAY_FAIL,
    }
}

/// `play_with_clown`: an emote and a honk.
fn play_with_clown(ctx: &mut AiCtx) -> Perform {
    let Some((t, pos)) = target(ctx, Key::FriendTarget) else {
        return DELAY_FAIL;
    };
    if dist(ctx, pos).is_none_or(|d| d > 1) {
        return RUNNING;
    }
    let who = bot_name(ctx.world, t);
    crate::mobs::defense::emote(ctx.world, ctx.pawn, &format!("празднует вместе с {who}!"));
    honk(ctx.world, ctx.pawn);
    DELAY_SUCCEED
}

/// `/datum/action/cooldown/mob_cooldown/bot/honk`.
pub fn honk(world: &mut World, mob: Entity) {
    super::play(world, mob, BIKEHORN, 50.0);
    attacking_state(world, mob);
}

/// `set_attacking_state`: "-c" for a moment (until the next look refresh).
fn attacking_state(world: &mut World, mob: Entity) {
    let Some(icon) = world
        .get::<crate::mobs::BasicMob>(mob)
        .map(|b| b.def.icon.clone())
    else {
        return;
    };
    let base = world
        .get::<Bot>(mob)
        .map(|b| b.base_icon.clone())
        .unwrap_or_default();
    crate::mobs::set_icon_state(world, mob, &icon, &format!("{base}-c"));
}

/// The honkbot's `post_stun`: a carbon target stutters 40 s and jitters (at least 100 s), the
/// "-c" look (the deafening is not ported).
pub fn honk_post_stun(world: &mut World, mob: Entity, target: Entity) {
    if world
        .get::<crate::medical::organs::OrganHost>(target)
        .is_none()
    {
        return;
    }
    use crate::status_effects::{Timed, set_timed};
    set_timed(world, target, Timed::Stutter, 40.0, false);
    set_timed(world, target, Timed::Jitter, 100.0, true);
    attacking_state(world, mob);
}

/// The honkbot's `post_arrest`: a honk (any of the `honkbot_e` sounds once emagged) and the
/// evil laugh.
pub fn honk_post_arrest(world: &mut World, mob: Entity) {
    let emagged = world.get::<Bot>(mob).is_some_and(Bot::emagged);
    super::play(
        world,
        mob,
        if emagged { "honkbot_e" } else { BIKEHORN },
        50.0,
    );
    super::play(world, mob, HONK_LAUGH, 75.0);
    let name = bot_name(world, mob);
    if let Some(&pos) = world.get::<TilePos>(mob) {
        show(
            world,
            pos,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Local,
            (Span::Danger, format!("{name} зловеще хохочет!")),
            &[],
        );
    }
}

/// General Beepsky's `early_melee_attack`: the swords.
pub fn grievous_slash(world: &mut World, mob: Entity, victim: Entity) {
    let Some(&pos) = world.get::<TilePos>(victim) else {
        return;
    };
    let zone = {
        let mut rng = world.resource_mut::<crate::combat::CombatRng>();
        BodyZone::ALL[rng.range(0, BodyZone::ALL.len() as u32 - 1) as usize]
    };
    super::hurt_zone(world, victim, zone, DamageType::Brute, SABER_FORCE);
    super::play(world, mob, BLADE, 50.0);
    let (name, who) = (bot_name(world, mob), bot_name(world, victim));
    show(
        world,
        pos,
        DEFAULT_MESSAGE_RANGE,
        ChatChannel::Combat,
        (
            Span::Danger,
            format!("{name} рубит {who} энергетическим мечом!"),
        ),
        &[(
            victim,
            Span::BoldWarning,
            format!("{name} рубит вас энергетическим мечом!"),
        )],
    );
}

/// `targeting_strategy/capable_human/washable_human`: a conscious human, any while emagged.
// shortcut: no blood on clothes here, so only an emagged hygienebot finds anyone.
fn washable(world: &World, hygienebot: &Bot, e: Entity) -> bool {
    is_human(world, e) && world.get::<Stat>(e) == Some(&Stat::Conscious) && hygienebot.emagged()
}

fn frustration(ctx: &AiCtx) -> f64 {
    ctx.bb.time(Key::WashFrustration).unwrap_or(0.0)
}

fn find_unwashed(ctx: &mut AiCtx) -> Perform {
    let Some(me) = bot(ctx).cloned() else {
        return FAIL;
    };
    if ctx.truthy(Key::CurrentTarget) {
        return DELAY_SUCCEED;
    }
    let found = search(ctx, 5, |w, e| washable(w, &me, e));
    let Some(&(t, _)) = found.first() else {
        return DELAY_FAIL;
    };
    ctx.bb.set(Key::CurrentTarget, Value::Entity(t));
    // `on_target_found`: an emagged one washes each person once.
    if me.emagged() {
        blacklist(ctx, t);
    }
    speak(ctx.world, ctx.pawn, super::voice::HYGIENE_FOUND[0], false);
    DELAY_SUCCEED
}

/// `wash_target` -> `melee_attack` -> `on_attack`: 3 s of spraying.
fn wash_target(ctx: &mut AiCtx) -> Perform {
    let Some((t, pos)) = target(ctx, Key::CurrentTarget) else {
        return DELAY_FAIL;
    };
    if dist(ctx, pos).is_none_or(|d| d > 1) {
        return RUNNING;
    }
    start_washing(ctx.world, ctx.pawn, ctx.now);
    commence_wash(ctx.world, ctx.pawn, t);
    DELAY_SUCCEED
}

/// `on_attack`: washing for `WASH_PERIOD`, everything on its tile washed.
fn start_washing(world: &mut World, mob: Entity, now: f64) {
    let Some(bot) = world.get::<Bot>(mob) else {
        return;
    };
    if now < bot.washing_until {
        return;
    }
    if let Some(mut b) = world.get_mut::<Bot>(mob) {
        b.washing_until = now + WASH_SECS;
    }
    super::refresh_look(world, mob);
    if let Some(&pos) = world.get::<TilePos>(mob) {
        let here: Vec<Entity> = world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .filter(|&e| e != mob && world.get::<Health>(e).is_some())
            .collect();
        for e in here {
            commence_wash(world, mob, e);
        }
    }
}

/// `commence_wash`: water puts out fire; an emagged one's flames set it alight.
fn commence_wash(world: &mut World, mob: Entity, target: Entity) {
    if world.get::<Bot>(mob).is_some_and(Bot::emagged) {
        crate::fire::adjust_fire_stacks(world, target, 3.0);
        crate::fire::ignite_mob(world, target);
        return;
    }
    let stacks = world
        .get::<crate::fire::FireStacks>(target)
        .map_or(0.0, |f| f.stacks);
    if stacks > 0.0 {
        crate::fire::adjust_fire_stacks(world, target, -stacks);
    }
}

fn trash_talk(ctx: &mut AiCtx) -> Perform {
    if !ctx.truthy(Key::CurrentTarget) {
        return DELAY_FAIL;
    }
    let count = (frustration(ctx) + 1.0).min(FRUSTRATION_LIMIT);
    ctx.bb.set(Key::WashFrustration, Value::Time(count));
    ctx.bb
        .set(Key::TrashTalkCooldown, Value::Time(ctx.now + 4.0));
    if count < ANGER_THRESHOLD {
        return DELAY_FAIL;
    }
    let line = pick(ctx, &super::voice::HYGIENE_THREATS);
    speak(ctx.world, ctx.pawn, line, false);
    DELAY_SUCCEED
}

fn find_victim(ctx: &mut AiCtx) -> Perform {
    if let Some((t, pos)) = target(ctx, Key::CurrentTarget)
        && dist(ctx, pos).is_some_and(|d| d <= 16)
        && Strategy::Basic.valid(ctx, t, 0)
    {
        return SUCCEED;
    }
    let found = search(ctx, DED_AGGRO, |_, _| true);
    let victim = found
        .into_iter()
        .map(|(e, _)| e)
        .find(|&e| Strategy::Basic.valid(ctx, e, DED_AGGRO) && !ignored(ctx, e));
    match victim {
        Some(t) => {
            ctx.bb.set(Key::CurrentTarget, Value::Entity(t));
            SUCCEED
        }
        None => {
            ctx.bb.clear(Key::CurrentTarget);
            FAIL
        }
    }
}

/// `exenterate`: 18 brute to every living thing around it but its own faction.
fn slash(ctx: &mut AiCtx) -> Perform {
    let Some((_, pos)) = target(ctx, Key::CurrentTarget) else {
        return DELAY_FAIL;
    };
    if dist(ctx, pos).is_none_or(|d| d > 1) {
        return DELAY_FAIL;
    }
    let Some(from) = ctx.pawn_pos() else {
        return DELAY_FAIL;
    };
    let around: Vec<Entity> = ctx
        .world
        .resource::<SpatialIndex>()
        .in_radius(from, 1)
        .filter(|&e| e != ctx.pawn)
        .collect();
    let victims: Vec<Entity> = around
        .into_iter()
        .filter(|&e| Strategy::Basic.valid(ctx, e, 1))
        .collect();
    let name = bot_name(ctx.world, ctx.pawn);
    super::play(ctx.world, ctx.pawn, DRILL, 100.0);
    show(
        ctx.world,
        from,
        DEFAULT_MESSAGE_RANGE,
        ChatChannel::Combat,
        (Span::Danger, format!("{name} яростно трясётся!")),
        &[],
    );
    for victim in victims {
        let zone = [
            BodyZone::Chest,
            BodyZone::Chest,
            BodyZone::RArm,
            BodyZone::LArm,
            BodyZone::RLeg,
            BodyZone::LLeg,
        ][ctx.rng.range(0, 5) as usize];
        if ctx.world.get::<crate::mobs::BasicMob>(victim).is_some() {
            if let Some(mut h) = ctx.world.get::<Health>(victim).cloned() {
                h.adjust(DamageType::Brute, DED_DAMAGE);
                super::ai::set_health(ctx.world, victim, h);
            }
        } else {
            super::hurt_zone(ctx.world, victim, zone, DamageType::Brute, DED_DAMAGE);
        }
        if let Some(c) = ctx
            .world
            .get::<crate::server::PlayerMob>(victim)
            .map(|p| p.client)
        {
            crate::tools::tell(
                ctx.world,
                c,
                Span::Warning,
                format!("Вас режут лезвия {name}!"),
            );
        }
    }
    super::play(ctx.world, ctx.pawn, BLADESLICE, 50.0);
    DELAY_SUCCEED
}

/// `COMSIG_ATOM_ENTERED` on a bot's tile (`connect_loc`): a securitron trips who steps onto it
/// while its target is out of reach, a honkbot slips them, General Beepsky cuts his target, a
/// cleanbot with a knife stabs (and learns titles), a spraying hygienebot washes them.
// Cost: per tick, per `Moved` one tile lookup (a few entities); bots on that tile only.
fn stepped_onto(world: &mut World, mut moves: Local<MessageCursor<Moved>>) {
    let moved: Vec<Moved> = moves
        .read(world.resource::<Messages<Moved>>())
        .copied()
        .collect();
    let now = world.resource::<Time>().elapsed_secs_f64();
    for step in moved {
        let mob = step.entity;
        if world.get::<Health>(mob).is_none() || world.get::<Bot>(mob).is_some() {
            continue;
        }
        let bots: Vec<Entity> = world
            .resource::<SpatialIndex>()
            .on_tile(step.to)
            .iter()
            .copied()
            .filter(|&e| e != mob && world.get::<Bot>(e).is_some_and(Bot::on))
            .collect();
        for bot in bots {
            entered(world, bot, mob, now);
        }
    }
}

fn entered(world: &mut World, bot: Entity, mob: Entity, now: f64) {
    let Some(b) = world.get::<Bot>(bot).cloned() else {
        return;
    };
    let current = world
        .get::<crate::mobs::ai::AiController>(bot)
        .and_then(|c| c.bb.entity(Key::CurrentTarget));
    let carbon = is_human(world, mob);
    match b.kind {
        // `secbot/on_entered` -> `knockOver`.
        BotKind::Sec if carbon => {
            let Some(target) = current else {
                return;
            };
            let near = world
                .get::<TilePos>(target)
                .zip(world.get::<TilePos>(bot))
                .and_then(|(a, b)| a.chebyshev(*b))
                .is_some_and(|d| d <= 1);
            if !near {
                let (name, who) = (bot_name(world, bot), bot_name(world, mob));
                // `knockOver`: `Paralyze(40)`.
                crate::status_effects::apply(world, mob, Incap::Paralyze, 4.0);
                if let Some(&pos) = world.get::<TilePos>(mob) {
                    show(
                        world,
                        pos,
                        DEFAULT_MESSAGE_RANGE,
                        ChatChannel::Combat,
                        (
                            Span::Warning,
                            format!("{who} спотыкается об {name} и падает!"),
                        ),
                        &[],
                    );
                }
            }
        }
        // The honkbot's slippery component (`pre_slip`: 70% while hunting).
        BotKind::Honk if carbon => {
            if current.is_none() || b.security_flags & HONKBOT_MODE_SLIP == 0 {
                return;
            }
            if !world
                .resource_mut::<crate::combat::CombatRng>()
                .chance(70.0)
            {
                return;
            }
            crate::status_effects::knockdown(world, mob, SLIP_KNOCKDOWN, 0.0);
            crate::status_effects::apply(world, mob, Incap::Paralyze, SLIP_PARALYZE);
            speak(world, bot, "Хонк...", false);
            attacking_state(world, bot);
        }
        BotKind::Grievous if current == Some(mob) => {
            let name = bot_name(world, bot);
            let who = bot_name(world, mob);
            super::play(
                world,
                bot,
                "sound/mobs/non-humanoids/beepsky/beepskyspinsabre.ogg",
                100.0,
            );
            if let Some(&pos) = world.get::<TilePos>(mob) {
                show(
                    world,
                    pos,
                    DEFAULT_MESSAGE_RANGE,
                    ChatChannel::Combat,
                    (
                        Span::Warning,
                        format!("{name} размахивает мечами и режет {who}!"),
                    ),
                    &[],
                );
            }
            grievous_slash(world, bot, mob);
        }
        BotKind::Clean if carbon && b.knife.is_some() => super::cleanbot_stab(world, bot, mob),
        BotKind::Hygiene if now < b.washing_until => commence_wash(world, bot, mob),
        _ => {}
    }
}

/// The secbot kinds' target was set (`on_target_set` of the controller): the announcement and
/// the mode.
pub fn on_target_set(world: &mut World, mob: Entity, target: Entity, threat: i32) {
    let Some(kind) = world.get::<Bot>(mob).map(|b| b.kind) else {
        return;
    };
    match kind {
        BotKind::Sec | BotKind::Grievous => {
            speak(
                world,
                mob,
                &format!("Level {threat} infraction alert!"),
                false,
            );
            let file = {
                let mut rng = world.resource_mut::<crate::combat::CombatRng>();
                [
                    "sound/mobs/non-humanoids/beepsky/criminal.ogg",
                    "sound/mobs/non-humanoids/beepsky/justice.ogg",
                    "sound/mobs/non-humanoids/beepsky/freeze.ogg",
                ][rng.range(0, 2) as usize]
            };
            super::play(world, mob, file, 50.0);
            super::set_mode(world, mob, BOT_HUNT);
            if kind == BotKind::Grievous {
                let (name, who) = (bot_name(world, mob), bot_name(world, target));
                crate::mobs::defense::emote(world, mob, &format!("указывает на {who}!"));
                let _ = name;
            }
        }
        BotKind::Ed209 => {
            let line = {
                let mut rng = world.resource_mut::<crate::combat::CombatRng>();
                ["Scumbag alert!", "Threat detected!"][rng.range(0, 1) as usize]
            };
            speak(world, mob, line, false);
        }
        // The honkbot controller has no `on_target_set`.
        _ => {}
    }
}

/// The custom controls of these kinds (`ui_data`): the securitron's, then the kind's own.
pub fn toggles(kind: BotKind) -> &'static [(&'static str, u32)] {
    use super::{SECBOT_CHECK_IDS, SECBOT_CHECK_RECORDS, SECBOT_HANDCUFF_TARGET};
    match kind {
        BotKind::Ed209 => &[
            ("handcuff", SECBOT_HANDCUFF_TARGET),
            ("check_ids", SECBOT_CHECK_IDS),
            ("check_records", SECBOT_CHECK_RECORDS),
        ],
        BotKind::Honk => &[
            ("slip_people", HONKBOT_MODE_SLIP),
            ("fake_cuff", SECBOT_HANDCUFF_TARGET),
            ("check_ids", SECBOT_CHECK_IDS),
            ("check_records", SECBOT_CHECK_RECORDS),
        ],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controllers_pick_their_trees() {
        let c = |p: &str| format!("/datum/ai_controller/basic_controller/bot{p}");
        assert!(tree(&c("/ed209")).is_some());
        assert!(tree(&c("/ed209/syndicate")).is_some());
        assert!(tree(&c("/honkbot")).is_some());
        assert!(tree(&c("/hygienebot")).is_some());
        assert!(tree(&c("/dedbot")).is_some());
        assert!(tree(&c("/secbot/super_beepsky")).is_some());
        assert!(tree(&c("/secbot")).is_none());
        assert!(tree(&c("/vibebot")).is_none());
    }

    #[test]
    fn kind_toggles_follow_tg_names() {
        assert_eq!(
            toggles(BotKind::Honk)[0],
            ("slip_people", HONKBOT_MODE_SLIP)
        );
        assert_eq!(toggles(BotKind::Ed209).len(), 3);
        assert!(toggles(BotKind::Sec).is_empty());
    }
}
