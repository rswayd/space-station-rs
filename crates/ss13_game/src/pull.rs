//! Pulling and grabs (server), after tg `atoms_movable.dm` (`start_pulling`, `Move`,
//! `check_pulling`, `can_be_pulled`), `living.dm` (`/mob/living/start_pulling`, `resist_grab`,
//! `update_pull_movespeed`), `living_defense.dm` (`grabbedby`, `grippedby`), `click_ctrl.dm` and
//! `mob_movement.dm` (`Process_Grab`).
//!
//! - Ctrl+click on an adjacent unanchored movable (an item on the floor, a closet, a living mob)
//!   starts pulling it ([`Pulling`] on the puller, [`PulledBy`] on the pulled; one puller per
//!   thing, a new puller takes it over). Ctrl+click on the mob already pulled upgrades the passive
//!   grab to an aggressive one (tg `grippedby`, the first upgrade is instant).
//! - After every cardinal step of the puller the pulled thing steps into the tile the puller left
//!   when it ended up diagonal (except on the second half of a diagonal step) or farther than one
//!   tile (tg `Move` -> `move_from_pull`). The pull breaks at distance > 1, when the puller is
//!   incapacitated, or the pulled thing leaves the map (picked up).
//! - Pulling a lying mob (passively) or a closet slows the puller by `PULL_PRONE_SLOWDOWN` /
//!   `drag_slowdown` ([`PullSlowdown`], added to the move delay by `crate::health`).
//! - A pulled mob that tries to step while in an aggressive grab, lying or incapacitated must
//!   resist first ([`HeldByGrab`] is checked by `crate::server`; tg `Process_Grab`): 60% divided
//!   by the effective grab state, a failure blocks moving for 4 s. The resist key (B) does the
//!   same without the 4 s penalty, with `CLICK_CD_RESIST`. A standing conscious mob in a passive
//!   pull simply walks away (the pull breaks on distance). A restrained (handcuffed) pulled mob
//!   cannot step at all (1 s, tg `Process_Grab`); its resist key works on the cuffs
//!   (`crate::restraints`) instead of the grab.
//! - The stop pulling key (C, Delete) or the HUD pull icon stops pulling.
// shortcut: grabs stop at aggressive (no neck/kill grab with their `do_after`), no pull offsets
// (`set_pull_offsets` pixel shifts), no mob swapping with the pulled mob, the puller is not turned
// to face a dragged dense object (`client/Move`), and no conga lines in one tick; add with
// strangling, buckling and the HUD pull state on the client.

use bevy::ecs::entity::EntityHashMap;
use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::{DmCloset, DmItem};
use ss13_net::chat::server::SendChat;
use ss13_net::server::IntentReceived;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::protocol::ClientIntent;
use ss13_shared::{Dir, Facing, TilePos};
use ss13_world::{MoveRequest, Moved, SpatialIndex, WorldSystems, resolve_move};

use crate::click::{CtrlClick, ResolveClicks};
use crate::combat::{CLICK_CD_MELEE, CombatRng, NextMove};
use crate::health::{BodyPosition, DEFAULT_MESSAGE_RANGE, Health, HealthSystems, Stat};
use crate::hud::{InteractHudData, InteractHudUi};
use crate::inventory::CarriedBy;
use crate::server::{PlayerMob, Players};
use crate::status_effects::Incapacitation;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiParams, UiReject, UiSystems};

/// `PULL_PRONE_SLOWDOWN`, deciseconds; closets' `drag_slowdown` is the same.
pub const PULL_PRONE_SLOWDOWN: f32 = 1.5;
/// `CLICK_CD_GRABBING`, `CLICK_CD_RESIST`, seconds.
pub const CLICK_CD_GRABBING: f64 = 1.0;
pub const CLICK_CD_RESIST: f64 = 2.0;
/// `BASE_GRAB_RESIST_CHANCE`, percent.
pub const BASE_GRAB_RESIST_CHANCE: f32 = 60.0;
/// `resist_grab(moving_resist)` failure: `client.move_delay = world.time + 4 SECONDS`.
pub const MOVING_RESIST_FAIL_SECS: f64 = 4.0;
/// `Process_Grab` of an incapacitated mob: `COOLDOWN_START(src, move_delay, 1 SECONDS)`.
pub const INCAPACITATED_GRAB_SECS: f64 = 1.0;

