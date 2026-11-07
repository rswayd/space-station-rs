//! Pneumatic airlock seals (server), after tg `/obj/item/door_seal`
//! (`code/game/objects/items/door_seal.dm`) and the airlock's `seal`
//! (`airlock.dm` `item_interaction`, `try_remove_seal`).
//!
//! - The seal clicked on a closed airlock braces it in `seal_time` (3 s): the seal goes into the
//!   airlock, which draws `sealed` and neither opens (not even forced) nor is pried
//!   (`crate::doors`, [`crate::doors::Door::seal`]).
//! - An empty hand on a sealed airlock takes the seal off in `unseal_time` (2 s), onto the
//!   user's tile; only humans have the dexterity.
// shortcut: no extra integrity while sealed (`AIRLOCK_SEAL_MULTIPLIER`): doors have no
// integrity yet; no suicide act.

use bevy::prelude::*;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use super::DeviceJob;
use ss13_mapload::DoorKind;

use crate::doors::{Door, DoorState, with_door};
use crate::inventory::{CarriedBy, Inventory};
use crate::sound::{PlaySound, playsound};
use crate::tools::{Actor, do_after, tell};

pub const DOOR_SEAL: &str = "/obj/item/door_seal";
/// `seal_time`, `unseal_time`, deciseconds.
const SEAL_DS: f32 = 30.0;
const UNSEAL_DS: f32 = 20.0;
const JAWS: &str = "sound/items/tools/jaws_pry.ogg";
const FORCED: &str = "sound/machines/airlock/airlockforced.ogg";

fn sound(world: &mut World, door: Entity, file: &str) {
    if let Some(&pos) = world.get::<TilePos>(door) {
        playsound(world, PlaySound::new(pos, file, 30.0, true));
    }
}

/// Why the airlock cannot take a seal now (`!density`, `seal`).
fn refusal(door: &Door) -> Option<&'static str> {
    if door.state != DoorState::Closed {
        Some("Шлюз нужно закрыть, прежде чем герметизировать!")
    } else if door.seal.is_some() {
        Some("Шлюз уже загерметизирован!")
    } else {
        None
    }
}

/// `airlock/item_interaction` with a door seal: the 3 s brace starts.
pub fn start_seal(world: &mut World, client: Entity, mob: Entity, door: Entity, seal: Entity) {
    let Some(d) = world.get::<Door>(door) else {
        return;
    };
    if d.kind != DoorKind::Airlock {
        return;
    }
    if let Some(text) = refusal(d) {
        tell(world, client, Span::Warning, text);
        return;
    }
    let actor = Actor {
        client,
        mob,
        item: Some(seal),
    };
    if !do_after(
        world,
        actor,
        door,
        SEAL_DS,
        0,
        crate::construction::Job::Device(DeviceJob::SealDoor { seal }),
    ) {
        return;
    }
    let user = crate::security::name_of(world, mob);
    crate::medical::visible(
        world,
        mob,
        "Вы начинаете герметизировать шлюз.",
        &format!("{user} начинает герметизировать шлюз."),
        Span::Notice,
    );
    sound(world, door, JAWS);
}

/// The end of the brace, re-checked: the airlock still closed and unsealed, the seal still in
/// the active hand (`do_after`).
pub fn complete_seal(world: &mut World, client: Entity, mob: Entity, door: Entity, seal: Entity) {
    let Some(d) = world.get::<Door>(door) else {
        return;
    };
    if let Some(text) = refusal(d) {
        tell(world, client, Span::Warning, text);
        return;
    }
    if world.get::<CarriedBy>(seal).map(|c| c.0) != Some(mob) {
        return;
    }
    world.entity_mut(seal).remove::<CarriedBy>();
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob)
        && let Some(slot) = inventory.hands.iter_mut().find(|h| **h == Some(seal))
    {
        *slot = None;
    }
    crate::storage::put_inside(world, seal, door);
    crate::inventory::refresh_look(world, mob);
    with_door(world, door, |door, appearance, _| {
        door.seal = Some(seal);
        door.apply_look(appearance);
        ((), false)
    });
    sound(world, door, FORCED);
    let user = crate::security::name_of(world, mob);
    crate::medical::visible(
        world,
        mob,
        "Вы заканчиваете герметизировать шлюз.",
        &format!("{user} заканчивает герметизировать шлюз."),
        Span::Notice,
    );
}

/// `airlock/try_remove_seal`: the 2 s removal starts. Only humans may.
pub fn start_unseal(world: &mut World, client: Entity, mob: Entity, door: Entity) {
    if world.get::<Door>(door).and_then(|d| d.seal).is_none() {
        return;
    }
    if world.get::<crate::human::HumanLook>(mob).is_none() {
        tell(
            world,
            client,
            Span::Warning,
            "Вам не хватает ловкости, чтобы снять герметизатор!",
        );
        return;
    }
    let actor = Actor {
        client,
        mob,
        item: None,
    };
    if !do_after(
        world,
        actor,
        door,
        UNSEAL_DS,
        0,
        crate::construction::Job::Device(DeviceJob::UnsealDoor),
    ) {
        return;
    }
    let user = crate::security::name_of(world, mob);
    crate::medical::visible(
        world,
        mob,
        "Вы начинаете снимать пневматический герметизатор со шлюза.",
        &format!("{user} начинает снимать герметизатор со шлюза."),
        Span::Notice,
    );
    sound(world, door, FORCED);
}

/// The end of the removal: the seal (if still there) onto the user's tile.
pub fn complete_unseal(world: &mut World, mob: Entity, door: Entity) {
    let Some(seal) = world.get::<Door>(door).and_then(|d| d.seal) else {
        return;
    };
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    with_door(world, door, |door, appearance, _| {
        door.seal = None;
        door.apply_look(appearance);
        ((), false)
    });
    if world.get_entity(seal).is_ok() {
        crate::storage::take_out(world, seal, pos);
    }
    sound(world, door, JAWS);
    let user = crate::security::name_of(world, mob);
    crate::medical::visible(
        world,
        mob,
        "Вы снимаете пневматический герметизатор со шлюза.",
        &format!("{user} снимает герметизатор со шлюза."),
        Span::Notice,
    );
}

/// The airlock's examine line (`"It's been braced with [seal]."`).
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(seal) = world.get::<Door>(target).and_then(|d| d.seal) else {
        return;
    };
    let name = crate::security::name_of(world, seal);
    lines.push((Span::Plain, format!("Шлюз укреплён: {name}.")));
}
