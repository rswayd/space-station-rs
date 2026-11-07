//! Magic traps and tripwires (server), after tg `code/game/objects/structures/traps.dm` and
//! `tripwire.dm`.
//!
//! - Traps ([`Trap`]): faint (alpha 30) until they go off. Something stepping on one (not an
//!   effect or a ghost) makes it flare: "... вспыхивает!", sparks, alpha 200, then it fades back
//!   over its cooldown (1 min; the divine ward 2 min) and ignores steps meanwhile. A living
//!   victim gets the trap's effect: shock (30 shock damage through gloves, 10 s paralysis),
//!   flame (2 s paralysis, a hotspot), frost (unless cold resistant: 2 s paralysis, -300 K),
//!   earth (10 s paralysis, 35 brute, a rock for 20 s), unholy (10 shock, 2 s paralysis, two
//!   hostile proteons, the trap gone after 3 s). A living examiner within a tile reveals it
//!   ("Вы обнаруживаете ...!") and sets it off without the effect. The divine ward is dense and
//!   vanishes after 2 min.
//! - Tripwires ([`TripwirePost`]): a sturdy cable tied around an anchored stanchion and then
//!   used on another one in a straight line up to 4 tiles away (`distance × 3 s`) strings
//!   heavy cables between them (a blocked tile in between refuses); a cable used on a post too
//!   far away or on another level ties it there instead. Wired posts and cables are dense to
//!   running movers (walking ones pass, `ss13_world::WalkPassable`). Wirecutters (5 s) on a
//!   wired post or on a cable cut the whole wire (20 cable coil); a wrench anchors and
//!   unanchors an unwired post; using the cable on itself recoils it.
// shortcut: no antimagic (`can_block_magic`) or immune minds, no freon status of the frost
// trap, no bounty trap (`/obj/item/bountytrap`, fugitives), the fade back is a step at the end
// of the cooldown instead of an animation. Tripwires let lying carbons and movers along the
// wire through only as `WalkPassable` allows (walking or unconscious), and do not topple mechs
// (`fall_and_crush`, moved to the mecha group); the sturdy cable's crafting recipe and its
// wirecutter act are not ported. Add each with its mechanic.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmItem, DmObj};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Dir, Facing, TilePos};
use ss13_world::{Density, Moved, SpatialIndex, WalkPassable};

use crate::click::ExamineClick;
use crate::construction::Job;
use crate::health::{DamageType, Health};
use crate::medical::{name_of, visible};
use crate::obj_damage::{Anchored, is_under, set_anchored, set_icon_state};
use crate::status_effects::Incap;
use crate::tools::{Actor, DoAfter, ToolKind, do_after, tell, tool_of, use_tool};

pub const TRAP: &str = "/obj/structure/trap";
pub const TRIPWIRE_POST: &str = "/obj/structure/tripwire/post";
pub const TRIPWIRE_CABLE: &str = "/obj/structure/tripwire/cable";
pub const TRIPWIRE_ITEM: &str = "/obj/item/tripwire_cable";
const CABLE_COIL: &str = "/obj/item/stack/cable_coil";
/// `/obj/structure/flora/rock/style_random` of the earth trap
/// (`ss13_mapload::runtime::GAME_RUNTIME_TYPES`).
const ROCK: &str = "/obj/structure/flora/rock/style_random";
/// `/mob/living/basic/construct/proteon/hostile` of the unholy trap.
const PROTEON: &str = "/mob/living/basic/construct/proteon/hostile";
/// `initial(alpha)` of a trap, and the flared one.
const HIDDEN_ALPHA: u8 = 30;
const FLARED_ALPHA: u8 = 200;
/// `time_between_triggers` (1 MINUTES; the ward's 2 MINUTES).
const COOLDOWN: f64 = 60.0;
const WARD_COOLDOWN: f64 = 120.0;
/// `T0C + 100`: `FIRE_MINIMUM_TEMPERATURE_TO_EXIST` of a new `/obj/effect/hotspot`.
const HOTSPOT_TEMP: f64 = 373.15;
const HOTSPOT_VOLUME: f64 = 125.0;
/// The longest wire between two posts, and the distance past which a cable ties instead.
const MAX_WIRE: u32 = 4;
const TIE_DISTANCE: u32 = 6;
/// `use_tool(src, user, 5 SECONDS)` of the wirecutters.
const CUT_DS: f32 = 50.0;
/// `do_after(user, distance_between * 3 SECONDS)` per tile.
const WIRE_DS_PER_TILE: f32 = 30.0;
const CUT_COIL: u32 = 20;