/// tg `pulling`, on the puller. One-to-one: inserting it on another puller takes the pulled
/// thing away from the first (tg `pulledby.stop_pulling()`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[relationship(relationship_target = PulledBy)]
pub struct Pulling(pub Entity);

/// tg `pulledby`, on the pulled thing.
#[derive(Component, Debug)]
#[relationship_target(relationship = Pulling)]
pub struct PulledBy(Entity);

impl PulledBy {
    pub fn puller(&self) -> Entity {
        self.0
    }
}

/// tg `grab_state` of a puller (`GRAB_PASSIVE`, `GRAB_AGGRESSIVE`); lives as long as [`Pulling`].
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum GrabState {
    #[default]
    Passive,
    Aggressive,
}

impl GrabState {
    pub fn level(self) -> u32 {
        match self {
            Self::Passive => 0,
            Self::Aggressive => 1,
        }
    }
}

/// tg `bulky_drag` movespeed modifier of a puller, deciseconds per tile (0 = none).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct PullSlowdown(pub f32);

/// On a pulled mob whose steps must resist the grab first (tg `Process_Grab` does not let it
/// through): an aggressive grab, or a lying or incapacitated mob.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct HeldByGrab;

/// A held mob tried to step `dir` (written by `crate::server` instead of a `MoveRequest`).
#[derive(Message, Clone, Copy, Debug)]
pub struct MoveResist {
    pub mob: Entity,
    pub dir: Dir,
}

/// `Time::elapsed_secs_f64` until which a held mob's steps are refused without a resist roll.
#[derive(Component, Clone, Copy, Debug, Default)]
struct ResistCooldown(f64);

pub struct PullServerPlugin;

impl Plugin for PullServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<MoveResist>()
            .add_systems(
                Update,
                (
                    follow_pullers,
                    check_pulls,
                    (
                        stop_pulling_intents,
                        ctrl_clicks,
                        resist_intents,
                        move_resists,
                    ),
                    (pull_slowdown, mark_held),
                )
                    .chain()
                    .after(WorldSystems::Movement)
                    .after(ResolveClicks)
                    .before(HealthSystems),
            )
            .add_systems(
                Update,
                refresh_interact_hud
                    .after(HealthSystems)
                    .before(UiSystems::Open),
            )
            .register_ui::<InteractHudUi>()
            .add_observer(on_stop_pulling)
            .add_observer(on_unpulled)
            .add_observer(open_interact_hud)
            .add_observer(close_interact_hud);
    }
}

impl ServerUi for InteractHudUi {
    type Source = &'static InteractHudData;
    type UserParam = ();
    type Act = ();

    fn static_data(_: &InteractHudData) {}

    fn data(source: &InteractHudData, _: Entity, _: &()) -> InteractHudData {
        *source
    }

    /// The buttons send `ClientIntent`s; the window takes no acts.
    fn parse_act(_: &str, _: &UiParams) -> Result<(), UiReject> {
        Err(UiReject::UnknownAction)
    }
}

/// A living player mob gets the pull and throw HUD (not a ghost).
fn open_interact_hud(
    add: On<Add<PlayerMob>>,
    mobs: Query<
        (&PlayerMob, Option<&crate::janitor::Walking>),
        (With<Stat>, Without<crate::ghost::Ghost>),
    >,
    mut open: MessageWriter<OpenUi>,
    mut commands: Commands,
) {
    if let Ok((player, walking)) = mobs.get(add.entity) {
        // A body taken over keeps its move intent; pulling and throw mode do not survive it.
        commands.entity(add.entity).try_insert(InteractHudData {
            walking: walking.is_some_and(|w| w.0),
            ..default()
        });
        open.write(OpenUi::new::<InteractHudUi>(
            player.client,
            add.entity,
            add.entity,
            "",
        ));
    }
}

/// The body lost its player (ghost, disconnect): the window closes with the data.
fn close_interact_hud(remove: On<Remove<PlayerMob>>, mut commands: Commands) {
    if let Ok(mut mob) = commands.get_entity(remove.entity) {
        mob.try_remove::<InteractHudData>();
    }
}

