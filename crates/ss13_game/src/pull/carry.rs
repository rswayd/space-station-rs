//! Fireman carries and piggyback rides (server), after tg `human.dm` (`mouse_buckle_handling`,
//! `fireman_carry`, `piggyback`, `is_buckle_possible`) and
//! `/datum/component/riding/creature/human`.
//!
//! - A human grabbing another aggressively drags the lying human onto itself: after 5 s
//!   (`do_after`) it carries it on its shoulders. A human held aggressively by a standing human
//!   drags itself onto its grabber: after 1.5 s it rides on its back. The grab ends either way.
//! - The carried mob is buckled to its carrier ([`Buckled`] + [`Carried`]): it stays on the
//!   carrier's tile and faces its way, drawn with tg's riding offsets and layers
//!   ([`super::HeldOffset`]); it cannot step (`BUCKLED_TRAIT` immobilized, a fireman carry also
//!   floors it lying at 90 degrees, `crate::status_effects`). The carrier slows by
//!   `HUMAN_CARRY_SLOWDOWN` (`super::pull_slowdown`).
//! - The ride ends when the carried mob resists (`crate::medical::surgery::resist_buckle`), when
//!   the carrier falls over (both thud: the rider is paralyzed 1 s and knocked down 4 s), and on
//!   a carrier's step when tg `ride_check` fails: a fireman carrier restrained or incapacitated,
//!   a piggyback rider restrained or incapacitated (the rider falls off, paralyzed 1 s).
// shortcut: no carry speed skills, quirks or spine implants (always 5 s), no riding offhand items
// in the hands, no shoving the rider off (right click), no third person dragging a mob onto a
// carrier, no `TRAIT_UNDENSE` of a piggyback rider (it is moved onto the carrier's tile, never
// stepped), no "не удаётся" line when the `do_after` is interrupted; add with skills and offhands.

use bevy::prelude::*;
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::{Facing, TilePos};

use super::{GrabState, HeldOffset, Pulling, name_of, visible_message};
use crate::health::{BodyPosition, Stat};
use crate::medical::surgery::Buckled;
use crate::status_effects::Incap;
use crate::tools::{Actor, DoAfter, tell};

/// `HUMAN_CARRY_SLOWDOWN`, deciseconds.
pub const HUMAN_CARRY_SLOWDOWN: f32 = 0.35;
/// `fireman_carry`'s `carrydelay`, `piggyback`'s `do_after`, deciseconds.
pub const FIREMAN_CARRY_DS: f32 = 50.0;
pub const PIGGYBACK_DS: f32 = 15.0;
/// `MOB_ABOVE_PIGGYBACK_LAYER`, `MOB_BELOW_PIGGYBACK_LAYER`.
pub const MOB_ABOVE_PIGGYBACK_LAYER: f32 = 4.06;
pub const MOB_BELOW_PIGGYBACK_LAYER: f32 = 3.94;

/// On a mob carried by a human (tg `buckled` to it with `/datum/component/riding/creature/human`):
/// `fireman` for a shoulder carry (`CARRIER_NEEDS_ARM`), else a piggyback ride
/// (`RIDER_NEEDS_ARMS`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Carried {
    pub by: Entity,
    pub fireman: bool,
}

pub struct CarryPlugin;

impl Plugin for CarryPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            carry_riders
                .after(ss13_world::WorldSystems::Movement)
                .before(crate::health::HealthSystems),
        );
    }
}

fn is_human(world: &World, mob: Entity) -> bool {
    world.get::<crate::human::HumanLook>(mob).is_some()
}

fn lying(world: &World, mob: Entity) -> bool {
    world.get::<BodyPosition>(mob).is_some_and(|b| b.lying())
}

/// `IS_UNCONSCIOUS_OR_CRIT`.
fn unconscious_or_crit(world: &World, mob: Entity) -> bool {
    world.get::<Stat>(mob).is_none_or(|s| *s >= Stat::SoftCrit)
        || crate::status_effects::incapacitation(world, mob).knocked_out
}

/// `INCAPACITATED_IGNORING(mob, INCAPABLE_GRAB)`: the grab's own hold does not count.
fn incapacitated(world: &World, mob: Entity) -> bool {
    crate::status_effects::incapacitation(world, mob).incapacitated
}

fn restrained(world: &World, mob: Entity) -> bool {
    crate::status_effects::incapacitation(world, mob).restrained
}

/// `can_be_firemanned`: a lying human.
pub fn can_be_firemanned(world: &World, target: Entity) -> bool {
    is_human(world, target) && lying(world, target)
}

