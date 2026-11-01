//! Handcuffs (server), after tg `code/game/objects/items/handcuffs.dm` (`attempt_to_cuff`,
//! `apply_cuffs`), `carbon.dm` (`set_handcuffed`, `update_handcuffed`, `resist_restraints`,
//! `cuff_resist`, `clear_cuffs`) and `carbon_update_icons.dm` (`update_worn_handcuffs`).
//!
//! - A click with handcuffs (cable cuffs, zipties: every `/obj/item/restraints/handcuffs`) on an
//!   adjacent human starts a `do_after` of the cuffs' `handcuff_time` ([`crate::tools`]: the
//!   victim stepping away, the user moving, dropping the cuffs or being stunned interrupts).
//!   At its end the cuffs leave the user's hand into the victim's [`Handcuffed`] slot: the
//!   victim drops what it holds, stops pulling, its hands are blocked (tg `TRAIT_RESTRAINED` ->
//!   `TRAIT_HANDS_BLOCKED`, `crate::status_effects::Incapacitation`) and it wears tg's
//!   `handcuff1` overlay.
//! - The resist key of a cuffed mob that can move ([`resist_restraints`]) is a `do_after` of
//!   the cuffs' `breakouttime` on itself; moving or being stunned interrupts it. At its end the
//!   cuffs drop at its feet. Used zipties cannot cuff again.
//! - Someone else takes the cuffs off through the strip menu's handcuffs entry (`crate::strip`,
//!   `/datum/strippable_item/mob_item_slot/handcuffs`): they drop at the victim's feet
//!   ([`uncuff`], `doStrip`).
// shortcut: no legcuffs, no clumsy self-cuffing, no `canBeHandcuffed` arm count (humans always have two arms here), no wirecutters
// on weak cuffs, no cyborg dispensing.

use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmHandcuffs, DmName, IconIndex};
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::{Appearance, IconStateId, TilePos};

use crate::health::DEFAULT_MESSAGE_RANGE;
use crate::inventory::{CarriedBy, Inventory};
use crate::pull::Pulling;
use crate::security::{name_of, show};
use crate::tools::{Actor, DoAfter, do_after, tell};

/// `HANDCUFF_LAYER` (`code/__DEFINES/mobs.dm`).
pub const HANDCUFF_LAYER: f32 = 5.0;
/// `update_worn_handcuffs`: `mutable_appearance('icons/mob/simple/mob.dmi', "handcuff1")`.
pub const HANDCUFF_OVERLAY: (&str, &str) = ("icons/mob/simple/mob.dmi", "handcuff1");
/// Zipties turn into used ones when they come off (`/obj/item/restraints/handcuffs/cable/
/// zipties/on_uncuffed`).
const ZIPTIES: &str = "/obj/item/restraints/handcuffs/cable/zipties";

/// tg `handcuffed`: the cuffs the mob wears (they carry [`CarriedBy`] the mob, outside every
/// inventory slot).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Handcuffed(pub Entity);

/// The timed steps of this module (`crate::construction::Job::Restraints`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestraintJob {
    /// The actor cuffs the target with the cuffs in its hand.
    Cuff,
    /// The actor breaks out of `cuffs`.
    Resist { cuffs: Entity },
}

pub struct RestraintsPlugin;

impl Plugin for RestraintsPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(forget_cuffs);
    }
}

/// Cuffs that leave the mob by any path (despawned) stop cuffing it.
fn forget_cuffs(
    remove: On<Remove<CarriedBy>>,
    carried: Query<&CarriedBy>,
    cuffed: Query<&Handcuffed>,
    mut commands: Commands,
) {
    let item = remove.entity;
    if let Ok(&CarriedBy(mob)) = carried.get(item)
        && cuffed.get(mob).is_ok_and(|c| c.0 == item)
    {
        commands.entity(mob).try_remove::<Handcuffed>();
    }
}

/// The handcuff overlay sprite of a cuffed mob, for the mob's look (`crate::inventory`).
pub fn overlay(icons: &IconIndex) -> Option<(f32, IconStateId)> {
    icons
        .get(HANDCUFF_OVERLAY.0, HANDCUFF_OVERLAY.1)
        .map(|id| (HANDCUFF_LAYER, id))
}

fn cuffs_name(world: &World, cuffs: Entity) -> String {
    world
        .get::<DmName>(cuffs)
        .map_or_else(|| "наручники".to_owned(), |n| n.name.clone())
}

