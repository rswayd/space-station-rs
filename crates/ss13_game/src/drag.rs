//! Drags (server): `ClientIntent::Drag`, tg `/atom/MouseDrop` -> `base_mouse_drop_handler`
//! (`code/_onclick/drag_drop.dm`), checked against the live world when applied.
//!
//! Both ends resolve first: an atom the client sees on the map, or a HUD slot (the item in it,
//! else the empty slot). Only a conscious, capable mob drags (tg `incapacitated`,
//! `can_perform_action`), and only things: an empty slot is not dragged. Then, in tg's order:
//! 1. `COMSIG_MOUSEDROP_ONTO` of the dragged thing: a storage
//!    ([`crate::storage::mousedrop_onto`]: into an empty hand, open on its user, dump into a
//!    storage or onto a tile) and a strippable human dropped onto the user
//!    ([`crate::strip::open_menu`]) take the drop first, each with its own reach checks;
//! 2. `COMSIG_MOUSEDROPPED_ONTO` of the target: the active hand's item onto a storage goes in
//!    ([`crate::storage::mousedropped_onto`]);
//! 3. both ends must be in reach (in the inventory or adjacent: `IsReachableBy`), then
//!    `mouse_drop_dragged` of the dragged thing (a headset dropped onto its user) and
//!    `mouse_drop_receive` of the target (an open closet takes an item or stuffs a mob,
//!    [`crate::closets::mouse_drop_receive`]).
//!
//! The timed results (a dump onto the floor, stuffing a mob into a closet, loading a microwave
//! from a bag) are [`DragJob`] `do_after`s, revalidated when they end.
//!
//! In tg a storage dragged onto a microwave is dumped onto the microwave's tile: the storage's
//! `COMSIG_MOUSEDROP_ONTO` handler takes every drop onto an object first, so
//! `microwave/mouse_drop_receive` never sees a storage. This port keeps that; a bag *used* on a
//! microwave (`item_interaction` -> `handle_dumping`, `crate::kitchen`) loads its food.
// shortcut: a headset dropped onto its user would `attack_self` (the radio window), which does
// not exist yet; screen objects (action buttons, the movable HUD) are not dragged.

use bevy::ecs::entity::EntityHashMap;
use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_net::server::{IntentReceived, Interest};
use ss13_shared::TilePos;
use ss13_shared::protocol::{ClientIntent, DragEnd, InventorySlot};

use crate::click::{ResolveClicks, SECOND_CLICK_LIMIT};
use crate::health::Stat;
use crate::inventory::Inventory;
use crate::server::{PlayerMob, Players};
use crate::storage::DropOnto;
use crate::tools::{Actor, DoAfter};

/// The timed steps of drags (`crate::construction::Job::Interact`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragJob {
    /// `storage` empties onto `at` (`dump_content_at` onto a tile).
    Dump { storage: Entity, at: TilePos },
    /// The target mob into `closet` (`closet/mouse_drop_receive`), placed at `at`: the closet's
    /// tile when the stuffing started.
    StuffCloset { closet: Entity, at: TilePos },
    /// The food in `storage` into the target microwave (`handle_dumping`).
    LoadMicrowave { storage: Entity },
}

/// One end of a drag, resolved for the dragging mob.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    /// On the map.
    World(Entity),
    /// In the mob's inventory: what its HUD slot shows.
    Carried(Entity),
    /// An empty hand slot, by hand index.
    Hand(usize),
    /// Another empty HUD slot.
    Screen,
}

impl End {
    pub fn entity(self) -> Option<Entity> {
        match self {
            Self::World(e) | Self::Carried(e) => Some(e),
            Self::Hand(_) | Self::Screen => None,
        }
    }
}

/// Why a drag does nothing before any handler sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// An empty slot was dragged: there is nothing to drag.
    NothingDragged,
    /// Dropped onto itself: BYOND sends that as a click, never as `MouseDrop`.
    OntoItself,
}

/// The checks that need no world: something is dragged, onto something else.
pub fn validate(from: End, to: End) -> Result<(), Refusal> {
    if from.entity().is_none() {
        return Err(Refusal::NothingDragged);
    }
    if from == to {
        return Err(Refusal::OntoItself);
    }
    Ok(())
}

/// tg `IsReachableBy` of an end, given whether a map atom is adjacent: carried things and the
/// HUD are always in reach.
pub fn in_reach(end: End, adjacent: impl FnOnce(Entity) -> bool) -> bool {
    match end {
        End::World(e) => adjacent(e),
        End::Carried(_) | End::Hand(_) | End::Screen => true,
    }
}

pub struct DragServerPlugin;

impl Plugin for DragServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            drags
                .after(ResolveClicks)
                .before(crate::inventory::inventory_intents)
                .before(ss13_tgui::server::UiSystems::Open),
        );
    }
}