/// Pull, throw and move intent state into the owner's HUD when any changes.
fn refresh_interact_hud(
    changed: Query<
        Entity,
        Or<(
            Changed<crate::throw::ThrowMode>,
            Changed<GrabState>,
            Changed<crate::janitor::Walking>,
            Changed<crate::rest::Rest>,
        )>,
    >,
    mut stopped: RemovedComponents<GrabState>,
    mut mobs: Query<(
        &PlayerMob,
        &mut InteractHudData,
        Option<&crate::throw::ThrowMode>,
        Has<Pulling>,
        Option<&crate::janitor::Walking>,
        Option<&crate::rest::Rest>,
    )>,
    mut open: MessageWriter<OpenUi>,
) {
    for mob in changed.iter().chain(stopped.read()) {
        let Ok((player, mut hud, throw_mode, pulling, walking, rest)) = mobs.get_mut(mob) else {
            continue;
        };
        let data = InteractHudData {
            throw_mode: throw_mode.is_some_and(|t| t.0),
            pulling,
            walking: walking.is_some_and(|w| w.0),
            resting: rest.is_some_and(|r| r.resting),
        };
        if hud.set_if_neq(data) {
            open.write(OpenUi::new::<InteractHudUi>(player.client, mob, mob, ""));
        }
    }
}

/// tg `can_be_pulled` without move force: an unanchored item lying on a tile, a closet that is
/// not wall-mounted, an unanchored structure or machine, a living mob; never the puller itself.
pub fn can_be_pulled(world: &World, puller: Entity, target: Entity) -> bool {
    let Ok(entity) = world.get_entity(target) else {
        return false;
    };
    if target == puller || !entity.contains::<TilePos>() || entity.contains::<CarriedBy>() {
        return false;
    }
    if let Some(item) = entity.get::<DmItem>() {
        return !item.anchored;
    }
    if let Some(closet) = entity.get::<DmCloset>() {
        return !closet.wall_mounted;
    }
    // A mech's `move_resist` (`MOVE_FORCE_EXTREMELY_STRONG`) is beyond any puller here.
    if entity.contains::<crate::mecha::Mech>() {
        return false;
    }
    if let Some(anchored) = entity.get::<crate::obj_damage::Anchored>() {
        return !anchored.0;
    }
    entity.contains::<Health>()
}

/// One cardinal step of any movable, BYOND `step()` without a mover cooldown (pulled things,
/// thrown items, pushed mobs): turns it (if it has `Facing`), refuses on the same blockers as
/// `ss13_world` (`Exit` of border objects, then `Enter`), and writes `Moved`. `Err` holds the
/// blockers, except `ignore`.
pub(crate) fn step_once(
    world: &mut World,
    entity: Entity,
    dir: Dir,
    ignore: Option<Entity>,
) -> Result<(), Vec<Entity>> {
    step_passing(world, entity, dir, |_, e| Some(e) == ignore)
}

/// [`step_once`] where the blockers `passes` accepts let it through.
pub(crate) fn step_passing(
    world: &mut World,
    entity: Entity,
    dir: Dir,
    passes: impl Fn(&World, Entity) -> bool,
) -> Result<(), Vec<Entity>> {
    let Some(&from) = world.get::<TilePos>(entity) else {
        return Err(Vec::new());
    };
    let to = from.step(dir);
    let index = world.resource::<SpatialIndex>();
    if !index.in_bounds(to) {
        return Err(Vec::new());
    }
    let keep = |e: &Entity| !passes(world, *e);
    let mut blockers: Vec<Entity> = index
        .exit_blockers(from, dir, entity)
        .filter(keep)
        .collect();
    if blockers.is_empty() {
        blockers.extend(index.enter_blockers(to, dir, entity).filter(keep));
    }
    if let Some(mut facing) = world.get_mut::<Facing>(entity) {
        facing.set_if_neq(Facing(dir));
    }
    if !blockers.is_empty() {
        return Err(blockers);
    }
    world.entity_mut(entity).insert(to);
    world.write_message(Moved {
        entity,
        from,
        to,
        dir,
    });
    Ok(())
}

/// tg `Move` toward an adjacent tile: a diagonal splits into two cardinal steps like
/// `ss13_world` ([`resolve_move`]).
pub(crate) fn move_to_adjacent(world: &mut World, entity: Entity, to: TilePos) -> bool {
    let Some(&from) = world.get::<TilePos>(entity) else {
        return false;
    };
    let Some(dir) = dir_between(from, to) else {
        return false;
    };
    resolve_move(dir, |d| step_once(world, entity, d, None).is_ok());
    world.get::<TilePos>(entity) == Some(&to)
}