/// What a trap does to its victim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrapKind {
    /// The base type and the divine ward: only the flare.
    Plain,
    Shock,
    Fire,
    Frost,
    Earth,
    Cult,
}

/// `/obj/structure/trap`: `last_trigger`, `time_between_triggers`, `charges`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Trap {
    pub kind: TrapKind,
    pub cooldown: f64,
    /// When it last flared (`Time::elapsed_secs_f64`); `None`: never.
    pub last_trigger: Option<f64>,
    /// Flares left (`charges`); `u32::MAX`: `INFINITY`.
    pub charges: u32,
}

/// A thing that is deleted at the given moment (tg `QDEL_IN`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct DeleteAt(pub f64);

/// `/obj/structure/tripwire/post`: `opposing_post` and the `tripwires` this post owns (only one
/// post of a pair has them).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct TripwirePost {
    pub opposing: Option<Entity>,
    pub wires: Vec<Entity>,
}

/// `/obj/item/tripwire_cable`: `connecting_post`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TripwireCable {
    pub post: Option<Entity>,
}

/// A heavy cable of a tripwire: the post that owns it.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct TripwireSegment {
    pub owner: Entity,
}

/// The timed steps of this module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrapJob {
    /// The sturdy cable strung from the target post to `to`.
    Wire { to: Entity, to_pos: TilePos },
    /// Wirecutters on a wired post or a cable.
    Cut,
}

pub struct TrapsPlugin;

impl Plugin for TrapsPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::HeldOnTarget, 3439, |w, c| {
            c.held
                .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target))
        });
        app.add_click_handler(ClickPhase::AttackSelf, 3259, |w, c| {
            attack_self(w, c.client, c.target)
        });
        app.add_observer(add_trap).add_systems(
            Update,
            (stepped_on, examined, timers).after(crate::click::ResolveClicks),
        );
    }
}

fn kind_of(path: &str) -> TrapKind {
    const KINDS: [(&str, TrapKind); 5] = [
        ("/obj/structure/trap/stun", TrapKind::Shock),
        ("/obj/structure/trap/fire", TrapKind::Fire),
        ("/obj/structure/trap/chill", TrapKind::Frost),
        ("/obj/structure/trap/damage", TrapKind::Earth),
        ("/obj/structure/trap/cult", TrapKind::Cult),
    ];
    KINDS
        .iter()
        .find(|(root, _)| is_under(path, root))
        .map_or(TrapKind::Plain, |&(_, k)| k)
}

/// `trap/Initialize` (and the ward's `QDEL_IN`), the sturdy cable's and posts' state.
fn add_trap(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    let Ok(dm) = objs.get(add.entity) else {
        return;
    };
    let entity = add.entity;
    if is_under(&dm.path, TRIPWIRE_ITEM) {
        commands.entity(entity).insert(TripwireCable::default());
        return;
    }
    if is_under(&dm.path, TRIPWIRE_POST) {
        commands.entity(entity).insert(TripwirePost::default());
        // `CanAllowThrough`: an unwired post lets everything through.
        commands.queue(move |world: &mut World| {
            world.entity_mut(entity).remove::<Density>();
        });
        return;
    }
    if !is_under(&dm.path, TRAP) {
        return;
    }
    // The bounty trap lives inside its item (not ported).
    if is_under(&dm.path, "/obj/structure/trap/stun/hunter") {
        return;
    }
    let ward = is_under(&dm.path, "/obj/structure/trap/ward");
    let cooldown = if ward { WARD_COOLDOWN } else { COOLDOWN };
    commands.entity(entity).insert(Trap {
        kind: kind_of(&dm.path),
        cooldown,
        last_trigger: None,
        charges: u32::MAX,
    });
    commands.queue(move |world: &mut World| {
        set_alpha(world, entity, HIDDEN_ALPHA);
        if ward {
            let now = world.resource::<Time>().elapsed_secs_f64();
            world
                .entity_mut(entity)
                .insert(DeleteAt(now + WARD_COOLDOWN));
        }
    });
}