/// `can_piggyback`: a carbon that is awake (only humans can mount humans,
/// `TRAIT_CAN_MOUNT_HUMANS`).
pub fn can_piggyback(world: &World, rider: Entity) -> bool {
    is_human(world, rider) && !unconscious_or_crit(world, rider)
}

/// The mob riding `carrier`, if any.
pub fn rider_of(world: &mut World, carrier: Entity) -> Option<Entity> {
    world
        .query::<(Entity, &Carried)>()
        .iter(world)
        .find(|(_, c)| c.by == carrier)
        .map(|(e, _)| e)
}

/// `/mob/living/carbon/human/mouse_buckle_handling` of `carrier` with `dragged` dropped onto it
/// by `actor.mob`: the carrier must grab it aggressively and be awake; the dragged mob climbing
/// onto its grabber rides piggyback, the grabber dragging a lying human onto itself carries it.
/// True when the drop was a carry's.
pub fn mouse_buckle_handling(
    world: &mut World,
    actor: Actor,
    carrier: Entity,
    dragged: Entity,
) -> bool {
    let grabbing = world.get::<Pulling>(carrier).map(|p| p.0) == Some(dragged)
        && world.get::<GrabState>(carrier) == Some(&GrabState::Aggressive);
    if !is_human(world, carrier) || !grabbing || unconscious_or_crit(world, carrier) {
        return false;
    }
    if actor.mob == dragged && can_piggyback(world, dragged) {
        piggyback(world, actor, carrier);
        return true;
    }
    if actor.mob == carrier && can_be_firemanned(world, dragged) {
        fireman_carry(world, actor, dragged);
        return true;
    }
    false
}

fn say(world: &mut World, at: Entity, others: (Span, String), personal: &[(Entity, Span, String)]) {
    let Some(&pos) = world.get::<TilePos>(at) else {
        return;
    };
    for message in visible_message(world, pos, ChatChannel::Combat, others, personal) {
        world.write_message(message);
    }
}

/// `fireman_carry` up to its `do_after`.
fn fireman_carry(world: &mut World, actor: Actor, target: Entity) {
    let (user, victim) = (name_of(world, actor.mob), name_of(world, target));
    if incapacitated(world, actor.mob) {
        let text = format!("Вы не можете поднять {victim} на плечи, пока он сопротивляется!");
        tell(world, actor.client, Span::Warning, text);
        return;
    }
    if !crate::tools::do_after(
        world,
        actor,
        target,
        FIREMAN_CARRY_DS,
        0,
        crate::construction::Job::Interact(crate::drag::DragJob::FiremanCarry),
    ) {
        return;
    }
    say(
        world,
        actor.mob,
        (
            Span::Notice,
            format!("{user} начинает поднимать {victim} на свои плечи..."),
        ),
        &[(
            actor.mob,
            Span::Notice,
            format!("Вы начинаете поднимать {victim} на свои плечи..."),
        )],
    );
}

/// The end of the fireman carry `do_after`: still a lying human, the carrier able, the target
/// loose; then `buckle_mob(target, TRUE, TRUE, CARRIER_NEEDS_ARM)`.
pub fn complete_fireman(world: &mut World, carrier: Entity, action: &DoAfter) {
    let target = action.target;
    let fails = !can_be_firemanned(world, target)
        || incapacitated(world, carrier)
        || world.get::<Buckled>(target).is_some();
    if fails || !buckle(world, carrier, target, true) {
        let (user, victim) = (name_of(world, carrier), name_of(world, target));
        let text = format!("{user} не удается поднять {victim} на плечи!");
        say(world, carrier, (Span::Warning, text), &[]);
    }
}

/// `piggyback` up to its `do_after` (the rider's, on the carrier).
fn piggyback(world: &mut World, actor: Actor, carrier: Entity) {
    if !crate::tools::do_after(
        world,
        actor,
        carrier,
        PIGGYBACK_DS,
        0,
        crate::construction::Job::Interact(crate::drag::DragJob::Piggyback),
    ) {
        return;
    }
    let (rider, ridden) = (name_of(world, actor.mob), name_of(world, carrier));
    let text = format!("{rider} начинает залазить на {ridden}...");
    say(world, carrier, (Span::Notice, text), &[]);
}