/// BYOND `get_dir`: the direction whose bits match the signs of the offset; `None` on the same
/// tile or another z.
pub fn dir_between(from: TilePos, to: TilePos) -> Option<Dir> {
    if from.z != to.z {
        return None;
    }
    let (dx, dy) = ((to.x - from.x).signum(), (to.y - from.y).signum());
    Dir::DMI_ORDER.into_iter().find(|d| d.offset() == (dx, dy))
}

/// tg `Move` of a puller that moved `puller_from` -> `puller_to`, whose pulled thing is at
/// `pulled`: whether the pulled thing must step into `puller_from`. `second_diagonal_step`: the
/// step was the second half of a diagonal move (`moving_diagonally == SECOND_DIAG_STEP`).
pub fn pulled_follows(puller_to: TilePos, pulled: TilePos, second_diagonal_step: bool) -> bool {
    let Some(distance) = pulled.chebyshev(puller_to) else {
        return false;
    };
    let diagonal = pulled.x != puller_to.x && pulled.y != puller_to.y;
    (!second_diagonal_step && diagonal) || distance > 1
}

/// The pulled things follow their pullers' steps of this tick, in order.
// Cost: per tick, one lookup per `Moved` (a few per moving mob).
fn follow_pullers(
    world: &mut World,
    mut cursor: Local<MessageCursor<Moved>>,
    mut steps: Local<EntityHashMap<u32>>,
) {
    let moved: Vec<Moved> = cursor
        .read(world.resource::<Messages<Moved>>())
        .copied()
        .collect();
    steps.clear();
    for m in moved {
        let n = steps.entry(m.entity).or_insert(0);
        *n += 1;
        let second_diagonal_step = *n == 2;
        let Some(&Pulling(pulled)) = world.get::<Pulling>(m.entity) else {
            continue;
        };
        let Some(&at) = world.get::<TilePos>(pulled) else {
            continue;
        };
        if pulled_follows(m.to, at, second_diagonal_step) && at.chebyshev(m.from) == Some(1) {
            move_to_adjacent(world, pulled, m.from);
        }
    }
}

/// tg `check_pulling`: the pull ends at distance > 1, across z-levels, when the puller is off the
/// map or incapacitated (soft crit and worse: `TRAIT_HANDS_BLOCKED`; stuns, stamcrit and lying
/// down: `TRAIT_PULL_BLOCKED`), or the pulled thing left the map.
// Cost: per tick, one pass over pullers only.
fn check_pulls(
    pullers: Query<(
        Entity,
        &Pulling,
        Option<&TilePos>,
        Option<&Stat>,
        Option<&Incapacitation>,
        Option<&BodyPosition>,
    )>,
    positions: Query<&TilePos, Without<CarriedBy>>,
    mut commands: Commands,
) {
    for (puller, &Pulling(pulled), pos, stat, traits, posture) in &pullers {
        let near = pos
            .zip(positions.get(pulled).ok())
            .is_some_and(|(a, b)| a.chebyshev(*b).is_some_and(|d| d <= 1));
        let blocked = traits.is_some_and(|t| t.incapacitated) || posture.is_some_and(|b| b.lying());
        if !near || blocked || stat.is_some_and(|s| *s >= Stat::SoftCrit) {
            commands.entity(puller).try_remove::<Pulling>();
        }
    }
}

/// Grab state and slowdown go with the pull.
fn on_stop_pulling(remove: On<Remove<Pulling>>, mut commands: Commands) {
    if let Ok(mut puller) = commands.get_entity(remove.entity) {
        puller.try_remove::<GrabState>();
        puller.try_insert(PullSlowdown(0.0));
    }
}

fn on_unpulled(remove: On<Remove<PulledBy>>, mut commands: Commands) {
    if let Ok(mut pulled) = commands.get_entity(remove.entity) {
        pulled.try_remove::<(HeldByGrab, ResistCooldown)>();
    }
}

/// The client's living mob, if `client` still controls it.
fn own_mob(players: &Players, owners: &Query<&PlayerMob>, client: Entity) -> Option<Entity> {
    let mob = players.mob(client)?;
    (owners.get(mob).ok()?.client == client).then_some(mob)
}

fn stop_pulling_intents(
    mut intents: MessageReader<IntentReceived>,
    players: Res<Players>,
    owners: Query<&PlayerMob>,
    mut commands: Commands,
) {
    for intent in intents.read() {
        if matches!(intent.intent, ClientIntent::StopPulling)
            && let Some(mob) = own_mob(&players, &owners, intent.client)
        {
            commands.entity(mob).try_remove::<Pulling>();
        }
    }
}

