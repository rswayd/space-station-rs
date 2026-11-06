//! Mobs in hands (tg `code/modules/mob/living/inhand_holder.dm`, `/datum/element/can_be_held`,
//! `/mob/living/proc/mob_try_pickup`, `mob_pickup`).
//!
//! - Small mobs whose type adds `can_be_held` (mice, cats, corgis, snails, frogs, ...;
//!   `MobDef::can_be_held`) and a pAI's holochassis of a carriable chassis are picked up by
//!   dragging them onto oneself while grabbing them aggressively: 2 s (`do_after`), then a
//!   `/obj/item/mob_holder` (a snail's `/obj/item/mob_holder/snail`) goes into an empty hand,
//!   looking like the mob, in hand as its `held_state` from `held_lh`/`held_rh`, worn in its
//!   `worn_slot_flags` from `head_icon`.
//! - The mob rides inside ([`crate::closets::InCloset`] with the holder as its container): hidden
//!   and unclickable, it follows the holder's turf (hand, bag, tray).
//! - It is let out (`release`) when the holder lands on a floor (dropped, thrown), when the mob
//!   tries to move (`relaymove`), when the holder is deleted, or when a pAI folds into its
//!   card; the carrier reads "X вырывается из рук!".
// shortcut: no hitting the mob through its holder (`attack_self`, items used on it), no
// holder weight class, no throwing the mob out (a thrown holder releases it where it lands),
// no `/obj/item/mob_holder/drone` hiding sprite or destructible bee holders, no pickups by
// non-human mobs (`mob_size` checks).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, TilePos};

use crate::closets::{ClosetRelayMove, InCloset};
use crate::inventory::{CarriedBy, Inventory};
use crate::storage::StoredIn;
use crate::tools::{Actor, DoAfter, tell};

pub const HOLDER: &str = "/obj/item/mob_holder";
pub const SNAIL_HOLDER: &str = "/obj/item/mob_holder/snail";
pub const SNAIL: &str = "/mob/living/basic/snail";
/// `mob_try_pickup`'s `do_after`, deciseconds.
const PICKUP_TIME: f32 = 20.0;
const PAI_LH: &str = "icons/mob/inhands/pai_item_lh.dmi";
const PAI_RH: &str = "icons/mob/inhands/pai_item_rh.dmi";
const PAI_HEAD: &str = "icons/mob/clothing/head/pai_head.dmi";
/// `ITEM_SLOT_HEAD`.
const ITEM_SLOT_HEAD: u32 = 1 << 6;
/// tg `possible_chassis` entries that can be carried.
const PAI_CARRIABLE: [&str; 11] = [
    "cat",
    "crow",
    "duffel",
    "frog",
    "monkey",
    "mouse",
    "rabbit",
    "repairbot",
    "kitten",
    "puppy",
    "spider",
];

/// On a holder item: the mob inside it.
#[derive(Component, Clone, Copy, Debug)]
pub struct MobHolder {
    pub mob: Entity,
}

pub struct MobHolderPlugin;

impl Plugin for MobHolderPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(release_on_despawn)
            .add_systems(Update, (relay_moves, follow_holders).chain());
    }
}

/// What a holder of `mob` looks like: its in-hand state, the in-hand files, the head file and
/// the worn slots. `None`: the mob cannot be held now.
struct Holdable {
    state: String,
    lh: String,
    rh: String,
    head: String,
    slots: u32,
}

fn holdable(world: &World, mob: Entity) -> Option<Holdable> {
    if let Some(pai) = world.get::<crate::silicon::pai::Pai>(mob) {
        return pai.holoform.then(|| Holdable {
            state: pai.chassis.to_owned(),
            lh: PAI_LH.to_owned(),
            rh: PAI_RH.to_owned(),
            head: PAI_HEAD.to_owned(),
            slots: ITEM_SLOT_HEAD,
        });
    }
    let basic = world.get::<crate::mobs::BasicMob>(mob)?;
    let def = &basic.def;
    if !def.can_be_held {
        return None;
    }
    // BandaStation mice: `held_state = "[body_icon_state]_[body_color]"`, their living sprite.
    let state = if !def.body_icon_state.is_empty() || def.held_state.is_empty() {
        basic.icon_living.clone()
    } else {
        def.held_state.clone()
    };
    Some(Holdable {
        state,
        lh: def.held_lh.clone(),
        rh: def.held_rh.clone(),
        head: def.head_icon.clone(),
        slots: def.worn_slot_flags,
    })
}

fn name(world: &World, e: Entity) -> String {
    crate::chem::name_of(world, e)
}