/// tg `/obj/item/restraints/handcuffs/attack` -> `attempt_to_cuff`: starts cuffing `victim`
/// with the active hand's `cuffs`. True when the click was the cuffs' (handled).
pub fn attempt_to_cuff(world: &mut World, actor: Actor, cuffs: Entity, victim: Entity) -> bool {
    let Some(dm) = world.get::<DmHandcuffs>(cuffs).cloned() else {
        return false;
    };
    // `iscarbon(target_mob) || used`: anything else is not the cuffs' business.
    if world.get::<Inventory>(victim).is_none() || dm.used {
        return true;
    }
    if world.get::<Handcuffed>(victim).is_some() {
        tell(world, actor.client, Span::Warning, "Уже в наручниках!");
        return true;
    }
    if world.get::<DoAfter>(actor.mob).is_some() {
        return true;
    }
    let (user, target, item) = (
        name_of(world, actor.mob),
        name_of(world, victim),
        cuffs_name(world, cuffs),
    );
    let Some(&at) = world.get::<TilePos>(victim) else {
        return true;
    };
    if !do_after(
        world,
        actor,
        victim,
        dm.handcuff_time,
        0,
        crate::construction::Job::Restraints(RestraintJob::Cuff),
    ) {
        return true;
    }
    show(
        world,
        at,
        DEFAULT_MESSAGE_RANGE,
        ChatChannel::Combat,
        (
            Span::Danger,
            format!("{user} пытается надеть {item} на {target}!"),
        ),
        &[(
            victim,
            Span::BoldWarning,
            format!("{user} пытается надеть {item} на вас!"),
        )],
    );
    true
}

/// End of a [`RestraintJob`] `do_after` (already checked by `crate::tools`: same tiles, same
/// held item, conscious and not incapacitated). The state is checked again here.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: RestraintJob) {
    let target = action.target;
    match job {
        RestraintJob::Cuff => {
            let Some(cuffs) = action.item else {
                return;
            };
            if world.get::<Handcuffed>(target).is_some()
                || world.get::<DmHandcuffs>(cuffs).is_none_or(|c| c.used)
            {
                tell(
                    world,
                    action.client,
                    Span::Warning,
                    format!(
                        "Вам не удалось надеть наручники на {}!",
                        name_of(world, target)
                    ),
                );
                return;
            }
            apply_cuffs(world, cuffs, target);
            let (user, victim) = (name_of(world, mob), name_of(world, target));
            show(
                world,
                action.target_pos,
                DEFAULT_MESSAGE_RANGE,
                ChatChannel::Combat,
                (
                    Span::Notice,
                    format!("{user} надевает наручники на {victim}."),
                ),
                &[(
                    target,
                    Span::BoldWarning,
                    format!("{user} надевает на вас наручники."),
                )],
            );
        }
        RestraintJob::Resist { cuffs } => {
            if world.get::<Handcuffed>(mob) != Some(&Handcuffed(cuffs)) {
                return;
            }
            let (user, item) = (name_of(world, mob), cuffs_name(world, cuffs));
            uncuff(world, mob, action.user_pos);
            // `clear_cuffs`.
            show(
                world,
                action.user_pos,
                DEFAULT_MESSAGE_RANGE,
                ChatChannel::Combat,
                (Span::Danger, format!("{user} снимает {item}!")),
                &[(mob, Span::Notice, format!("Вы успешно снимаете {item}."))],
            );
        }
    }
}

/// tg `apply_cuffs` -> `equip_to_slot(ITEM_SLOT_HANDCUFFED)` -> `set_handcuffed` +
/// `update_handcuffed`: out of the user's hand onto `victim`, who drops what it holds and stops
/// pulling.
pub fn apply_cuffs(world: &mut World, cuffs: Entity, victim: Entity) {
    let Some(&pos) = world.get::<TilePos>(victim) else {
        return;
    };
    world.entity_mut(cuffs).remove::<CarriedBy>();
    world
        .entity_mut(cuffs)
        .insert(CarriedBy(victim))
        .remove::<TilePos>();
    let held: Vec<Entity> = world
        .get::<Inventory>(victim)
        .map(|i| i.hands.iter().flatten().copied().collect())
        .unwrap_or_default();
    for item in held {
        world.entity_mut(item).remove::<CarriedBy>().insert(pos);
    }
    world.entity_mut(victim).insert(Handcuffed(cuffs));
    world.entity_mut(victim).remove::<Pulling>();
    // The look gains the overlay.
    if let Some(mut inventory) = world.get_mut::<Inventory>(victim) {
        inventory.set_changed();
    }
}