/// tg `visible_message` with its self and target lines: `others` to every client in
/// `DEFAULT_MESSAGE_RANGE` of `at`, except the mobs in `personal`, who get their own line.
pub(crate) fn visible_message(
    world: &World,
    at: TilePos,
    channel: ChatChannel,
    others: (Span, String),
    personal: &[(Entity, Span, String)],
) -> Vec<SendChat> {
    let message = |span, text: &str| ChatMessage {
        channel,
        segments: vec![ChatSegment::new(span, text)],
    };
    let exclude: Vec<Entity> = personal.iter().map(|(p, ..)| *p).collect();
    let clients = crate::obj_damage::clients_in_range(world, at, DEFAULT_MESSAGE_RANGE, &exclude);
    let mut out = vec![SendChat {
        clients,
        message: message(others.0, &others.1),
    }];
    for (mob, span, text) in personal {
        if let Some(client) = world.get::<PlayerMob>(*mob).map(|p| p.client) {
            out.push(SendChat {
                clients: vec![client],
                message: message(*span, text),
            });
        }
    }
    out
}

fn name_of(world: &World, entity: Entity) -> String {
    if let Some(name) = world.get::<Name>(entity) {
        return name.as_str().to_owned();
    }
    world
        .get::<ss13_mapload::DmName>(entity)
        .map_or_else(|| "что-то".to_owned(), |n| n.name.clone())
}

/// tg `base_click_ctrl` -> `grab` (living targets: `grabbedby`) or `pulled`, checked again at the
/// moment of effect: a conscious puller off its click cooldown, the target adjacent (by
/// `crate::click`) and pullable.
fn ctrl_clicks(world: &mut World, mut clicks: Local<MessageCursor<CtrlClick>>) {
    let clicks: Vec<CtrlClick> = clicks
        .read(world.resource::<Messages<CtrlClick>>())
        .copied()
        .collect();
    let now = world.resource::<Time>().elapsed_secs_f64();
    for click in clicks {
        let (mob, target) = (click.mob, click.target);
        let Ok(entity) = world.get_entity(mob) else {
            continue;
        };
        if entity.get::<Stat>() != Some(&Stat::Conscious)
            || entity
                .get::<Incapacitation>()
                .is_some_and(|t| t.incapacitated)
            // `TRAIT_PULL_BLOCKED` of lying down.
            || entity.get::<BodyPosition>().is_some_and(|b| b.lying())
            || entity.get::<NextMove>().is_none_or(|n| now < n.0)
            || world.get::<TilePos>(target) != Some(&click.target_pos)
            || !can_be_pulled(world, mob, target)
        {
            continue;
        }
        let living = world.get::<Health>(target).is_some();
        let pulling = world.get::<Pulling>(mob).map(|p| p.0);
        let (user, victim) = (name_of(world, mob), name_of(world, target));
        let mut chat = Vec::new();
        if pulling == Some(target) {
            // tg `pulled`: pulling it already does nothing; `grabbedby` upgrades a living one.
            if !living || world.get::<GrabState>(mob) != Some(&GrabState::Passive) {
                continue;
            }
            world.entity_mut(mob).insert(GrabState::Aggressive);
            // tg `grippedby`: the victim lets go of what it pulls.
            world.entity_mut(target).remove::<Pulling>();
            chat = visible_message(
                world,
                click.target_pos,
                ChatChannel::Combat,
                (Span::Danger, format!("{user} агрессивно хватает {victim}!")),
                &[
                    (
                        target,
                        Span::BoldWarning,
                        format!("{user} агрессивно хватает вас!"),
                    ),
                    (
                        mob,
                        Span::Danger,
                        format!("Вы агрессивно хватаете {victim}!"),
                    ),
                ],
            );
        } else {
            if let Some(old) = world.get::<PulledBy>(target).map(PulledBy::puller)
                && living
            {
                let from = name_of(world, old);
                chat.extend(visible_message(
                    world,
                    click.target_pos,
                    ChatChannel::Combat,
                    (
                        Span::Danger,
                        format!("{user} оттаскивает {victim} из захвата {from}."),
                    ),
                    &[
                        (
                            target,
                            Span::Danger,
                            format!("{user} оттаскивает вас из захвата {from}"),
                        ),
                        (
                            mob,
                            Span::Notice,
                            format!("Вы оттаскиваете {victim} из захвата {from}"),
                        ),
                    ],
                ));
            }
            world
                .entity_mut(mob)
                .insert((Pulling(target), GrabState::Passive, PullSlowdown(0.0)));
            if living {
                chat.extend(visible_message(
                    world,
                    click.target_pos,
                    ChatChannel::Combat,
                    (Span::Warning, format!("{user} пассивно хватает {victim}!")),
                    &[
                        (
                            target,
                            Span::Warning,
                            format!("{user} пассивно хватает вас!"),
                        ),
                        (mob, Span::Notice, format!("Вы пассивно хватаете {victim}!")),
                    ],
                ));
            }
        }
        // `/mob/living/start_pulling` and `pulled` wait `CLICK_CD_GRABBING`; a grab of a mob is
        // followed by `changeNext_move(CLICK_CD_MELEE)` in `base_click_ctrl`.
        let cooldown = if living {
            CLICK_CD_MELEE
        } else {
            CLICK_CD_GRABBING
        };
        world.entity_mut(mob).insert(NextMove(now + cooldown));
        if let (Some(mut facing), Some(dir)) = (
            world.get_mut::<Facing>(mob),
            dir_between(click.mob_pos, click.target_pos),
        ) {
            facing.set_if_neq(Facing(dir));
        }
        for message in chat {
            world.write_message(message);
        }
    }
}