fn set_alpha(world: &mut World, e: Entity, alpha: u8) {
    if let Some(mut look) = world.get_mut::<Appearance>(e) {
        let [r, g, b, _] = look.color.unwrap_or([255, 255, 255, 255]);
        let color = Some([r, g, b, alpha]);
        if look.color != color {
            look.color = color;
        }
    }
}

/// Whether a trap with this state ignores a step at `now` (`last_trigger +
/// time_between_triggers > world.time`).
pub fn cooling_down(trap: &Trap, now: f64) -> bool {
    trap.last_trigger.is_some_and(|t| t + trap.cooldown > now)
}

/// `trap/flare`: the message, sparks, visible; the last charge deletes it after 1 s.
fn flare(world: &mut World, trap: Entity) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(mut t) = world.get::<Trap>(trap).copied() else {
        return;
    };
    let name = crate::cooking::capitalize(&name_of(world, trap));
    visible(
        world,
        trap,
        "",
        &format!("{name} ярко вспыхивает!"),
        Span::Warning,
    );
    if let Some(&pos) = world.get::<TilePos>(trap) {
        crate::effects::particles::do_sparks(world, 4, true, pos);
    }
    set_alpha(world, trap, FLARED_ALPHA);
    t.last_trigger = Some(now);
    if t.charges != u32::MAX {
        t.charges = t.charges.saturating_sub(1);
        if t.charges == 0 {
            world.entity_mut(trap).insert(DeleteAt(now + 1.0));
        }
    }
    world.entity_mut(trap).insert(t);
}

