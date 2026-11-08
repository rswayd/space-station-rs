//! World clicks (server): every `ClientIntent::Click` is validated once here and handed to the
//! systems that react to clicks (doors, inventory) as [`ClickResolved`].
//!
//! tg `ClickOn` dispatches modifiers first: a shift+middle click points ([`PointClick`]), an
//! alt+shift click opens the loot panel and a shift-click examines ([`ExamineClick`]: any entity
//! the client sees, also from crit or as a ghost), a middle click cycles the hands
//! ([`MiddleClick`]), an alt-click is tg `AltClickOn` ([`AltClick`]: `click_alt`, else the loot
//! panel), a ctrl-click pulls ([`CtrlClick`], adjacent, conscious); a mob aiming a pointed spell
//! ([`ClickIntercept`]) sends its other clicks to it ([`InterceptedClick`], seen, no reach, in any
//! state); then a mob in throw mode throws at whatever it clicked ([`ThrowClick`], seen, no
//! reach). Everything else is a
//! [`ClickResolved`]. Server code clicks for a client with [`ClickOn`] (tg `user.ClickOn(thing)`:
//! the loot panel), which goes through the same queue and checks.
//!
//! Checks, tg `ClickOn` -> `CanReach`: the client controls a conscious mob (tg returns early when
//! `INCAPACITATED`: crit, dead, stuns, stamcrit; a mob with blocked hands only examines and
//! pulls), the client sees the
//! target (its per-client `NetId` resolves through `Interest`), and the target's tile is
//! adjacent to the mob's ([`SpatialIndex::adjacent`]: a diagonal needs a free side tile, so no
//! reaching through wall corners). A plain click (no shift, no ctrl) is then dropped while the
//! mob is on its click cooldown (tg `next_move`, [`NextMove`]), as tg `ClickOn` does before
//! hands, throw mode and reach; consumers that start a cooldown (melee, throws, item attacks)
//! set it.
//!
//! Every click is handled, in the order the client sent it: clicks wait in a per-client queue
//! and one is resolved per client per tick, so the effect of a click (an item taken, a door
//! opened, a cooldown started) is in the world before the next click of that client is checked
//! (tg handles each `Click` in turn; `next_click` keeps them at least a decisecond apart, the
//! one tick here). Clicks over tg's `second_click_limit` in one second are dropped on arrival,
//! which also bounds the queue.

use std::collections::VecDeque;

use bevy::ecs::entity::EntityHashMap;
use bevy::prelude::*;
use ss13_net::server::{IntentReceived, Interest};
use ss13_shared::TilePos;
use ss13_shared::protocol::{ClickModifiers, ClientIntent, NetId};
use ss13_world::{Mover, SpatialIndex, WorldSystems};

use crate::closets::InCloset;
use crate::combat::NextMove;
use crate::health::Stat;
use crate::server::{PlayerMob, Players};
use crate::status_effects::Incapacitation;
use crate::throw::ThrowMode;

/// A click that passed the checks above this tick. Consumers still check what their effect
/// needs (the item is still on the floor, the door is not bolted).
#[derive(Message, Clone, Copy, Debug)]
pub struct ClickResolved {
    pub client: Entity,
    pub mob: Entity,
    pub mob_pos: TilePos,
    pub target: Entity,
    pub target_pos: TilePos,
    pub modifiers: ClickModifiers,
}

/// A shift-click (without ctrl, alt or middle) on an entity the client sees, by its mob or ghost.
#[derive(Message, Clone, Copy, Debug)]
pub struct ExamineClick {
    pub client: Entity,
    pub examiner: Entity,
    pub target: Entity,
}

/// A ctrl-click (without shift) of a conscious mob on an adjacent entity.
#[derive(Message, Clone, Copy, Debug)]
pub struct CtrlClick {
    pub client: Entity,
    pub mob: Entity,
    pub mob_pos: TilePos,
    pub target: Entity,
    pub target_pos: TilePos,
}

/// A ctrl+shift click of a conscious mob on an entity it sees: tg `click_ctrl_shift`, which
/// offers the held item to a living target (`crate::buffs`).
#[derive(Message, Clone, Copy, Debug)]
pub struct GiveClick {
    pub client: Entity,
    pub mob: Entity,
    pub target: Entity,
}