/// One drag per intent, in arrival order; at most tg's click limit per client and second
/// (BYOND has no `MouseDrop` limit, but each drag costs like a click).
fn drags(
    world: &mut World,
    mut intents: Local<MessageCursor<IntentReceived>>,
    mut limits: Local<EntityHashMap<(u64, u32)>>,
) {
    let second = world.resource::<Time>().elapsed_secs_f64() as u64;
    let drags: Vec<(Entity, DragEnd, DragEnd)> = intents
        .read(world.resource::<Messages<IntentReceived>>())
        .filter_map(|i| match i.intent {
            ClientIntent::Drag { from, to, .. } => Some((i.client, from, to)),
            _ => None,
        })
        .collect();
    limits.retain(|_, (s, _)| *s == second);
    for (client, from, to) in drags {
        let (s, count) = limits.entry(client).or_insert((second, 0));
        *s = second;
        *count += 1;
        if *count <= SECOND_CLICK_LIMIT {
            drag(world, client, from, to);
        }
    }
}

/// The client's mob, if it can drag at all: on the map, conscious, not incapacitated, not
/// inside a closet.
fn dragger(world: &World, client: Entity) -> Option<Entity> {
    let mob = world.resource::<Players>().mob(client)?;
    let entity = world.get_entity(mob).ok()?;
    entity.get::<PlayerMob>().filter(|p| p.client == client)?;
    if entity.get::<Stat>() != Some(&Stat::Conscious)
        || !entity.contains::<TilePos>()
        || entity.contains::<crate::closets::InCloset>()
        || entity.contains::<crate::mecha::Pilot>()
        || crate::status_effects::incapacitation(world, mob).incapacitated
    {
        return None;
    }
    Some(mob)
}

fn resolve(world: &World, client: Entity, mob: Entity, end: DragEnd) -> Option<End> {
    match end {
        DragEnd::Atom(id) => {
            let e = world.resource::<Interest>().entity(client, id)?;
            world.get::<TilePos>(e)?;
            // A pilot is inside its mech, a hidden mob inside its closet (tg `loc`).
            if world.get::<crate::mecha::Pilot>(e).is_some()
                || world.get::<crate::closets::InCloset>(e).is_some()
            {
                return None;
            }
            Some(End::World(e))
        }
        DragEnd::Slot(slot) => {
            let inventory = world.get::<Inventory>(mob)?;
            Some(match (inventory.slot(slot), slot) {
                (Some(item), _) => End::Carried(item),
                (None, InventorySlot::LeftHand) => End::Hand(0),
                (None, InventorySlot::RightHand) => End::Hand(1),
                (None, _) => End::Screen,
            })
        }
    }
}