/// `trap/on_entered` of the steps taken this tick.
// Cost: (steps this tick) x (things on the tile entered), a handful.
fn stepped_on(world: &mut World, mut cursor: Local<MessageCursor<Moved>>) {
    let moves: Vec<(Entity, TilePos)> = cursor
        .read(world.resource::<Messages<Moved>>())
        .map(|m| (m.entity, m.to))
        .collect();
    if moves.is_empty() {
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    for (mover, to) in moves {
        let trap = world
            .resource::<SpatialIndex>()
            .on_tile(to)
            .iter()
            .copied()
            .find(|&e| e != mover && world.get::<Trap>(e).is_some());
        let Some(trap) = trap else {
            continue;
        };
        let Some(t) = world.get::<Trap>(trap).copied() else {
            continue;
        };
        if cooling_down(&t, now) || t.charges == 0 {
            continue;
        }
        // `ignore_typecache`: effects and ghosts.
        let effect = world
            .get::<DmObj>(mover)
            .is_some_and(|o| is_under(&o.path, "/obj/effect"));
        if effect || world.get::<crate::ghost::Ghost>(mover).is_some() {
            continue;
        }
        flare(world, trap);
        if world.get::<Health>(mover).is_some() {
            trap_effect(world, trap, t.kind, mover);
        }
    }
}

/// `trap_effect` of each kind.
fn trap_effect(world: &mut World, trap: Entity, kind: TrapKind, victim: Entity) {
    let Some(&pos) = world.get::<TilePos>(trap) else {
        return;
    };
    let client = world
        .get::<crate::server::PlayerMob>(victim)
        .map(|p| p.client);
    let say = |world: &mut World, text: &str| {
        if let Some(client) = client {
            tell(world, client, Span::Danger, text);
        }
    };
    let paralyze = |world: &mut World, secs: f32| {
        crate::status_effects::apply(world, victim, Incap::Paralyze, secs);
    };
    match kind {
        TrapKind::Plain => {}
        TrapKind::Shock => {
            // `SHOCK_NOGLOVES`: gloves do not help.
            crate::wires::electrocute_act(world, victim, 30.0, trap, 1.0);
            paralyze(world, 10.0);
        }
        TrapKind::Fire => {
            say(world, "Самовозгорание!");
            paralyze(world, 2.0);
            if let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() {
                atmos.hotspot_expose(pos, HOTSPOT_TEMP, HOTSPOT_VOLUME, true);
            }
            // The hotspot's `perform_exposure` on what stands in it.
            crate::fire::fire_act(world, victim, HOTSPOT_TEMP);
        }
        TrapKind::Frost => {
            if crate::species::has_trait(world, victim, crate::species::Trait::ResistCold) {
                return;
            }
            say(world, "Вас сковывает лютый холод!");
            paralyze(world, 2.0);
            crate::medical::diseases::adjust_bodytemperature(world, victim, -300.0, 0.0);
        }
        TrapKind::Earth => {
            say(world, "Земля содрогается у вас под ногами!");
            paralyze(world, 10.0);
            crate::mobs::lavaland::hurt(world, victim, DamageType::Brute, 35.0, None);
            if let Some(rock) = crate::construction::spawn_obj(world, ROCK, pos) {
                let now = world.resource::<Time>().elapsed_secs_f64();
                world.entity_mut(rock).insert(DeleteAt(now + 20.0));
            }
        }
        TrapKind::Cult => {
            say(
                world,
                "С треском враждебные конструкты выходят из укрытия, оглушая вас!",
            );
            crate::wires::electrocute_act(world, victim, 10.0, trap, 1.0);
            paralyze(world, 2.0);
            for _ in 0..2 {
                crate::mobs::spawn::spawn_mob(world, PROTEON, pos);
            }
            let now = world.resource::<Time>().elapsed_secs_f64();
            world.entity_mut(trap).insert(DeleteAt(now + 3.0));
        }
    }
}

/// `trap/examine`: a living examiner next to it reveals it.
fn examined(world: &mut World, mut cursor: Local<MessageCursor<ExamineClick>>) {
    let clicks: Vec<ExamineClick> = cursor
        .read(world.resource::<Messages<ExamineClick>>())
        .copied()
        .collect();
    for click in clicks {
        if world.get::<Trap>(click.target).is_none()
            || world.get::<Health>(click.examiner).is_none()
        {
            continue;
        }
        let (Some(&at), Some(&trap)) = (
            world.get::<TilePos>(click.examiner),
            world.get::<TilePos>(click.target),
        ) else {
            continue;
        };
        if at.chebyshev(trap).is_none_or(|d| d > 1) {
            continue;
        }
        let name = name_of(world, click.target);
        tell(
            world,
            click.client,
            Span::Notice,
            format!("Вы обнаруживаете {name}!"),
        );
        flare(world, click.target);
    }
}

/// The fade back of flared traps and [`DeleteAt`] deletions.
// Cost: per tick, the traps and timed deletions on the map (a handful).
fn timers(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let faded: Vec<Entity> = world
        .query::<(Entity, &Trap)>()
        .iter(world)
        .filter(|(_, t)| t.last_trigger.is_some() && !cooling_down(t, now))
        .map(|(e, _)| e)
        .collect();
    for trap in faded {
        set_alpha(world, trap, HIDDEN_ALPHA);
        if let Some(mut t) = world.get_mut::<Trap>(trap) {
            t.last_trigger = None;
        }
    }
    let due: Vec<Entity> = world
        .query::<(Entity, &DeleteAt)>()
        .iter(world)
        .filter(|(_, d)| d.0 <= now)
        .map(|(e, _)| e)
        .collect();
    for e in due {
        world.despawn(e);
    }
}

/// The tiles a wire from `from` to `to` crosses, with each cable's icon state and direction;
/// `None` unless they are in a straight line 1..=[`MAX_WIRE`] apart (tg `item_interaction`'s
/// checks; two adjacent posts need no cable).
pub fn wire_layout(from: TilePos, to: TilePos) -> Option<Vec<(TilePos, &'static str, Dir)>> {
    let distance = from.chebyshev(to)?;
    if distance == 0 || distance > MAX_WIRE || (from.x != to.x && from.y != to.y) {
        return None;
    }
    let dir = Dir::CARDINALS.into_iter().find(|d| {
        let (dx, dy) = d.offset();
        (to.x - from.x).signum() == dx && (to.y - from.y).signum() == dy
    })?;
    let mut tiles = Vec::new();
    let mut at = from;
    for step in 1..distance {
        at = at.step(dir);
        let state = match distance {
            2 => "tripwire_1",
            3 => "tripwire_2",
            _ if step == 2 => "tripwire_3",
            _ => "tripwire_2",
        };
        // `distance_extended > distance_between / 2` (integer `step`, real half).
        let facing = if f64::from(step) > f64::from(distance) / 2.0 {
            dir
        } else {
            dir.reverse()
        };
        tiles.push((at, state, facing));
    }
    Some(tiles)
}

fn anchored(world: &World, e: Entity) -> bool {
    world.get::<Anchored>(e).is_some_and(|a| a.0)
}

/// `post/clear_wire`: both posts unwired, every cable gone.
pub fn clear_wire(world: &mut World, post: Entity) {
    let Some(p) = world.get::<TripwirePost>(post).cloned() else {
        return;
    };
    world.entity_mut(post).insert(TripwirePost::default());
    world.entity_mut(post).remove::<(Density, WalkPassable)>();
    set_icon_state(world, post, "tripwire_post");
    for wire in p.wires {
        if world.get_entity(wire).is_ok() {
            world.despawn(wire);
        }
    }
    if let Some(other) = p.opposing
        && world.get::<TripwirePost>(other).is_some()
    {
        clear_wire(world, other);
    }
}

/// `connect_to_post`: wired, facing the other post, dense to runners.
fn connect(world: &mut World, post: Entity, other: Entity, dir: Dir, wires: Vec<Entity>) {
    world.entity_mut(post).insert((
        TripwirePost {
            opposing: Some(other),
            wires,
        },
        Facing(dir),
        Density,
        WalkPassable,
    ));
    set_icon_state(world, post, "tripwire_post_tied");
}

/// The sturdy cable's `attack_self`: recoiled.
fn attack_self(world: &mut World, client: Entity, item: Entity) -> bool {
    let Some(cable) = world.get::<TripwireCable>(item).copied() else {
        return false;
    };
    if cable.post.is_none_or(|p| world.get_entity(p).is_err()) {
        return false;
    }
    world.entity_mut(item).insert(TripwireCable { post: None });
    tell(world, client, Span::Notice, "Вы сматываете кабель обратно.");
    true
}

/// Wirecutters and wrenches on posts and cables, the sturdy cable on a post.
fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let post = world.get::<TripwirePost>(target).cloned();
    let segment = world.get::<TripwireSegment>(target).is_some();
    if post.is_none() && !segment {
        return false;
    }
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    let name = name_of(world, target);
    match tool_of(world, held).map(|(t, _)| t) {
        Some(ToolKind::Wirecutters) => {
            if post.as_ref().is_some_and(|p| p.opposing.is_none()) {
                return false;
            }
            let who = crate::cooking::capitalize(&name_of(world, mob));
            let (own, others) = (
                format!("Вы начинаете перерезать {name}..."),
                format!("{who} начинает перерезать {name}..."),
            );
            visible(world, mob, &own, &others, Span::Notice);
            use_tool(world, actor, target, CUT_DS, 0, Job::Trap(TrapJob::Cut));
            return true;
        }
        Some(ToolKind::Wrench) if post.is_some() => {
            if post.as_ref().is_some_and(|p| p.opposing.is_some()) {
                tell(world, client, Span::Warning, "сначала снимите растяжку!");
                return true;
            }
            let to = !anchored(world, target);
            set_anchored(world, target, to);
            tell(
                world,
                client,
                Span::Notice,
                if to {
                    "закреплено"
                } else {
                    "откреплено"
                },
            );
            return true;
        }
        _ => {}
    }
    let Some(post) = post else {
        return false;
    };
    let Some(cable) = world.get::<TripwireCable>(held).copied() else {
        return false;
    };
    if !anchored(world, target) || post.opposing.is_some() {
        return false;
    }
    let here = world.get::<TilePos>(target).copied();
    let other = cable
        .post
        .filter(|&p| world.get::<TripwirePost>(p).is_some());
    let other_pos = other.and_then(|p| world.get::<TilePos>(p).copied());
    let distance = here.zip(other_pos).and_then(|(a, b)| a.chebyshev(b));
    let (Some(other), Some(other_pos), Some(distance)) = (other, other_pos, distance) else {
        return tie(world, client, held, target);
    };
    if distance > TIE_DISTANCE {
        return tie(world, client, held, target);
    }
    if other == target || !anchored(world, other) {
        return true;
    }
    let text = if distance > MAX_WIRE {
        "Столб на другом конце слишком далеко!"
    } else if distance == 0 {
        "Нет смысла их соединять, они и так рядом."
    } else if here.is_some_and(|h| h.x != other_pos.x && h.y != other_pos.y) {
        "Под углом растяжка будет куда менее эффективна."
    } else {
        do_after(
            world,
            actor,
            target,
            WIRE_DS_PER_TILE * distance as f32,
            0,
            Job::Trap(TrapJob::Wire {
                to: other,
                to_pos: other_pos,
            }),
        );
        return true;
    };
    tell(world, client, Span::Notice, text);
    true
}