fn carried_mob(world: &World, e: Entity) -> Option<Entity> {
    world.get::<CarriedBy>(e).map(|c| c.0)
}

/// `COMSIG_MOUSEDROP_ONTO` of a holdable mob dropped onto `actor.mob` by a user grabbing it
/// aggressively (`trying_to_hold_mob`): `mob_try_pickup` starts. True when the drop was its.
pub fn mousedrop_onto(world: &mut World, actor: Actor, target: Entity) -> bool {
    let user = actor.mob;
    let grabbing = world
        .get::<crate::pull::Pulling>(user)
        .is_some_and(|p| p.0 == target)
        && world.get::<crate::pull::GrabState>(user) == Some(&crate::pull::GrabState::Aggressive);
    if !grabbing {
        return false;
    }
    if world
        .get::<crate::silicon::pai::Pai>(target)
        .is_some_and(|p| p.holoform && !PAI_CARRIABLE.contains(&p.chassis))
    {
        let text = format!(
            "{}'s current form isn't able to be carried!",
            name(world, target)
        );
        tell(world, actor.client, Span::Warning, text);
        return true;
    }
    if holdable(world, target).is_none() {
        return false;
    }
    try_pickup(world, actor, target);
    true
}

/// `mob_try_pickup(user)`: free hands, not buckled, then 2 s.
fn try_pickup(world: &mut World, actor: Actor, target: Entity) {
    let free = world
        .get::<Inventory>(actor.mob)
        .is_some_and(|i| i.hands.iter().any(Option::is_none));
    if !free {
        tell(world, actor.client, Span::Warning, "Ваши руки заняты!");
        return;
    }
    if world
        .get::<crate::medical::surgery::Buckled>(target)
        .is_some()
    {
        let text = format!("Нужно отстегнуть {}!", name(world, target));
        tell(world, actor.client, Span::Warning, text);
        return;
    }
    let (user, victim) = (name(world, actor.mob), name(world, target));
    crate::chem::visible(
        world,
        actor.mob,
        &format!("Вы начинаете подбирать {victim}..."),
        &format!("{user} начинает подбирать {victim}!"),
        Span::Warning,
    );
    if let Some(client) = world
        .get::<crate::server::PlayerMob>(target)
        .map(|p| p.client)
    {
        tell(
            world,
            client,
            Span::Danger,
            format!("{user} начинает подбирать вас!"),
        );
    }
    crate::tools::do_after(
        world,
        actor,
        target,
        PICKUP_TIME,
        0,
        crate::construction::Job::Interact(crate::drag::DragJob::PickupMob),
    );
}

/// The end of the pickup: the mob still holdable and loose, a hand still free.
pub fn complete_pickup(world: &mut World, user: Entity, action: &DoAfter) {
    let target = action.target;
    if world.get::<InCloset>(target).is_some()
        || world
            .get::<Inventory>(user)
            .is_none_or(|i| i.hands.iter().all(Option::is_some))
    {
        return;
    }
    pickup(world, user, target);
}

/// `mob_pickup(user)`: a new holder with the mob inside, into the user's hands. Returns it.
pub fn pickup(world: &mut World, user: Entity, target: Entity) -> Option<Entity> {
    let look = holdable(world, target)?;
    let path = if world
        .get::<crate::mobs::BasicMob>(target)
        .is_some_and(|b| is_under(&b.def.path, SNAIL))
    {
        SNAIL_HOLDER
    } else {
        HOLDER
    };
    let holder = crate::construction::spawn_detached(world, path)?;
    if let Some(mut item) = world.get_mut::<DmItem>(holder) {
        item.inhand_icon_state = look.state;
        if !look.lh.is_empty() {
            item.lefthand_file = look.lh;
        }
        if !look.rh.is_empty() {
            item.righthand_file = look.rh;
        }
        if !look.head.is_empty() {
            item.worn_icon = look.head;
        }
        if look.slots != 0 {
            item.slot_flags = look.slots;
        }
    }
    // `update_visuals`: `appearance = held_guy.appearance`; `name = new_prisoner.name`.
    let appearance = world.get::<Appearance>(target).cloned();
    let mob_name = name(world, target);
    {
        let mut entity = world.entity_mut(holder);
        if let Some(appearance) = appearance {
            entity.insert(appearance);
        }
        entity.insert((Name::new(mob_name.clone()), MobHolder { mob: target }));
    }
    crate::closets::put_mob_inside(world, target, holder);
    let user_name = name(world, user);
    crate::chem::visible(
        world,
        user,
        &format!("Вы подбираете {mob_name}!"),
        &format!("{user_name} подбирает {mob_name}!"),
        Span::Warning,
    );
    if let Some(inventory) = world.get::<Inventory>(user).cloned() {
        crate::storage::put_in_hands(world, user, &inventory, holder);
    }
    if let Some(pos) = crate::explosions::turf_of(world, holder) {
        set_pos(world, target, pos);
    }
    Some(holder)
}