/// The odds of tg `resist_grab` against `grab` for a mob that is `lying` or not:
/// `None` = free at once (effective state below aggressive), else the escape chance in percent.
pub fn resist_chance(grab: GrabState, lying: bool) -> Option<f32> {
    let effective = grab.level() + u32::from(lying);
    (effective >= 1).then(|| (BASE_GRAB_RESIST_CHANCE / effective as f32).clamp(0.0, 100.0))
}

/// tg `resist_grab`: `true` when the mob is free (the pull stopped). Messages as BandaStation's.
fn resist_grab(world: &mut World, mob: Entity, moving: bool, now: f64) -> bool {
    let Some(puller) = world.get::<PulledBy>(mob).map(PulledBy::puller) else {
        return true;
    };
    if world.get::<Stat>(mob).is_some_and(|s| *s >= Stat::SoftCrit)
        || crate::status_effects::incapacitation(world, mob).incapacitated
    {
        return false;
    }
    let grab = world.get::<GrabState>(puller).copied().unwrap_or_default();
    let lying = world.get::<BodyPosition>(mob).is_some_and(|b| b.lying());
    let Some(chance) = resist_chance(grab, lying) else {
        world.entity_mut(puller).remove::<Pulling>();
        return true;
    };
    let escaped = world.resource_mut::<CombatRng>().chance(chance);
    let (victim, user) = (name_of(world, mob), name_of(world, puller));
    let at = world.get::<TilePos>(mob).copied().unwrap_or_default();
    let chat = if escaped {
        world.entity_mut(puller).remove::<Pulling>();
        visible_message(
            world,
            at,
            ChatChannel::Combat,
            (
                Span::Danger,
                format!("{victim} вырывается из хватки {user}!"),
            ),
            &[
                (
                    mob,
                    Span::Danger,
                    format!("Вы вырываетесь из хватки {user}!"),
                ),
                (
                    puller,
                    Span::Warning,
                    format!("{victim} вырывается из вашей хватки!"),
                ),
            ],
        )
    } else {
        // `damage_on_resist_fail`: `rand(7, 13)` stamina.
        let damage = world.resource_mut::<CombatRng>().range(7, 13) as f32;
        crate::status_effects::adjust_stamina(world, mob, damage);
        if moving {
            world
                .entity_mut(mob)
                .insert(ResistCooldown(now + MOVING_RESIST_FAIL_SECS));
        }
        visible_message(
            world,
            at,
            ChatChannel::Combat,
            (
                Span::Danger,
                format!("{victim} неуспешно пытается вырваться из хватки {user}!"),
            ),
            &[
                (
                    mob,
                    Span::Warning,
                    format!("Вы неуспешно пытаетесь вырваться из хватки {user}!"),
                ),
                (
                    puller,
                    Span::Danger,
                    format!("{victim} неуспешно пытается вырваться из вашей хватки!"),
                ),
            ],
        )
    };
    for message in chat {
        world.write_message(message);
    }
    escaped
}