fn tie(world: &mut World, client: Entity, cable: Entity, post: Entity) -> bool {
    world
        .entity_mut(cable)
        .insert(TripwireCable { post: Some(post) });
    let (what, name) = (name_of(world, cable), name_of(world, post));
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы обвязываете один конец {what} вокруг {name}."),
    );
    true
}

/// [`TrapJob`] at the end of its `do_after`.
pub fn complete(world: &mut World, _mob: Entity, action: &DoAfter, job: TrapJob) {
    let (client, target) = (action.client, action.target);
    match job {
        TrapJob::Cut => {
            let owner = if let Some(seg) = world.get::<TripwireSegment>(target) {
                Some(seg.owner)
            } else {
                world
                    .get::<TripwirePost>(target)
                    .and_then(|p| p.opposing)
                    .map(|_| target)
            };
            let Some(owner) = owner else {
                return;
            };
            tell(world, client, Span::Notice, "Вы перерезаете растяжку.");
            crate::construction::spawn_stack(world, CABLE_COIL, action.target_pos, CUT_COIL);
            // A cut cable's `wire_qdeleted` clears the whole wire.
            clear_wire(world, owner);
        }
        TrapJob::Wire { to, to_pos } => {
            let (Some(post), Some(other)) = (
                world.get::<TripwirePost>(target).cloned(),
                world.get::<TripwirePost>(to).cloned(),
            ) else {
                return;
            };
            if !anchored(world, target)
                || !anchored(world, to)
                || post.opposing.is_some()
                || other.opposing.is_some()
                || world.get::<TilePos>(to) != Some(&to_pos)
            {
                return;
            }
            let Some(layout) = wire_layout(action.target_pos, to_pos) else {
                return;
            };
            let blocked = layout.iter().any(|&(pos, ..)| {
                world
                    .resource::<SpatialIndex>()
                    .on_tile(pos)
                    .iter()
                    .any(|&e| world.get::<Density>(e).is_some() && world.get::<Health>(e).is_none())
            });
            if blocked {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Кабель не протянуть сквозь твёрдые предметы.",
                );
                return;
            }
            let mut wires = Vec::new();
            for (pos, state, facing) in layout {
                let Some(wire) = crate::construction::spawn_obj(world, TRIPWIRE_CABLE, pos) else {
                    continue;
                };
                set_icon_state(world, wire, state);
                world.entity_mut(wire).insert((
                    TripwireSegment { owner: target },
                    Facing(facing),
                    Density,
                    WalkPassable,
                ));
                wires.push(wire);
            }
            let dir = Dir::CARDINALS
                .into_iter()
                .find(|&d| {
                    action.target_pos.step(d).chebyshev(to_pos)
                        < action.target_pos.chebyshev(to_pos)
                })
                .unwrap_or(Dir::South);
            connect(world, target, to, dir, wires);
            connect(world, to, target, dir.reverse(), Vec::new());
            if let Some(item) = action.item
                && world.get::<DmItem>(item).is_some()
            {
                world.despawn(item);
            }
            tell(
                world,
                client,
                Span::Notice,
                "Вы успешно соединяете два столба.",
            );
        }
    }
}

