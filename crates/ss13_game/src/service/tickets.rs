//! The ticket machine (server), after tg `code/modules/paperwork/ticketmachine.dm`.
//!
//! - An empty hand on the machine takes the next numbered ticket (one per person while it is
//!   held: the holder gets another only when the ticket is gone), up to 100; then it wants a
//!   hand labeler refill roll (3 s, only when empty), which resets both counters and makes the
//!   old tickets disperse.
//! - A button with a ticket controller (`/obj/item/assembly/control/ticket_machine`,
//!   `crate::wallmount::controls`) serves the next ticket: the served one disperses, the next
//!   one vibrates and the machine announces it.
//! - Emagged ("bureaucratic nightmare engaged"): random counters, the old tickets disperse, and
//!   every ticket comes out burning (5 s apart), sets its taker on fire and falls; tickets are
//!   no longer used up when served.
// shortcut: the machine shows its number on examine only (no maptext); every machine and button
// uses the default id `ticket_machine_default` (custom map ids and multitool links are not
// read); a burning ticket is dropped and burns away at once.

use bevy::prelude::*;
use ss13_shared::chat::Span;

use super::server::{item_path, sound_at};
use crate::obj_damage::is_under;
use crate::tools::{Actor, tell};

const MACHINE: &str = "/obj/machinery/ticket_machine";
pub const TICKET: &str = "/obj/item/ticket_machine_ticket";
const LABELER_REFILL: &str = "/obj/item/hand_labeler_refill";
const TAKE_SOUND: &str = "sound/machines/terminal/terminal_insert_disc.ogg";
const SERVE_SOUND: &str = "sound/announcer/announcement/announce_dig.ogg";
/// `max_number`, `cooldown` (emagged), the refill's `do_after`.
const MAX_NUMBER: u32 = 100;
const EMAG_COOLDOWN: f64 = 5.0;
const REFILL_DS: f32 = 30.0;
const SAMETILE_MESSAGE_RANGE: u32 = 1;

/// `/obj/machinery/ticket_machine` state.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct TicketMachine {
    pub ticket_number: u32,
    pub current_number: u32,
    pub emagged: bool,
    /// `ready` is false until this time.
    pub ready_at: f64,
    /// `tickets` with their `owner_ref` and number, oldest first; the head is `current_ticket`
    /// once served.
    pub tickets: Vec<(Entity, Entity, u32)>,
    pub current: Option<Entity>,
}

/// `/obj/item/ticket_machine_ticket`: its `number` and `source`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ticket {
    pub number: u32,
    pub source: Entity,
}

pub(super) fn add_obj_parts(world: &mut World, e: Entity, path: &str) {
    if is_under(path, MACHINE) && world.get::<TicketMachine>(e).is_none() {
        world.entity_mut(e).insert(TicketMachine::default());
    }
}

/// tg `ticket_machine_ticket/Destroy`: tickets gone since are off the machine's lists.
fn prune(world: &mut World, machine: Entity) {
    let alive: Vec<bool> = world
        .get::<TicketMachine>(machine)
        .map(|m| {
            m.tickets
                .iter()
                .map(|(t, ..)| world.get_entity(*t).is_ok())
                .collect()
        })
        .unwrap_or_default();
    let current_alive = world
        .get::<TicketMachine>(machine)
        .and_then(|m| m.current)
        .is_some_and(|c| world.get_entity(c).is_ok());
    if let Some(mut m) = world.get_mut::<TicketMachine>(machine) {
        let mut it = alive.into_iter();
        m.tickets.retain(|_| it.next().unwrap_or(false));
        if !current_alive {
            m.current = None;
        }
    }
}

/// `audible_message("\the [ticket] disperses!", SAMETILE_MESSAGE_RANGE)` and `qdel`.
fn disperse(world: &mut World, ticket: Entity) {
    if world.get_entity(ticket).is_err() {
        return;
    }
    let name = crate::chem::name_of(world, ticket);
    hear_on_tile(world, ticket, &format!("{name} disperses!"));
    world.despawn(ticket);
}

/// A `SAMETILE_MESSAGE_RANGE` audible message from `source` (its tile or its holder's).
fn hear_on_tile(world: &mut World, source: Entity, text: &str) {
    let Some(pos) = crate::chem::map_pos(world, source) else {
        return;
    };
    let clients = crate::obj_damage::clients_in_range(world, pos, SAMETILE_MESSAGE_RANGE, &[]);
    if !clients.is_empty() {
        world.write_message(ss13_net::chat::server::SendChat {
            clients,
            message: crate::chem::line(Span::Notice, text),
        });
    }
}