/// tg `execute_resist`: off the click cooldown, `CLICK_CD_RESIST`, then `resist_grab` if pulled.
fn resist_intents(world: &mut World, mut intents: Local<MessageCursor<IntentReceived>>) {
    let clients: Vec<Entity> = intents
        .read(world.resource::<Messages<IntentReceived>>())
        .filter(|i| matches!(i.intent, ClientIntent::Resist))
        .map(|i| i.client)
        .collect();
    let now = world.resource::<Time>().elapsed_secs_f64();
    for client in clients {
        let Some(mob) = world.resource::<Players>().mob(client) else {
            continue;
        };
        let Ok(entity) = world.get_entity(mob) else {
            continue;
        };
        let traits = entity.get::<Incapacitation>().copied().unwrap_or_default();
        // `can_resist`: off cooldown, not incapacitated.
        if entity.get::<PlayerMob>().map(|p| p.client) != Some(client)
            || entity.get::<NextMove>().is_none_or(|n| now < n.0)
            || entity.get::<Stat>().is_none_or(|s| *s >= Stat::SoftCrit)
            || traits.incapacitated
        {
            continue;
        }
        world
            .entity_mut(mob)
            .insert(NextMove(now + CLICK_CD_RESIST));
        if !traits.restrained && world.get::<PulledBy>(mob).is_some() {
            resist_grab(world, mob, false, now);
        } else if !traits.immobilized && crate::fire::on_fire(world, mob) {
            // tg `execute_resist`: able to move and on fire -> stop, drop and roll.
            crate::fire::resist_fire(world, mob);
        } else if traits.restrained && !traits.immobilized {
            // `mobility_flags & MOBILITY_MOVE` -> `resist_restraints`.
            crate::restraints::resist_restraints(world, client, mob);
        }
    }
}

/// tg `Process_Grab` for the steps `crate::server` held back: an incapacitated mob waits 1 s, a
/// failed resist 4 s; an escape lets the step through in the same tick.
fn move_resists(world: &mut World, mut resists: Local<MessageCursor<MoveResist>>) {
    let resists: Vec<MoveResist> = resists
        .read(world.resource::<Messages<MoveResist>>())
        .copied()
        .collect();
    let now = world.resource::<Time>().elapsed_secs_f64();
    for MoveResist { mob, dir } in resists {
        let Ok(entity) = world.get_entity(mob) else {
            continue;
        };
        if entity.get::<PulledBy>().is_none() {
            world.write_message(MoveRequest { entity: mob, dir });
            continue;
        }
        if entity.get::<ResistCooldown>().is_some_and(|c| now < c.0) {
            continue;
        }
        let traits = entity.get::<Incapacitation>().copied().unwrap_or_default();
        if entity.get::<Stat>().is_some_and(|s| *s >= Stat::SoftCrit) || traits.incapacitated {
            world
                .entity_mut(mob)
                .insert(ResistCooldown(now + INCAPACITATED_GRAB_SECS));
            continue;
        }
        if traits.restrained {
            // `Process_Grab` of a restrained mob.
            world
                .entity_mut(mob)
                .insert(ResistCooldown(now + INCAPACITATED_GRAB_SECS));
            if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
                crate::tools::tell(
                    world,
                    client,
                    Span::Warning,
                    "Вы сдержаны! Вы не можете двигаться!",
                );
            }
            continue;
        }
        if resist_grab(world, mob, true, now) {
            world.write_message(MoveRequest { entity: mob, dir });
        }
    }
}

/// tg `update_pull_movespeed`: a passively pulled lying mob or a closet (not a crate) slows the
/// puller.
// Cost: per tick, one pass over pullers only.
fn pull_slowdown(
    mut pullers: Query<(&Pulling, &GrabState, &mut PullSlowdown)>,
    bodies: Query<&BodyPosition>,
    closets: Query<&DmCloset>,
) {
    for (&Pulling(pulled), grab, mut slowdown) in &mut pullers {
        let drag = if let Ok(body) = bodies.get(pulled) {
            if body.lying() && *grab < GrabState::Aggressive {
                PULL_PRONE_SLOWDOWN
            } else {
                0.0
            }
        } else if closets.get(pulled).is_ok_and(|c| !c.is_crate) {
            PULL_PRONE_SLOWDOWN
        } else {
            0.0
        };
        slowdown.set_if_neq(PullSlowdown(drag));
    }
}