/// A click of a conscious mob in throw mode on an entity it sees, at any distance.
#[derive(Message, Clone, Copy, Debug)]
pub struct ThrowClick {
    pub client: Entity,
    pub mob: Entity,
    pub target: Entity,
    pub target_pos: TilePos,
}

/// A middle click (without shift or ctrl): tg `MiddleClickOn` -> `cycle_hand`.
#[derive(Message, Clone, Copy, Debug)]
pub struct MiddleClick {
    pub client: Entity,
    pub mob: Entity,
}

/// A shift+middle click on an entity the client sees: tg `ShiftMiddleClickOn` -> `pointed`.
#[derive(Message, Clone, Copy, Debug)]
pub struct PointClick {
    pub client: Entity,
    pub mob: Entity,
    pub target: Entity,
}

/// An alt-click (tg `AltClickOn` -> `base_click_alt`) on an entity the client sees, by its mob
/// in any state; `loot_only` for alt+shift (tg `alt_shift_click_on`). `reach`: the target is
/// adjacent (tg `can_perform_action`'s reach part).
#[derive(Message, Clone, Copy, Debug)]
pub struct AltClick {
    pub client: Entity,
    pub mob: Entity,
    pub target: Entity,
    pub target_pos: TilePos,
    pub reach: bool,
    pub loot_only: bool,
    /// Alt+right click (tg `click_alt_secondary`).
    pub secondary: bool,
}

/// A plain or right click of a mob with a [`ClickIntercept`] on an entity its client sees, at any
/// distance and in any state: tg `check_click_intercept` (a pointed spell's `InterceptClickOn`,
/// which checks range and the caster itself).
#[derive(Message, Clone, Copy, Debug)]
pub struct InterceptedClick {
    pub client: Entity,
    pub mob: Entity,
    pub mob_pos: TilePos,
    pub target: Entity,
    pub target_pos: TilePos,
    pub right: bool,
}

/// A plain or right click past reach (tg `ClickOn` -> `ranged_interact_with_atom` of the held
/// item, `RangedAttack`), after every check a [`ClickResolved`] passes but adjacency: guns fire
/// at it (`crate::guns`).
#[derive(Message, Clone, Copy, Debug)]
pub struct RangedClick {
    pub client: Entity,
    pub mob: Entity,
    pub mob_pos: TilePos,
    pub target: Entity,
    pub target_pos: TilePos,
    pub modifiers: ClickModifiers,
}

/// tg `click_intercept`: the mob's next clicks go to [`InterceptedClick`] instead.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct ClickIntercept;

/// On a machine whose occupant works its controls from inside (tg `controls_inside` sleepers:
/// `CanReach` from inside reaches `loc`): a plain click of the mob inside on it is resolved as
/// an adjacent click. Every other click from inside a closet or machine is dropped.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct ControlsFromInside;

/// A click the server makes for `client` on `target` (tg `user.ClickOn(thing, modifiers)`),
/// queued and checked like one the client sent.
#[derive(Message, Clone, Copy, Debug)]
pub struct ClickOn {
    pub client: Entity,
    pub target: Entity,
    pub modifiers: ClickModifiers,
}

/// Consumers of [`ClickResolved`] (and the other click messages) run after this set.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResolveClicks;

pub struct ClickServerPlugin;

impl Plugin for ClickServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ClickResolved>()
            .add_message::<ExamineClick>()
            .add_message::<CtrlClick>()
            .add_message::<GiveClick>()
            .add_message::<ThrowClick>()
            .add_message::<MiddleClick>()
            .add_message::<PointClick>()
            .add_message::<AltClick>()
            .add_message::<ClickOn>()
            .add_message::<InterceptedClick>()
            .add_message::<RangedClick>()
            .add_message::<crate::modsuit::server::ModuleClick>()
            .add_message::<crate::silicon::ai::AiClick>()
            .add_message::<crate::antag::blob::BlobClick>()
            .add_message::<crate::science::xenobio_console::XenoConsoleClick>()
            .add_message::<crate::silicon::interact::RobotClick>()
            .add_systems(
                Update,
                resolve_clicks
                    .in_set(ResolveClicks)
                    .after(WorldSystems::Movement),
            );
    }
}