fn disperse_all(world: &mut World, machine: Entity) {
    let tickets: Vec<Entity> = world
        .get_mut::<TicketMachine>(machine)
        .map(|mut m| {
            m.current = None;
            m.tickets.drain(..).map(|(t, ..)| t).collect()
        })
        .unwrap_or_default();
    for ticket in tickets {
        disperse(world, ticket);
    }
}

fn refresh_icon(world: &mut World, machine: Entity) {
    let full = world
        .get::<TicketMachine>(machine)
        .is_some_and(|m| m.ticket_number >= MAX_NUMBER);
    let state = if full {
        "ticketmachine_empty"
    } else {
        "ticketmachine"
    };
    super::server::set_icon_state(world, machine, "icons/obj/service/bureaucracy.dmi", state);
}

/// `ticket_machine/attack_hand`. True when `target` is a ticket machine.
pub(super) fn take_ticket(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<TicketMachine>(target).is_none() {
        return false;
    }
    prune(world, target);
    let t = super::server::now(world);
    let Some(m) = world.get::<TicketMachine>(target).cloned() else {
        return true;
    };
    if t < m.ready_at {
        tell(
            world,
            client,
            Span::Warning,
            "You press the button, but nothing happens...",
        );
        return true;
    }
    if m.ticket_number >= MAX_NUMBER {
        tell(
            world,
            client,
            Span::Warning,
            "Ticket supply depleted, please refill this unit with a hand labeller refill \
             cartridge!",
        );
        return true;
    }
    if !m.emagged && m.tickets.iter().any(|&(_, owner, _)| owner == mob) {
        tell(world, client, Span::Warning, "You already have a ticket!");
        return true;
    }
    let Some(pos) = crate::chem::map_pos(world, target) else {
        return true;
    };
    sound_at(world, target, TAKE_SOUND, 100.0, false);
    let number = m.ticket_number + 1;
    tell(
        world,
        client,
        Span::Notice,
        format!(
            "You take a ticket from {}, looks like you're number {number} in queue...",
            crate::chem::name_of(world, target)
        ),
    );
    let Some(ticket) = crate::construction::spawn_obj(world, TICKET, pos) else {
        return true;
    };
    let name = format!("{} #{number}", crate::chem::name_of(world, ticket));
    super::server::set_name(world, ticket, &name);
    world.entity_mut(ticket).insert(Ticket {
        number,
        source: target,
    });
    if let Some(mut m) = world.get_mut::<TicketMachine>(target) {
        m.ticket_number = number;
        m.tickets.push((ticket, mob, number));
        if m.emagged {
            m.ready_at = t + EMAG_COOLDOWN;
        }
    }
    if m.emagged {
        // `theirticket.fire_act()`, dropped, and the taker set alight.
        if let Some(&at) = world.get::<ss13_shared::TilePos>(mob) {
            crate::construction::spawn_obj(world, "/obj/effect/decal/cleanable/ash", at);
        }
        crate::fire::adjust_fire_stacks(world, mob, 1.0);
        crate::fire::ignite_mob(world, mob);
        world.despawn(ticket);
        prune(world, target);
    } else if let Some(inventory) = world.get::<crate::inventory::Inventory>(mob).cloned() {
        crate::storage::put_in_hands(world, mob, &inventory, ticket);
    }
    refresh_icon(world, target);
    true
}

/// `ticket_machine/item_interaction` (a refill roll, once empty) and `emag_act`. True when the
/// click was theirs.
pub(super) fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(m) = world.get::<TicketMachine>(target).cloned() else {
        return false;
    };
    if crate::emag::is_emag(world, held) {
        if !m.emagged {
            crate::balloon::balloon_alert(world, mob, target, "bureaucratic nightmare engaged");
            let n = world
                .resource_mut::<crate::combat::CombatRng>()
                .range(0, MAX_NUMBER);
            disperse_all(world, target);
            if let Some(mut m) = world.get_mut::<TicketMachine>(target) {
                m.ticket_number = n;
                m.current_number = n;
                m.emagged = true;
            }
            refresh_icon(world, target);
        }
        return true;
    }
    if item_path(world, held) != LABELER_REFILL {
        return false;
    }
    let name = crate::chem::name_of(world, target);
    if m.ticket_number < MAX_NUMBER {
        let left = MAX_NUMBER - m.ticket_number;
        let (verb, s) = if left == 1 { ("is", "") } else { ("are", "s") };
        tell(
            world,
            client,
            Span::Notice,
            format!(
                "{name} refuses {}! There {verb} still {left} ticket{s} left!",
                crate::chem::name_of(world, held)
            ),
        );
        return true;
    }
    tell(
        world,
        client,
        Span::Notice,
        format!(
            "You start to refill {name}'s ticket holder (doing this will reset its ticket count!)."
        ),
    );
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    crate::tools::do_after(
        world,
        actor,
        target,
        REFILL_DS,
        0,
        crate::construction::Job::Art(super::ArtJob::TicketRefill),
    );
    true
}