/// [`HeldByGrab`] on pulled mobs whose steps need a resist.
// Cost: per tick, one pass over pulled mobs only.
fn mark_held(
    pulled: Query<(
        Entity,
        &PulledBy,
        &Stat,
        &BodyPosition,
        Has<HeldByGrab>,
        Option<&Incapacitation>,
    )>,
    grabs: Query<&GrabState>,
    mut commands: Commands,
) {
    for (mob, by, stat, body, held, traits) in &pulled {
        let grab = grabs.get(by.puller()).copied().unwrap_or_default();
        let traits = traits.copied().unwrap_or_default();
        let hold = grab >= GrabState::Aggressive
            || body.lying()
            || *stat >= Stat::SoftCrit
            || traits.incapacitated
            || traits.restrained;
        if hold && !held {
            commands.entity(mob).insert(HeldByGrab);
        } else if !hold && held {
            commands.entity(mob).remove::<HeldByGrab>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn at(x: i32, y: i32) -> TilePos {
        TilePos::new(x, y, 1)
    }

    #[test]
    fn pulled_follows_tg_move_rules() {
        // In line behind: the puller steps away, distance 2.
        assert!(pulled_follows(at(6, 5), at(4, 5), false));
        // Sideways from a pulled thing behind: diagonal now, follows.
        assert!(pulled_follows(at(5, 6), at(4, 5), false));
        // The second half of a diagonal step leaves it diagonal: stays.
        assert!(!pulled_follows(at(6, 6), at(5, 5), true));
        // Still cardinally adjacent (the puller stepped around it): stays.
        assert!(!pulled_follows(at(5, 6), at(5, 5), false));
        // The puller stepped onto its tile (a lying body): stays.
        assert!(!pulled_follows(at(4, 5), at(4, 5), false));
        // Another z-level: never.
        assert!(!pulled_follows(TilePos::new(5, 5, 2), at(4, 5), false));
    }

    #[test]
    fn resist_odds_follow_tg() {
        assert_eq!(resist_chance(GrabState::Passive, false), None);
        assert_eq!(resist_chance(GrabState::Passive, true), Some(60.0));
        assert_eq!(resist_chance(GrabState::Aggressive, false), Some(60.0));
        assert_eq!(resist_chance(GrabState::Aggressive, true), Some(30.0));
    }

    /// A puller walks east, north-east and back west with a body behind it; the body keeps one
    /// tile behind, lies diagonal after the diagonal step and is never lost.
    #[test]
    fn a_dragged_body_follows_through_steps_and_diagonals() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ss13_world::WorldPlugin))
            .add_systems(
                Update,
                (follow_pullers, check_pulls)
                    .chain()
                    .after(WorldSystems::Movement),
            );
        app.world_mut()
            .resource_mut::<SpatialIndex>()
            .create_level(1, 20, 20)
            .unwrap();
        let world = app.world_mut();
        let body = world.spawn(at(4, 5)).id();
        let puller = world
            .spawn((at(5, 5), ss13_world::Mover::new(0.0), ss13_world::Density))
            .id();
        world.entity_mut(puller).insert(Pulling(body));
        let walk = |app: &mut App, dir| {
            app.world_mut().write_message(MoveRequest {
                entity: puller,
                dir,
            });
            app.update();
            let w = app.world();
            (
                *w.get::<TilePos>(puller).unwrap(),
                *w.get::<TilePos>(body).unwrap(),
            )
        };
        assert_eq!(walk(&mut app, Dir::East), (at(6, 5), at(5, 5)));
        assert_eq!(walk(&mut app, Dir::East), (at(7, 5), at(6, 5)));
        // Diagonal: the first half (north) drags it into (7, 5), the second half leaves it there.
        assert_eq!(walk(&mut app, Dir::NorthEast), (at(8, 6), at(7, 5)));
        // West: it is right below now, cardinally adjacent, and stays.
        assert_eq!(walk(&mut app, Dir::West), (at(7, 6), at(7, 5)));
        // East again: diagonal behind, it steps into the tile the puller left.
        assert_eq!(walk(&mut app, Dir::East), (at(8, 6), at(7, 6)));
        assert_eq!(app.world().get::<Pulling>(puller), Some(&Pulling(body)));
        assert_eq!(
            app.world().get::<PulledBy>(body).map(PulledBy::puller),
            Some(puller)
        );
        // Moved away by something else (pushed, picked up): the pull breaks.
        app.world_mut().entity_mut(body).insert(at(12, 6));
        app.update();
        assert!(app.world().get::<Pulling>(puller).is_none(), "pull broke");
        assert!(app.world().get::<PulledBy>(body).is_none());
    }
}