/// tg `second_click_limit` (config default 15): clicks a client sends beyond this many in one
/// second are ignored.
// shortcut: no admin exemption (tg skips the limit for `holder`); add it with admin ranks.
pub const SECOND_CLICK_LIMIT: u32 = 15;

/// What a queued click is on: an id the client sent, or an entity the server named.
#[derive(Clone, Copy, Debug)]
enum Clicked {
    Net(NetId),
    Server(Entity),
}

/// One client's clicks not resolved yet, oldest first, and its tg `clicklimiter` second.
#[derive(Default)]
struct ClickQueue {
    clicks: VecDeque<(Clicked, ClickModifiers)>,
    second: u64,
    count: u32,
}

// Cost: per tick, one queue push per received click and one resolved click per client with
// clicks waiting.
// shortcut: tg runs `UnarmedAttack` with `CLICK_CD_HANDCUFFED` for a mob whose hands are
// blocked; a human does nothing useful with it, so its clicks (but ctrl and examine) stop here.
fn resolve_clicks(
    mut intents: MessageReader<IntentReceived>,
    mut server_clicks: MessageReader<ClickOn>,
    mut queues: Local<EntityHashMap<ClickQueue>>,
    time: Res<Time>,
    players: Res<Players>,
    interest: Res<Interest>,
    index: Res<SpatialIndex>,
    mobs: Query<(
        &TilePos,
        &PlayerMob,
        Option<&Stat>,
        Option<&ThrowMode>,
        Option<&Incapacitation>,
        Option<&NextMove>,
        Option<&InCloset>,
        Has<crate::silicon::ai::AiEye>,
        Has<crate::mecha::Pilot>,
        Has<ClickIntercept>,
        Has<crate::silicon::cyborg::Cyborg>,
        Has<crate::ghost::Ghost>,
        Has<crate::science::xenobio_console::XenoOperator>,
        Has<crate::antag::blob::BlobOvermind>,
    )>,
    movers: Query<(), With<Mover>>,
    objs: Query<&ss13_mapload::DmObj>,
    inside: Query<(), Or<(With<crate::mecha::Pilot>, With<InCloset>)>>,
    positions: Query<(&TilePos, Has<ControlsFromInside>)>,
    module_users: Query<(), With<crate::modsuit::server::ModuleSelected>>,
    jaunters: Query<(), With<crate::spells::Jaunting>>,
    mut out: ClickWriters,
    mut xeno: MessageWriter<crate::science::xenobio_console::XenoConsoleClick>,
) {
    let now = time.elapsed_secs_f64();
    let second = now as u64;
    let sent = intents.read().filter_map(|intent| match intent.intent {
        ClientIntent::Click { target, modifiers } => {
            Some((intent.client, Clicked::Net(target), modifiers))
        }
        _ => None,
    });
    let made = server_clicks
        .read()
        .map(|c| (c.client, Clicked::Server(c.target), c.modifiers));
    for (client, target, modifiers) in sent.chain(made) {
        let queue = queues.entry(client).or_default();
        if queue.second != second {
            queue.second = second;
            queue.count = 0;
        }
        queue.count += 1;
        if queue.count <= SECOND_CLICK_LIMIT {
            queue.clicks.push_back((target, modifiers));
        }
    }
    // A queue stays while it has clicks or its second still counts; a gone client's empties.
    queues.retain(|_, q| !q.clicks.is_empty() || q.second == second);
    for (&client, queue) in queues.iter_mut() {
        let Some((clicked, modifiers)) = queue.clicks.pop_front() else {
            continue;
        };
        let Some(mob) = players.mob(client) else {
            queue.clicks.clear();
            continue;
        };
        let Ok((
            &mob_pos,
            player,
            stat,
            throw_mode,
            traits,
            next_move,
            closet,
            ai_eye,
            piloting,
            intercept,
            robot,
            ghost,
            xeno_operator,
            overmind,
        )) = mobs.get(mob)
        else {
            continue;
        };
        if player.client != client {
            continue;
        }
        // A server click is on something the client could have clicked: replicated to it.
        let target = match clicked {
            Clicked::Net(id) => interest.entity(client, id),
            Clicked::Server(e) => interest.net_id(client, e).map(|_| e),
        };
        let Some(target) = target else {
            continue;
        };
        // A pilot is inside its mech and a hidden mob inside its closet or disposals (tg `loc`
        // is the container): only the container can be clicked (tg `CanReach`).
        if target != mob && inside.contains(target) {
            continue;
        }
        let Ok((&target_pos, controls_inside)) = positions.get(target) else {
            continue;
        };
        let in_closet = closet.is_some();
        // The machine the mob is in, when its controls are inside.
        let from_inside = controls_inside && closet.is_some_and(|c| c.closet == target);
        // tg `/mob/living/silicon/ai/ClickOn` has its own rules (`crate::silicon::ai`).
        if ai_eye {
            out.ai.write(crate::silicon::ai::AiClick {
                client,
                eye: mob,
                target,
                target_pos,
                modifiers,
            });
            continue;
        }
        // tg `ClickOn`: the modifier procs come before the incapacitation check.
        // tg `adjacent.dm`: tables and racks (`LETPASSTHROW`, `LETPASSCLICKS`) leave the
        // corner of a diagonal reach open.
        let passes = |e| {
            objs.get(e).is_ok_and(|o| {
                o.pass_flags_self
                    & (crate::inventory::LETPASSTHROW | crate::inventory::LETPASSCLICKS)
                    != 0
            })
        };
        let reach_of = |target_pos| {
            index.adjacent(mob_pos, target_pos, |e| {
                e == target || movers.contains(e) || passes(e)
            })
        };
        if modifiers.shift && modifiers.middle {
            out.point.write(PointClick {
                client,
                mob,
                target,
            });
            continue;
        }
        if modifiers.shift && modifiers.alt && !modifiers.ctrl {
            out.alt.write(AltClick {
                client,
                mob,
                target,
                target_pos,
                reach: !in_closet && reach_of(target_pos),
                loot_only: true,
                secondary: false,
            });
            continue;
        }
        // tg `GrantActions` of the slime console: its operator's shift, ctrl and alt clicks
        // (`crate::science::xenobio_console`, which examines other shift-clicked things).
        if xeno_operator
            && !modifiers.middle
            && u8::from(modifiers.shift) + u8::from(modifiers.ctrl) + u8::from(modifiers.alt) == 1
        {
            xeno.write(crate::science::xenobio_console::XenoConsoleClick {
                client,
                mob,
                target,
                target_pos,
                modifiers,
            });
            continue;
        }
        if modifiers.shift && !modifiers.ctrl {
            out.examine.write(ExamineClick {
                client,
                examiner: mob,
                target,
            });
            continue;
        }
        // tg `/mob/eye/blob/ClickOn` (`crate::antag::blob`): the rest is on the clicked tile.
        if overmind {
            if !modifiers.right {
                out.blob.write(crate::antag::blob::BlobClick {
                    client,
                    overmind: mob,
                    target_pos,
                    modifiers,
                });
            }
            continue;
        }
        // tg `/obj/vehicle/sealed/mecha/on_mouseclick` and `on_click_alt` (`COMSIG_MOB_CLICKON`,
        // `COMSIG_MOB_ALTCLICKON` of the pilot) take the rest (`crate::mecha`).
        if piloting {
            if !modifiers.ctrl {
                out.mech.write(crate::mecha::core::MechClick {
                    client,
                    pilot: mob,
                    target,
                    target_pos,
                    modifiers,
                });
            }
            continue;
        }
        if modifiers.middle {
            // tg `CtrlMiddleClickOn` is an admin tag: nothing for players. A selected MOD
            // module takes the click instead (`COMSIG_MOB_MIDDLECLICKON`, `crate::modsuit`).
            if !modifiers.ctrl && module_users.contains(mob) {
                out.module.write(crate::modsuit::server::ModuleClick {
                    client,
                    mob,
                    target,
                    target_pos,
                });
            } else if !modifiers.ctrl {
                out.middle.write(MiddleClick { client, mob });
            }
            continue;
        }
        if modifiers.alt && !modifiers.shift {
            // Alt+right is tg `click_alt_secondary` (a jumpsuit's accessory).
            out.alt.write(AltClick {
                client,
                mob,
                target,
                target_pos,
                reach: !in_closet && reach_of(target_pos),
                loot_only: false,
                secondary: modifiers.right,
            });
            continue;
        }
        // shortcut: from inside a closet tg reaches only the closet itself (`CanReach` through
        // `loc`); here a mob inside clicks nothing but a machine with [`ControlsFromInside`].
        // A jaunter is inside its `phased_mob` holder: it reaches nothing (tg `CanReach`).
        if (in_closet && !from_inside) || jaunters.contains(mob) {
            continue;
        }
        if intercept && !modifiers.ctrl {
            out.intercept.write(InterceptedClick {
                client,
                mob,
                mob_pos,
                target,
                target_pos,
                right: modifiers.right,
            });
            continue;
        }
        // tg `observer/ClickOn` -> `attack_ghost` of the target, from any distance it sees.
        if ghost {
            if !modifiers.ctrl && !modifiers.shift && !modifiers.right {
                out.ghost.write(crate::multiz::GhostClick {
                    client,
                    ghost: mob,
                    target,
                });
            }
            continue;
        }
        let traits = traits.copied().unwrap_or_default();
        // tg `INCAPACITATED_IGNORING(src, INCAPABLE_RESTRAINTS)`: stuns and stamcrit too.
        if stat != Some(&Stat::Conscious) || traits.incapacitated {
            continue;
        }
        if modifiers.shift {
            // Ctrl+shift: tg `CtrlShiftClickOn` -> `/mob/living/click_ctrl_shift` -> `give`
            // (`crate::buffs`).
            out.give.write(GiveClick {
                client,
                mob,
                target,
            });
            continue;
        }
        // tg `ClickCross` skips the target itself and checks objects only, not mobs.
        let reach = from_inside || reach_of(target_pos);
        if modifiers.ctrl {
            if reach {
                out.ctrl.write(CtrlClick {
                    client,
                    mob,
                    mob_pos,
                    target,
                    target_pos,
                });
            }
            continue;
        }
        // tg `ClickOn`: `if(next_move > world.time) return`, after ctrl and before everything
        // else.
        if next_move.is_some_and(|n| now < n.0) {
            continue;
        }
        if traits.hands_blocked {
            continue;
        }
        if throw_mode.is_some_and(|t| t.0) && !from_inside {
            out.throw.write(ThrowClick {
                client,
                mob,
                target,
                target_pos,
            });
            continue;
        }
        if reach {
            out.resolved.write(ClickResolved {
                client,
                mob,
                mob_pos,
                target,
                target_pos,
                modifiers,
            });
        } else if robot
            && !modifiers.right
            && mob_pos
                .chebyshev(target_pos)
                .is_some_and(|d| d <= crate::silicon::interact::ROBOT_INTERACTION_RANGE)
        {
            out.robot.write(crate::silicon::interact::RobotClick {
                client,
                borg: mob,
                target,
            });
        } else if !robot {
            out.ranged.write(RangedClick {
                client,
                mob,
                mob_pos,
                target,
                target_pos,
                modifiers,
            });
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct ClickWriters<'w> {
    resolved: MessageWriter<'w, ClickResolved>,
    examine: MessageWriter<'w, ExamineClick>,
    ctrl: MessageWriter<'w, CtrlClick>,
    throw: MessageWriter<'w, ThrowClick>,
    middle: MessageWriter<'w, MiddleClick>,
    point: MessageWriter<'w, PointClick>,
    alt: MessageWriter<'w, AltClick>,
    ai: MessageWriter<'w, crate::silicon::ai::AiClick>,
    mech: MessageWriter<'w, crate::mecha::core::MechClick>,
    intercept: MessageWriter<'w, InterceptedClick>,
    robot: MessageWriter<'w, crate::silicon::interact::RobotClick>,
    ranged: MessageWriter<'w, RangedClick>,
    ghost: MessageWriter<'w, crate::multiz::GhostClick>,
    module: MessageWriter<'w, crate::modsuit::server::ModuleClick>,
    give: MessageWriter<'w, GiveClick>,
    blob: MessageWriter<'w, crate::antag::blob::BlobClick>,
}