/// The end of the piggyback `do_after`: the rider still awake, neither side incapacitated; then
/// `buckle_mob(target, TRUE, TRUE, RIDER_NEEDS_ARMS)`.
pub fn complete_piggyback(world: &mut World, rider: Entity, action: &DoAfter) {
    let carrier = action.target;
    let (who, ridden) = (name_of(world, rider), name_of(world, carrier));
    if !can_piggyback(world, rider) {
        let text = format!("{who} не удается залезть на {ridden}!");
        say(world, carrier, (Span::Warning, text), &[]);
        return;
    }
    if incapacitated(world, rider) || incapacitated(world, carrier) {
        let text = format!("{who} не может удержаться на {ridden}!");
        say(world, rider, (Span::Warning, text), &[]);
        return;
    }
    buckle(world, carrier, rider, false);
}

/// `buckle_mob(mob, force = TRUE)` of a human: `is_buckle_possible` (not itself, loose, adjacent,
/// one rider, no stacking), then the riding component (the carrier stops pulling) and the rider
/// on the carrier's tile.
pub fn buckle(world: &mut World, carrier: Entity, mob: Entity, fireman: bool) -> bool {
    let (Some(&at), Some(&pos)) = (world.get::<TilePos>(carrier), world.get::<TilePos>(mob)) else {
        return false;
    };
    if mob == carrier
        || at.chebyshev(pos).is_none_or(|d| d > 1)
        || world.get::<Buckled>(mob).is_some()
        || world.get::<Carried>(carrier).is_some()
        || rider_of(world, carrier).is_some()
        || rider_of(world, mob).is_some()
    {
        return false;
    }
    if !is_human(world, mob) {
        let text = format!(
            "Кажется, {} не может оседлать {}...",
            name_of(world, mob),
            name_of(world, carrier)
        );
        say(world, mob, (Span::Warning, text), &[]);
        return false;
    }
    world.entity_mut(carrier).remove::<Pulling>();
    let facing = world.get::<Facing>(carrier).copied();
    let mut rider = world.entity_mut(mob);
    rider.insert((
        at,
        Buckled(carrier),
        Carried {
            by: carrier,
            fireman,
        },
    ));
    if let Some(facing) = facing {
        rider.insert(facing);
    }
    if world.get::<super::PullSlowdown>(carrier).is_none() {
        world.entity_mut(carrier).insert(super::PullSlowdown(0.0));
    }
    true
}

/// tg `user_unbuckle_mob` -> `unbuckle_feedback` of a rider freed by `actor` (resisting or
/// helped off), after `crate::medical::surgery::unbuckle` took [`Buckled`]. False when `mob`
/// was not carried.
pub fn post_unbuckle(world: &mut World, actor: Actor, mob: Entity, carrier: Entity) -> bool {
    if world.get::<Carried>(mob).is_none_or(|c| c.by != carrier) {
        return false;
    }
    world.entity_mut(mob).remove::<Carried>();
    let (user, rider, ridden) = (
        name_of(world, actor.mob),
        name_of(world, mob),
        name_of(world, carrier),
    );
    if actor.mob == mob {
        say(
            world,
            mob,
            (
                Span::Notice,
                format!("{user} отстегивает себя от {ridden}."),
            ),
            &[(
                mob,
                Span::Notice,
                format!("Вы отстегиваете себя от {ridden}."),
            )],
        );
    } else {
        say(
            world,
            mob,
            (
                Span::Notice,
                format!("{user} отстёгивает {rider} от {ridden}."),
            ),
            &[(
                mob,
                Span::Notice,
                format!("{user} отстёгивает вас от {ridden}."),
            )],
        );
    }
    true
}

/// tg riding offsets and layers of the human seat (`get_rider_offsets_and_layers`) facing
/// `facing`: a fireman carry 6 px up, a piggyback rider 8 px up and 6 px back on the east-west
/// axis; above the carrier when it faces north.
pub fn rider_offset(facing: ss13_shared::Dir, fireman: bool) -> HeldOffset {
    let (dx, dy) = facing.offset();
    let layer = if dx == 0 && dy > 0 {
        MOB_ABOVE_PIGGYBACK_LAYER
    } else {
        MOB_BELOW_PIGGYBACK_LAYER
    };
    let pixel = if fireman {
        [0, 6]
    } else {
        [-6 * dx.signum() as i16, 8]
    };
    HeldOffset {
        pixel,
        layer: Some(layer),
    }
}

/// Throws `rider` off its carrier: unbuckled, the ride's traits gone, paralyzed for `paralyze`
/// seconds (and knocked down for `knockdown`).
fn fall_off(world: &mut World, rider: Entity, paralyze: f32, knockdown: f32) {
    world.entity_mut(rider).remove::<(Buckled, Carried)>();
    crate::status_effects::apply(world, rider, Incap::Paralyze, paralyze);
    if knockdown > 0.0 {
        crate::status_effects::apply(world, rider, Incap::Knockdown, knockdown);
    }
}