/// The cuffs come off `mob` and drop at `pos` (tg `dropItemToGround`); zipties are used up.
pub fn uncuff(world: &mut World, mob: Entity, pos: TilePos) {
    let Some(Handcuffed(cuffs)) = world.get::<Handcuffed>(mob).copied() else {
        return;
    };
    world.entity_mut(mob).remove::<Handcuffed>();
    world.entity_mut(cuffs).remove::<CarriedBy>().insert(pos);
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        inventory.set_changed();
    }
    let ziptie = world
        .get::<DmHandcuffs>(cuffs)
        .is_some_and(|c| ss13_mapload::fidelity::is_under(&c.path, ZIPTIES));
    if ziptie {
        used_zipties(world, cuffs);
    }
}

/// `zipties/on_uncuffed`: "A pair of broken zipties.", `cuff_used`, `used`.
fn used_zipties(world: &mut World, cuffs: Entity) {
    if let Some(mut dm) = world.get_mut::<DmHandcuffs>(cuffs) {
        dm.used = true;
    }
    if let Some(mut name) = world.get_mut::<DmName>(cuffs) {
        name.desc = "A pair of broken zipties.".to_owned();
    }
    let dmi = world.get::<DmAtom>(cuffs).map(|a| a.icon.clone());
    let state = dmi.and_then(|dmi| {
        world
            .get_resource::<IconIndex>()
            .and_then(|icons| icons.get(&dmi, "cuff_used"))
    });
    if let (Some(state), Some(mut appearance)) = (state, world.get_mut::<Appearance>(cuffs)) {
        appearance.icon_state = state;
    }
}

/// tg `resist_restraints` -> `cuff_resist`: a `do_after` of the cuffs' `breakouttime` on the
/// mob itself.
pub fn resist_restraints(world: &mut World, client: Entity, mob: Entity) {
    let Some(Handcuffed(cuffs)) = world.get::<Handcuffed>(mob).copied() else {
        return;
    };
    let item = cuffs_name(world, cuffs);
    if world.get::<DoAfter>(mob).is_some() {
        // `DOING_INTERACTION(src, REF(cuffs))`.
        tell(
            world,
            client,
            Span::Warning,
            format!("Вы уже пытаетесь снять {item}!"),
        );
        return;
    }
    let breakout = world
        .get::<DmHandcuffs>(cuffs)
        .map_or(600.0, |c| c.breakouttime);
    let actor = Actor {
        client,
        mob,
        item: None,
    };
    let job = crate::construction::Job::Restraints(RestraintJob::Resist { cuffs });
    if !do_after(world, actor, mob, breakout, 0, job) {
        return;
    }
    let Some(&at) = world.get::<TilePos>(mob) else {
        return;
    };
    let user = name_of(world, mob);
    show(
        world,
        at,
        DEFAULT_MESSAGE_RANGE,
        ChatChannel::Combat,
        (Span::Warning, format!("{user} пытается снять {item}!")),
        &[(
            mob,
            Span::Notice,
            format!(
                "Вы пытаетесь снять {item}... (Это займет примерно {}, и вы не должны двигаться.)",
                display_time(breakout / 10.0)
            ),
        )],
    );
}

/// DM `DisplayTimeText` for whole seconds and minutes, in Russian.
pub fn display_time(secs: f32) -> String {
    let secs = secs.round().max(0.0) as u32;
    let plural = |n: u32, one: &'static str, few: &'static str, many: &'static str| {
        let (d, h) = (n % 10, n % 100);
        if d == 1 && h != 11 {
            one
        } else if (2..=4).contains(&d) && !(12..=14).contains(&h) {
            few
        } else {
            many
        }
    };
    let (minutes, rest) = (secs / 60, secs % 60);
    let mut parts = Vec::new();
    if minutes > 0 {
        parts.push(format!(
            "{minutes} {}",
            plural(minutes, "минута", "минуты", "минут")
        ));
    }
    if rest > 0 || minutes == 0 {
        parts.push(format!(
            "{rest} {}",
            plural(rest, "секунда", "секунды", "секунд")
        ));
    }
    parts.join(" и ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_time_reads_like_dm() {
        assert_eq!(display_time(60.0), "1 минута");
        assert_eq!(display_time(45.0), "45 секунд");
        assert_eq!(display_time(30.0), "30 секунд");
        assert_eq!(display_time(1.0), "1 секунда");
        assert_eq!(display_time(92.0), "1 минута и 32 секунды");
        assert_eq!(display_time(0.0), "0 секунд");
    }
}