fn set_pos(world: &mut World, mob: Entity, pos: TilePos) {
    if world.get::<TilePos>(mob) != Some(&pos) {
        world.entity_mut(mob).insert(pos);
    }
}

/// tg `release`: the mob out onto the holder's turf, the holder gone.
pub fn release(world: &mut World, holder: Entity, messages: bool) {
    let Some(MobHolder { mob }) = world.get::<MobHolder>(holder).copied() else {
        return;
    };
    let pos = crate::explosions::turf_of(world, holder);
    let mob_name = name(world, mob);
    if messages
        && let Some(captor) = carried_mob(world, holder)
        && let Some(client) = world
            .get::<crate::server::PlayerMob>(captor)
            .map(|p| p.client)
    {
        tell(
            world,
            client,
            Span::Warning,
            format!("{mob_name} вырывается из рук!"),
        );
    }
    // A snail leaving its tray (`hydroponics/Exited`).
    if let Some(tray) = world.get::<StoredIn>(holder).map(|s| s.0) {
        crate::botany::snail_left(world, tray, holder);
    }
    world.entity_mut(holder).remove::<MobHolder>();
    crate::closets::let_out(world, mob);
    if let Some(pos) = pos {
        set_pos(world, mob, pos);
    }
    world.despawn(holder);
    if messages {
        crate::chem::visible(
            world,
            mob,
            "",
            &format!("{mob_name} расправляется!"),
            Span::Warning,
        );
    }
}

/// A holder deleted by any other path lets its mob out where it last was.
fn release_on_despawn(
    remove: On<Remove<MobHolder>>,
    holders: Query<&MobHolder>,
    mut commands: Commands,
) {
    let Ok(&MobHolder { mob }) = holders.get(remove.entity) else {
        return;
    };
    let holder = remove.entity;
    commands.queue(move |world: &mut World| {
        if world
            .get::<InCloset>(mob)
            .is_some_and(|c| c.closet == holder)
        {
            crate::closets::let_out(world, mob);
        }
    });
}

/// `relaymove`, `container_resist_act`: a step of the mob inside lets it out.
fn relay_moves(world: &mut World, mut moves: Local<MessageCursor<ClosetRelayMove>>) {
    let Some(messages) = world.get_resource::<Messages<ClosetRelayMove>>() else {
        return;
    };
    let mobs: Vec<Entity> = moves.read(messages).map(|m| m.mob).collect();
    for mob in mobs {
        if world.get::<crate::health::Stat>(mob) != Some(&crate::health::Stat::Conscious) {
            continue;
        }
        let Some(holder) = world.get::<InCloset>(mob).map(|c| c.closet) else {
            continue;
        };
        if world.get::<MobHolder>(holder).is_some() {
            release(world, holder, true);
        }
    }
}

/// Every tick: a mob follows its holder's turf; a holder on the floor (`Moved` onto a turf)
/// or a pAI no longer in its holochassis lets it out.
// Cost: one pass over the holders (a handful in a round) per tick.
fn follow_holders(world: &mut World) {
    let mut query = world.query::<(Entity, &MobHolder)>();
    let holders: Vec<(Entity, Entity)> = query.iter(world).map(|(e, h)| (e, h.mob)).collect();
    for (holder, mob) in holders {
        if world.get_entity(mob).is_err() {
            // `on_mob_deleted`.
            world.entity_mut(holder).remove::<MobHolder>();
            world.despawn(holder);
            continue;
        }
        let on_floor = world.get::<TilePos>(holder).is_some()
            && world.get::<CarriedBy>(holder).is_none()
            && world.get::<StoredIn>(holder).is_none();
        let folded = world
            .get::<crate::silicon::pai::Pai>(mob)
            .is_some_and(|p| !p.holoform);
        if folded {
            // The pAI is back in its card, where `fold_in` put it.
            world.entity_mut(holder).remove::<MobHolder>();
            crate::closets::let_out(world, mob);
            world.despawn(holder);
            continue;
        }
        if on_floor {
            release(world, holder, true);
            continue;
        }
        if let Some(pos) = crate::explosions::turf_of(world, holder) {
            set_pos(world, mob, pos);
        }
    }
}