/// The refill's `do_after` is done: checked again, the roll is used up.
pub(super) fn refilled(world: &mut World, client: Entity, machine: Entity, roll: Option<Entity>) {
    let Some(roll) = roll.filter(|&r| world.get_entity(r).is_ok()) else {
        return;
    };
    if item_path(world, roll) != LABELER_REFILL
        || world
            .get::<TicketMachine>(machine)
            .is_none_or(|m| m.ticket_number < MAX_NUMBER)
    {
        return;
    }
    let name = crate::chem::name_of(world, machine);
    let roll_name = crate::chem::name_of(world, roll);
    tell(
        world,
        client,
        Span::Notice,
        format!("You insert {roll_name} into {name} as it whirs nondescriptly."),
    );
    world.despawn(roll);
    disperse_all(world, machine);
    if let Some(mut m) = world.get_mut::<TicketMachine>(machine) {
        m.ticket_number = 0;
        m.current_number = 0;
    }
    refresh_icon(world, machine);
}

/// `/obj/item/assembly/control/ticket_machine/activate` -> `increment()` on every ticket
/// machine (they all share the default id).
pub fn serve_next(world: &mut World, client: Option<Entity>) {
    let machines: Vec<Entity> = world
        .query_filtered::<Entity, With<TicketMachine>>()
        .iter(world)
        .collect();
    let mut none_left = !machines.is_empty();
    for machine in machines {
        prune(world, machine);
        increment(world, machine);
        if world
            .get::<TicketMachine>(machine)
            .is_some_and(|m| m.current.is_some())
        {
            none_left = false;
        }
    }
    if none_left && let Some(client) = client {
        tell(
            world,
            client,
            Span::Notice,
            "The button light indicates that there are no more tickets to be processed.",
        );
    }
}

/// `ticket_machine/increment`.
fn increment(world: &mut World, machine: Entity) {
    let Some(m) = world.get::<TicketMachine>(machine).cloned() else {
        return;
    };
    if !m.emagged
        && let Some(current) = m.current
    {
        if let Some(mut m) = world.get_mut::<TicketMachine>(machine) {
            if !m.tickets.is_empty() {
                m.tickets.remove(0);
            }
            m.current = None;
        }
        disperse(world, current);
    }
    let Some(&(next, _, number)) = world
        .get::<TicketMachine>(machine)
        .and_then(|m| m.tickets.first())
    else {
        return;
    };
    if let Some(mut m) = world.get_mut::<TicketMachine>(machine) {
        m.current = Some(next);
        m.current_number = number;
    }
    sound_at(world, machine, SERVE_SOUND, 50.0, false);
    let ticket_name = crate::chem::name_of(world, next);
    crate::atmos_machinery::machine_say(world, machine, &format!("Now serving {ticket_name}!"));
    if !m.emagged {
        let name = crate::chem::name_of(world, next);
        hear_on_tile(world, next, &format!("{name} vibrates!"));
    }
}

/// `ticket_machine/examine` and `ticket_machine_ticket/examine`.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(Span, String)>) {
    if let Some(m) = world.get::<TicketMachine>(target) {
        lines.push((
            Span::Notice,
            format!(
                "The ticket machine shows that ticket #{} is currently being served.",
                m.current_number
            ),
        ));
        lines.push((
            Span::Notice,
            format!(
                "You can take a ticket out with Left-Click to be number {} in queue.",
                m.ticket_number + 1
            ),
        ));
    }
    if let Some(ticket) = world.get::<Ticket>(target) {
        lines.push((
            Span::Notice,
            format!(
                "The ticket reads shimmering text that tells you that you are number {} in queue.",
                ticket.number
            ),
        ));
        if let Some(m) = world.get::<TicketMachine>(ticket.source) {
            let away = i64::from(ticket.number) - i64::from(m.current_number);
            let s = if away == 1 { "" } else { "s" };
            lines.push((
                Span::Notice,
                format!(
                    "Below that, you can see that you are {away} spot{s} away from being served."
                ),
            ));
        }
    }
}