/// What [`crate::obj_damage`] tells the post's deletion: its wire goes too (`post/Destroy`).
pub(crate) fn deconstruct(world: &mut World, target: Entity) {
    if world.get::<TripwirePost>(target).is_some() {
        clear_wire(world, target);
    } else if let Some(seg) = world.get::<TripwireSegment>(target).copied() {
        clear_wire(world, seg.owner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trap_kinds_follow_their_subtype() {
        assert_eq!(kind_of("/obj/structure/trap/stun"), TrapKind::Shock);
        assert_eq!(kind_of("/obj/structure/trap/chill"), TrapKind::Frost);
        assert_eq!(kind_of("/obj/structure/trap/damage"), TrapKind::Earth);
        assert_eq!(kind_of("/obj/structure/trap/ward"), TrapKind::Plain);
        assert_eq!(kind_of("/obj/structure/trap"), TrapKind::Plain);
    }

    #[test]
    fn a_flared_trap_ignores_steps_until_its_cooldown_ends() {
        let mut trap = Trap {
            kind: TrapKind::Fire,
            cooldown: COOLDOWN,
            last_trigger: None,
            charges: u32::MAX,
        };
        assert!(!cooling_down(&trap, 5.0));
        trap.last_trigger = Some(10.0);
        assert!(cooling_down(&trap, 69.0));
        assert!(!cooling_down(&trap, 70.0));
    }

    #[test]
    fn wires_run_straight_up_to_four_tiles() {
        let a = TilePos::new(1, 1, 1);
        assert_eq!(wire_layout(a, TilePos::new(2, 1, 1)), Some(Vec::new()));
        assert_eq!(wire_layout(a, a), None);
        assert_eq!(wire_layout(a, TilePos::new(3, 3, 1)), None);
        assert_eq!(wire_layout(a, TilePos::new(6, 1, 1)), None);
        assert_eq!(
            wire_layout(a, TilePos::new(3, 1, 1)),
            Some(vec![(TilePos::new(2, 1, 1), "tripwire_1", Dir::West)])
        );
        let four = wire_layout(a, TilePos::new(1, 5, 1)).unwrap();
        let states: Vec<&str> = four.iter().map(|&(_, s, _)| s).collect();
        assert_eq!(states, ["tripwire_2", "tripwire_3", "tripwire_2"]);
        let dirs: Vec<Dir> = four.iter().map(|&(.., d)| d).collect();
        assert_eq!(dirs, [Dir::South, Dir::South, Dir::North]);
    }
}