/// tg `ride_check` on a carrier's move: a fireman carrier restrained or incapacitated, a
/// piggyback rider restrained or incapacitated.
pub fn ride_holds(world: &World, carrier: Entity, rider: Entity, fireman: bool) -> bool {
    if lying(world, carrier) {
        return false;
    }
    let holder = if fireman { carrier } else { rider };
    !restrained(world, holder) && !incapacitated(world, holder)
}

/// Riders follow their carriers (tile, facing, offsets); a carrier that fell over drops its rider
/// (`check_carrier_fall_over`), a failed `ride_check` on a step drops it too; a ride whose buckle
/// or carrier is gone ends.
// Cost: per tick, one pass over carried mobs only.
fn carry_riders(world: &mut World, mut riders: Local<Vec<(Entity, Carried)>>) {
    riders.extend(
        world
            .query::<(Entity, &Carried)>()
            .iter(world)
            .map(|(e, c)| (e, *c)),
    );
    for (rider, carried) in riders.drain(..) {
        let carrier = carried.by;
        let buckled = world.get::<Buckled>(rider) == Some(&Buckled(carrier));
        let Some(&at) = world.get::<TilePos>(carrier).filter(|_| buckled) else {
            world.entity_mut(rider).remove::<Carried>();
            continue;
        };
        let (rider_name, carrier_name) = (name_of(world, rider), name_of(world, carrier));
        if lying(world, carrier) {
            fall_off(world, rider, 1.0, 4.0);
            say(
                world,
                carrier,
                (
                    Span::Danger,
                    format!("{rider_name} падает с {carrier_name}, и они оба валятся на землю!"),
                ),
                &[
                    (
                        carrier,
                        Span::Warning,
                        format!("Вы падаете на землю, роняя {rider_name}!"),
                    ),
                    (
                        rider,
                        Span::Danger,
                        format!("{carrier_name} падает на землю, роняя вас!"),
                    ),
                ],
            );
            continue;
        }
        if world.get::<TilePos>(rider) != Some(&at) {
            if !ride_holds(world, carrier, rider, carried.fireman) {
                fall_off(world, rider, 1.0, 0.0);
                say(
                    world,
                    rider,
                    (
                        Span::Warning,
                        format!("{rider_name} падает с {carrier_name}!"),
                    ),
                    &[(
                        rider,
                        Span::Warning,
                        format!("Вы падаете с {carrier_name}!"),
                    )],
                );
                continue;
            }
            world.entity_mut(rider).insert(at);
        }
        let facing = world.get::<Facing>(carrier).copied();
        if let Some(facing) = facing {
            if let Some(mut own) = world.get_mut::<Facing>(rider) {
                own.set_if_neq(facing);
            }
            let wanted = rider_offset(facing.0, carried.fireman);
            if world.get::<HeldOffset>(rider) != Some(&wanted) {
                world.entity_mut(rider).insert(wanted);
            }
        }
        // `buckle_lying = 90`: a body on the shoulders lies one way.
        if carried.fireman
            && let Some(mut body) = world.get_mut::<BodyPosition>(rider)
            && body.lying()
            && body.lying_angle != 90
        {
            body.lying_angle = 90;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_shared::Dir;

    #[test]
    fn riders_sit_where_tg_puts_them() {
        let north = rider_offset(Dir::North, false);
        assert_eq!(north.pixel, [0, 8]);
        assert_eq!(north.layer, Some(MOB_ABOVE_PIGGYBACK_LAYER));
        assert_eq!(rider_offset(Dir::South, false).pixel, [0, 8]);
        assert_eq!(rider_offset(Dir::East, false).pixel, [-6, 8]);
        assert_eq!(rider_offset(Dir::West, false).pixel, [6, 8]);
        assert_eq!(
            rider_offset(Dir::East, false).layer,
            Some(MOB_BELOW_PIGGYBACK_LAYER)
        );
        let fireman = rider_offset(Dir::West, true);
        assert_eq!(fireman.pixel, [0, 6]);
        assert_eq!(fireman.layer, Some(MOB_BELOW_PIGGYBACK_LAYER));
        assert_eq!(
            rider_offset(Dir::North, true).layer,
            Some(MOB_ABOVE_PIGGYBACK_LAYER)
        );
    }
}