fn drag(world: &mut World, client: Entity, from: DragEnd, to: DragEnd) {
    let Some(mob) = dragger(world, client) else {
        return;
    };
    let (Some(from), Some(to)) = (
        resolve(world, client, mob, from),
        resolve(world, client, mob, to),
    ) else {
        return;
    };
    if validate(from, to).is_err() {
        return;
    }
    // A cyborg's model and modules never leave it (`TRAIT_NODROP`; tg's model is no storage).
    if matches!(from, End::Carried(_)) && world.get::<crate::silicon::cyborg::Cyborg>(mob).is_some()
    {
        return;
    }
    let dragged = from.entity().expect("validated");
    let traits = crate::status_effects::incapacitation(world, mob);
    let actor = Actor {
        client,
        mob,
        item: world.get::<Inventory>(mob).and_then(Inventory::active_item),
    };
    let adjacent = |world: &World, e: Entity| crate::storage::in_reach(world, mob, e);

    // 1. `COMSIG_MOUSEDROP_ONTO` of the dragged thing.
    if world.get::<crate::storage::Storage>(dragged).is_some() && !traits.hands_blocked {
        let over = match to {
            End::World(e) | End::Carried(e) => DropOnto::Atom(e),
            End::Hand(hand) => DropOnto::Hand(hand),
            End::Screen => DropOnto::Screen,
        };
        if crate::storage::mousedrop_onto(world, actor, dragged, over) {
            return;
        }
    }
    if to == End::World(mob)
        && matches!(from, End::World(_))
        && adjacent(world, dragged)
        && crate::strip::open_menu(world, client, mob, dragged)
    {
        return;
    }
    // 2. `COMSIG_MOUSEDROPPED_ONTO` of the target: climbing onto a table or rack.
    if let (End::World(over), End::World(_)) = (to, from)
        && crate::tables::mousedropped_onto(world, actor, over, dragged)
    {
        return;
    }
    if let Some(over) = to.entity()
        && world.get::<crate::storage::Storage>(over).is_some()
        && crate::storage::mousedropped_onto(world, mob, over, dragged)
    {
        return;
    }
    // 3. Both ends in reach, then `mouse_drop_dragged` and `mouse_drop_receive`.
    if !in_reach(from, |e| adjacent(world, e)) || !in_reach(to, |e| adjacent(world, e)) {
        return;
    }
    // `/obj/structure/chair/mouse_drop_dragged` onto its user: picked up.
    if to == End::World(mob)
        && matches!(from, End::World(_))
        && crate::furniture::mouse_drop_dragged(world, actor, dragged)
    {
        return;
    }
    // `drag_pickup` of card decks.
    if to == End::World(mob)
        && matches!(from, End::World(_))
        && crate::service::cards::drag_pickup(world, mob, dragged)
    {
        return;
    }
    // A skateboard picked up, a wheelchair folded (`crate::vehicles`).
    if to == End::World(mob)
        && matches!(from, End::World(_))
        && crate::vehicles::mouse_drop_dragged(world, actor, dragged)
    {
        return;
    }
    // `/obj/vehicle/sealed/mouse_drop_receive` (`NEED_HANDS`): climbing into a mech.
    if let (End::World(over), End::World(_)) = (to, from)
        && !traits.hands_blocked
        && crate::mecha::core::mouse_drop_receive(world, actor, over, dragged)
    {
        return;
    }
    if let End::World(over) = to
        && matches!(from, End::World(_))
        && crate::disposals::mouse_drop_receive(world, actor, over, dragged)
    {
        return;
    }
    // `mulebot/mouse_drop_receive`: loading a MULEbot.
    if let (End::World(over), End::World(_)) = (to, from)
        && crate::bots::mule::mouse_drop_receive(world, actor, over, dragged)
    {
        return;
    }
    // `sleeper/mouse_drop_receive`.
    if let (End::World(over), End::World(_)) = (to, from)
        && crate::medical::pods::mouse_drop_receive(world, actor.mob, over, dragged)
    {
        return;
    }
    // `netpod/mouse_drop_receive`.
    if let (End::World(over), End::World(_)) = (to, from)
        && crate::science::bitrunning::avatar::mouse_drop_receive(world, over, dragged)
    {
        return;
    }
    // `dna_scannernew/mouse_drop_receive`.
    if let (End::World(over), End::World(_)) = (to, from)
        && crate::genetics::console::mouse_drop_receive(world, over, dragged)
    {
        return;
    }
    if let End::World(over) = to
        && world.get::<crate::closets::Closet>(over).is_some()
        && matches!(from, End::World(_))
    {
        crate::closets::mouse_drop_receive(world, actor, over, dragged);
        return;
    }
    // An IV drip dropped onto a mob attaches (`iv_drip/mouse_drop_dragged`); a mob dropped onto
    // an operating table or a bed is buckled (`mouse_buckle_handling`). The drip needs hands
    // (`NEED_HANDS`), buckling a user not `incapacitated`, which counts restraints.
    if let (End::World(over), End::World(_)) = (to, from)
        && !traits.hands_blocked
    {
        if world
            .get::<crate::medical::blood::IvDrip>(dragged)
            .is_some()
        {
            crate::medical::blood::iv_mouse_drop(world, actor, dragged, over);
            return;
        }
        crate::medical::surgery::mouse_drop_receive(world, actor, over, dragged);
    }
}

/// End of a [`DragJob`] `do_after` (`crate::tools` checked that neither side moved).
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: DragJob) {
    match job {
        DragJob::Dump { storage, at } => crate::storage::complete_dump(world, mob, storage, at),
        DragJob::StuffCloset { closet, at } => {
            crate::closets::complete_stuff(world, mob, action.target, closet, at);
        }
        DragJob::LoadMicrowave { storage } => {
            let held = world.get::<Inventory>(mob).and_then(Inventory::active_item);
            if held == Some(storage) {
                crate::kitchen::load_from(world, action.client, storage, action.target);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(n: u32) -> Entity {
        Entity::from_raw_u32(n).unwrap()
    }

    #[test]
    fn only_things_are_dragged_onto_other_things() {
        let (bag, floor) = (End::World(e(1)), End::World(e(2)));
        assert_eq!(validate(bag, floor), Ok(()));
        assert_eq!(validate(End::Carried(e(1)), End::Hand(1)), Ok(()));
        assert_eq!(validate(bag, End::Screen), Ok(()));
        assert_eq!(validate(End::Hand(0), floor), Err(Refusal::NothingDragged));
        assert_eq!(validate(End::Screen, bag), Err(Refusal::NothingDragged));
        assert_eq!(validate(bag, bag), Err(Refusal::OntoItself));
        assert_eq!(
            validate(End::Carried(e(3)), End::Carried(e(3))),
            Err(Refusal::OntoItself)
        );
    }

    #[test]
    fn reach_needs_adjacency_only_on_the_map() {
        assert!(in_reach(End::World(e(1)), |_| true));
        assert!(!in_reach(End::World(e(1)), |_| false));
        assert!(in_reach(End::Carried(e(1)), |_| false));
        assert!(in_reach(End::Hand(0), |_| false));
        assert!(in_reach(End::Screen, |_| false));
    }
}
