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
//! - A straight jacket worn as the suit restrains too ([`SuitRestrained`], tg `wear_suit.
//!   breakouttime` -> `TRAIT_RESTRAINED`): hands blocked, pulling stops. Resisting is a 5 minute
//!   `do_after`, after which it drops at the wearer's feet (`cuff_resist(wear_suit)`); taking it
//!   off by any path drops what the wearer holds (`doUnEquip`: "suit is restraining").
//! - Only a mob with both hands can be cuffed (`canBeHandcuffed`), checked again at the end.
//!   The cuffs' sound plays at the start and the end; a blind victim feels them dig in.
//! - Cuffsnapping (`/datum/element/cuffsnapping`): a right click of wirecutters on a cuffed mob
//!   cuts weak cuffs (cable, zipties, fakes) at once; alien wirecutters cut handcuffs in 1 s,
//!   the jaws of life in cutter mode at once; else "but fails" ([`cuffsnap`]).
//! - Cult shackles and hard-light energy cuffs (abductor, voidwalker) shatter when they come off
//!   (`on_uncuffed`).
//! - `cuff_break`: a Hulk breaks its cuffs in 5 s (`FAST_CUFFBREAK`, with a roar), an adult
//!   alien at once (`INSTANT_CUFFBREAK`); broken cuffs are gone.
// shortcut: the straight jacket's `equip_delay_self` (5 s) and `strip_delay` (6 s) are not
// ported: the inventory has no self-equip delays and the strip menu uses tg's defaults.
// shortcut: no boxcutter cuffsnapping, no cuffed items (`/datum/status_effect/cuffed_item`), no
// `resist_cooldown` of the fake cuffs.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
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
/// `HANDCUFFS_TYPE_WEAK` cuffs: cable (zipties included) and the fakes.
const CABLE_CUFFS: &str = "/obj/item/restraints/handcuffs/cable";
const FAKE_CUFFS: &str = "/obj/item/restraints/handcuffs/fake";
/// Cuffs that shatter when they come off (`cult/on_uncuffed`, `energy/on_uncuffed`).
const CULT_CUFFS: &str = "/obj/item/restraints/handcuffs/cult";
const ENERGY_CUFFS: &str = "/obj/item/restraints/handcuffs/energy";
/// `cuffsound` (cable restraints: `cablecuff.ogg`), `cuffsuccesssound`.
const CUFF_SOUND: &str = "sound/items/weapons/handcuffs.ogg";
const CABLE_CUFF_SOUND: &str = "sound/items/weapons/cablecuff.ogg";
const CUFF_SUCCESS_SOUND: &str = "sound/items/handcuff_finish.ogg";
/// Cuffsnapping tools: alien wirecutters (`snap_time_strong_handcuffs = 1 SECONDS`) and the
/// jaws of life (0 for both).
const ABDUCTOR_WIRECUTTERS: &str = "/obj/item/wirecutters/abductor";
const JAWS_OF_LIFE: &str = "/obj/item/crowbar/power";
/// `FAST_CUFFBREAK`'s 5 s, deciseconds.
const FAST_CUFFBREAK_DS: f32 = 50.0;
/// The Hulk's roars of `human/cuff_resist`.
// shortcut: said aloud, not over the common radio (tg's `;` prefix).
const HULK_ROARS: [&str; 5] = [
    "RAAAAAAAARGH!",
    "HNNNNNNNNNGGGGGGH!",
    "GWAAAAAAAARRRHHH!",
    "NNNNNNNNGGGGGGGGHH!",
    "AAAAAAARRRGH!",
];

/// tg `handcuffed`: the cuffs the mob wears (they carry [`CarriedBy`] the mob, outside every
/// inventory slot).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Handcuffed(pub Entity);

/// `/obj/item/clothing/suit/jacket/straight_jacket`, the only suit with a `breakouttime`.
pub const STRAIGHT_JACKET: &str = "/obj/item/clothing/suit/jacket/straight_jacket";
/// Its `breakouttime` (5 minutes), deciseconds.
pub const STRAIGHT_JACKET_BREAKOUT: f32 = 3000.0;

/// The restraining suit (a straight jacket) the mob wears (tg `TRAIT_RESTRAINED` from
/// `SUIT_TRAIT`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SuitRestrained(pub Entity);

/// The timed steps of this module (`crate::construction::Job::Restraints`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestraintJob {
    /// The actor cuffs the target with the cuffs in its hand.
    Cuff,
    /// The actor breaks out of `cuffs`.
    Resist { cuffs: Entity },
    /// The actor gets out of the restraining `suit`.
    ResistSuit { suit: Entity },
    /// The actor gets out of the leg restraints (`crate::legcuffs`).
    ResistLegs { cuffs: Entity },
    /// The actor breaks its hand or leg `cuffs` (`FAST_CUFFBREAK`).
    Break { cuffs: Entity },
    /// The actor cuts the target's `cuffs` with the held cutter (cuffsnapping).
    Snap { cuffs: Entity },
}

pub struct RestraintsPlugin;

impl Plugin for RestraintsPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(forget_cuffs)
            .add_systems(Update, sync_suit_restraint);
    }
}

/// Cuffs that leave the mob by any path (despawned) stop cuffing it.
fn forget_cuffs(
    remove: On<Remove<CarriedBy>>,
    carried: Query<&CarriedBy>,
    cuffed: Query<&Handcuffed>,
    legcuffed: Query<&crate::legcuffs::Legcuffed>,
    mut commands: Commands,
) {
    let item = remove.entity;
    let Ok(&CarriedBy(mob)) = carried.get(item) else {
        return;
    };
    if cuffed.get(mob).is_ok_and(|c| c.0 == item) {
        commands.entity(mob).try_remove::<Handcuffed>();
    }
    if legcuffed.get(mob).is_ok_and(|c| c.0 == item) {
        commands
            .entity(mob)
            .try_remove::<crate::legcuffs::Legcuffed>();
    }
}

/// tg human `equip_to_slot`/`doUnEquip` of a suit with a `breakouttime`: wearing one
/// restrains and stops pulling; losing it drops the held items.
fn sync_suit_restraint(
    mobs: Query<(Entity, &Inventory, Option<&SuitRestrained>), Changed<Inventory>>,
    items: Query<&ss13_mapload::DmItem>,
    mut commands: Commands,
) {
    for (mob, inventory, current) in &mobs {
        let jacket = inventory.suit.filter(|&s| {
            items
                .get(s)
                .is_ok_and(|i| ss13_mapload::fidelity::is_under(&i.path, STRAIGHT_JACKET))
        });
        match (jacket, current) {
            (Some(suit), Some(&SuitRestrained(now))) if suit == now => {}
            (Some(suit), _) => {
                commands
                    .entity(mob)
                    .insert(SuitRestrained(suit))
                    .remove::<Pulling>();
            }
            (None, Some(_)) => {
                let held: Vec<Entity> = inventory.hands.iter().flatten().copied().collect();
                commands.entity(mob).remove::<SuitRestrained>();
                commands.queue(move |world: &mut World| {
                    let Some(&pos) = world.get::<TilePos>(mob) else {
                        return;
                    };
                    for item in held {
                        let still = world
                            .get::<Inventory>(mob)
                            .is_some_and(|i| i.hands.contains(&Some(item)));
                        if still {
                            if let Some(mut inv) = world.get_mut::<Inventory>(mob) {
                                for hand in inv.hands.iter_mut() {
                                    if *hand == Some(item) {
                                        *hand = None;
                                    }
                                }
                            }
                            crate::inventory::put_down(world, item, pos);
                        }
                    }
                });
            }
            (None, None) => {}
        }
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
    // `handcuffs_clumsiness_check`: a clumsy carbon cuffs itself half the time.
    if world.get::<Inventory>(actor.mob).is_some()
        && crate::genetics::is_clumsy(world, actor.mob)
        && world
            .resource_mut::<crate::combat::CombatRng>()
            .chance(50.0)
    {
        tell(
            world,
            actor.client,
            Span::Warning,
            "Э-э... как эти штуки работают?!",
        );
        if world.get::<Handcuffed>(actor.mob).is_none()
            && let Some(&pos) = world.get::<TilePos>(actor.mob)
        {
            crate::inventory::put_down(world, cuffs, pos);
            apply_cuffs(world, cuffs, actor.mob);
        }
        return true;
    }
    if world.get::<Handcuffed>(victim).is_some() {
        tell(world, actor.client, Span::Warning, "Уже в наручниках!");
        return true;
    }
    if !can_be_handcuffed(world, victim) {
        tell(world, actor.client, Span::Warning, "нельзя заковать!");
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
    // `get_handcuff_time`: `TRAIT_FAST_CUFFING` (MOD restraint assist) cuffs in 0.75 of it.
    let fast = world
        .get::<crate::modsuit::modules::FastCuffing>(actor.mob)
        .is_some()
        || crate::clothing::traits::has_trait(
            world,
            actor.mob,
            crate::clothing::traits::FAST_CUFFING,
        );
    let time = if fast {
        dm.handcuff_time * crate::modsuit::modules::FAST_CUFFING_FACTOR
    } else {
        dm.handcuff_time
    };
    if !do_after(
        world,
        actor,
        victim,
        time,
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
    if crate::vision::is_blind(world, victim)
        && let Some(client) = world
            .get::<crate::server::PlayerMob>(victim)
            .map(|p| p.client)
    {
        tell(
            world,
            client,
            Span::BoldWarning,
            format!("Кто-то хватает вас за запястья, и {item} впиваются в вашу кожу!"),
        );
    }
    let sound = if is_under(&dm.path, CABLE_CUFFS) && !is_under(&dm.path, ZIPTIES) {
        CABLE_CUFF_SOUND
    } else {
        CUFF_SOUND
    };
    play(world, at, sound);
    true
}

/// `playsound(loc, sound, 30, TRUE, -2)` of the cuffs.
fn play(world: &mut World, at: TilePos, sound: &str) {
    let mut sound = crate::sound::PlaySound::new(at, sound, 30.0, true);
    sound.extrarange = -2.0;
    crate::sound::playsound(world, sound);
}

/// tg human `canBeHandcuffed`: both hands (`num_hands >= 2`).
fn can_be_handcuffed(world: &World, victim: Entity) -> bool {
    (0..2).all(|h| crate::medical::bodyparts::has_hand(world, victim, h))
}

/// `HANDCUFFS_TYPE_WEAK` (`restraint_strength`).
fn weak_cuffs(world: &World, cuffs: Entity) -> bool {
    world
        .get::<DmHandcuffs>(cuffs)
        .is_some_and(|c| is_under(&c.path, CABLE_CUFFS) || is_under(&c.path, FAKE_CUFFS))
}

/// `/datum/element/cuffsnapping` of `cutter`: the deciseconds it takes to cut weak cuffs and
/// strong ones (`None`: it cannot).
fn snap_times(world: &World, cutter: Entity) -> Option<(f32, Option<f32>)> {
    let (kind, _) = crate::tools::tool_of(world, cutter)?;
    if kind != crate::tools::ToolKind::Wirecutters {
        return None;
    }
    let path = world.get::<ss13_mapload::DmItem>(cutter)?.path.clone();
    Some(if is_under(&path, JAWS_OF_LIFE) {
        (0.0, Some(0.0))
    } else if is_under(&path, ABDUCTOR_WIRECUTTERS) {
        (0.0, Some(10.0))
    } else {
        (0.0, None)
    })
}

/// `cuffsnapping/try_cuffsnap_target`: a right click of a cutter on a cuffed mob. True when it
/// took the click.
pub fn cuffsnap(world: &mut World, actor: Actor, cutter: Entity, target: Entity) -> bool {
    let Some(Handcuffed(cuffs)) = world.get::<Handcuffed>(target).copied() else {
        return false;
    };
    let Some((weak_time, strong_time)) = snap_times(world, cutter) else {
        return false;
    };
    if world.get::<DoAfter>(actor.mob).is_some() {
        return true;
    }
    let time = if weak_cuffs(world, cuffs) {
        Some(weak_time)
    } else {
        strong_time
    };
    let Some(time) = time else {
        // `check_cuffs_strength`.
        let (user, victim, tool) = (
            name_of(world, actor.mob),
            name_of(world, target),
            name_of(world, cutter),
        );
        if let Some(&at) = world.get::<TilePos>(target) {
            show(
                world,
                at,
                DEFAULT_MESSAGE_RANGE,
                ChatChannel::Combat,
                (
                    Span::Notice,
                    format!(
                        "{user} пытается перерезать наручники {victim} с помощью {tool}, но безуспешно!"
                    ),
                ),
                &[],
            );
        }
        return true;
    };
    if time > 0.0 {
        do_after(
            world,
            actor,
            target,
            time,
            0,
            crate::construction::Job::Restraints(RestraintJob::Snap { cuffs }),
        );
    } else {
        snap(world, actor.mob, cutter, target, cuffs);
    }
    true
}

/// `do_snip_snap`: the cuffs are cut (gone).
fn snap(world: &mut World, user: Entity, cutter: Entity, target: Entity, cuffs: Entity) {
    if world.get::<Handcuffed>(target) != Some(&Handcuffed(cuffs)) {
        return;
    }
    crate::animation::do_attack_animation_with(
        world,
        user,
        target,
        crate::animation::AttackVisual::Item(cutter),
    );
    let (name_user, victim, tool) = (
        name_of(world, user),
        name_of(world, target),
        name_of(world, cutter),
    );
    world.entity_mut(target).remove::<Handcuffed>();
    world.despawn(cuffs);
    if let Some(mut inventory) = world.get_mut::<Inventory>(target) {
        inventory.set_changed();
    }
    if let Some(&at) = world.get::<TilePos>(target) {
        show(
            world,
            at,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Combat,
            (
                Span::Notice,
                format!("{name_user} перерезает наручники {victim} с помощью {tool}!"),
            ),
            &[],
        );
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(at, "sound/items/tools/wirecutter.ogg", 50.0, true),
        );
    }
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
                || !can_be_handcuffed(world, target)
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
            // `apply_cuffs(dispense = iscyborg(user))`: a cyborg's module stays and a new pair
            // of its type goes on. (tg then `qdel(src)`s the module, an obvious bug: the
            // cyborg would lose its only cuffs after one use.)
            let worn = if world.get::<crate::silicon::cyborg::Cyborg>(mob).is_some() {
                let new = world
                    .get::<DmHandcuffs>(cuffs)
                    .map(|c| c.path.clone())
                    .and_then(|path| {
                        crate::construction::spawn_obj(world, &path, action.target_pos)
                    });
                let Some(new) = new else {
                    return;
                };
                new
            } else {
                cuffs
            };
            apply_cuffs(world, worn, target);
            play(world, action.target_pos, CUFF_SUCCESS_SOUND);
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
        RestraintJob::ResistSuit { suit } => {
            let worn = world.get::<Inventory>(mob).and_then(|i| i.suit);
            if world.get::<SuitRestrained>(mob) != Some(&SuitRestrained(suit)) || worn != Some(suit)
            {
                return;
            }
            let (user, item) = (name_of(world, mob), cuffs_name(world, suit));
            let pos = action.user_pos;
            crate::inventory::unequip(world, mob, pos, ss13_shared::protocol::InventorySlot::Suit);
            crate::inventory::put_down(world, suit, pos);
            show(
                world,
                pos,
                DEFAULT_MESSAGE_RANGE,
                ChatChannel::Combat,
                (Span::Danger, format!("{user} снимает {item}!")),
                &[(mob, Span::Notice, format!("Вы успешно снимаете {item}."))],
            );
        }
        RestraintJob::ResistLegs { cuffs } => {
            let current = world.get::<crate::legcuffs::Legcuffed>(mob);
            if current != Some(&crate::legcuffs::Legcuffed(cuffs)) {
                return;
            }
            let (user, item) = (name_of(world, mob), cuffs_name(world, cuffs));
            crate::legcuffs::remove(world, mob, action.user_pos);
            show(
                world,
                action.user_pos,
                DEFAULT_MESSAGE_RANGE,
                ChatChannel::Combat,
                (Span::Danger, format!("{user} снимает {item}!")),
                &[(mob, Span::Notice, format!("Вы успешно снимаете {item}."))],
            );
        }
        RestraintJob::Snap { cuffs } => {
            if let Some(cutter) = action.item {
                snap(world, mob, cutter, target, cuffs);
            }
        }
        RestraintJob::Break { cuffs } => break_restraint(world, mob, cuffs, action.user_pos),
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
    shatter_on_uncuff(world, mob, cuffs, pos);
}

/// `cult/on_uncuffed` and `energy/on_uncuffed`: the cuffs are gone in a discharge.
fn shatter_on_uncuff(world: &mut World, wearer: Entity, cuffs: Entity, pos: TilePos) {
    let Some(path) = world.get::<DmHandcuffs>(cuffs).map(|c| c.path.clone()) else {
        return;
    };
    let (who, item) = (name_of(world, wearer), cuffs_name(world, cuffs));
    let (others, own) = if is_under(&path, CULT_CUFFS) {
        (
            format!("Оковы {who} рассыпаются в разряде тёмной магии!"),
            format!("Ваши {item} рассыпаются в разряде тёмной магии!"),
        )
    } else if is_under(&path, ENERGY_CUFFS) {
        crate::effects::particles::do_sparks(world, 4, false, pos);
        (
            format!("{item} {who} разрушается в разряде энергии!"),
            format!("{item} {who} разрушается в разряде энергии!"),
        )
    } else {
        return;
    };
    world.despawn(cuffs);
    show(
        world,
        pos,
        DEFAULT_MESSAGE_RANGE,
        ChatChannel::Combat,
        (Span::Danger, others),
        &[(wearer, Span::BoldWarning, own)],
    );
}

/// `clear_cuffs(cuffs, cuff_break)`: broken hand or leg cuffs are gone.
fn break_restraint(world: &mut World, mob: Entity, cuffs: Entity, pos: TilePos) {
    let item = cuffs_name(world, cuffs);
    if world.get::<Handcuffed>(mob) == Some(&Handcuffed(cuffs)) {
        world.entity_mut(mob).remove::<Handcuffed>();
        if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
            inventory.set_changed();
        }
    } else if world.get::<crate::legcuffs::Legcuffed>(mob)
        == Some(&crate::legcuffs::Legcuffed(cuffs))
    {
        crate::legcuffs::remove(world, mob, pos);
    } else {
        return;
    }
    if world.get_entity(cuffs).is_ok() {
        world.despawn(cuffs);
    }
    let user = name_of(world, mob);
    show(
        world,
        pos,
        DEFAULT_MESSAGE_RANGE,
        ChatChannel::Combat,
        (Span::Danger, format!("{user} ломает {item}!")),
        &[(mob, Span::Notice, format!("Вы успешно ломаете {item}."))],
    );
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
    // `get_all_attached_restraints`: the cuffs first, then a restraining suit.
    let (cuffs, breakout, job) =
        if let Some(Handcuffed(cuffs)) = world.get::<Handcuffed>(mob).copied() {
            let breakout = world
                .get::<DmHandcuffs>(cuffs)
                .map_or(600.0, |c| c.breakouttime);
            (cuffs, breakout, RestraintJob::Resist { cuffs })
        } else if let Some(crate::legcuffs::Legcuffed(cuffs)) =
            world.get::<crate::legcuffs::Legcuffed>(mob).copied()
        {
            let breakout = world
                .get::<ss13_mapload::DmLegcuffs>(cuffs)
                .map_or(300.0, |c| c.breakouttime);
            (cuffs, breakout, RestraintJob::ResistLegs { cuffs })
        } else if let Some(SuitRestrained(suit)) = world.get::<SuitRestrained>(mob).copied() {
            (
                suit,
                STRAIGHT_JACKET_BREAKOUT,
                RestraintJob::ResistSuit { suit },
            )
        } else {
            return;
        };
    let item = cuffs_name(world, cuffs);
    // `cuff_break`: an adult alien (`INSTANT_CUFFBREAK`), a Hulk (`FAST_CUFFBREAK`); not out of
    // a straight jacket.
    let breakable = !matches!(job, RestraintJob::ResistSuit { .. });
    let alien = world
        .get::<crate::antag::xeno::Xeno>(mob)
        .is_some_and(|x| x.caste != crate::antag::xeno::Caste::Larva);
    if breakable && alien {
        if let Some(&pos) = world.get::<TilePos>(mob) {
            break_restraint(world, mob, cuffs, pos);
        }
        return;
    }
    let hulk = breakable && crate::genetics::effects::is_hulk(world, mob);
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
    let actor = Actor {
        client,
        mob,
        item: None,
    };
    if hulk {
        let roar = {
            let mut rng = world.resource_mut::<crate::combat::CombatRng>();
            HULK_ROARS[rng.range(0, HULK_ROARS.len() as u32 - 1) as usize]
        };
        crate::spells::say_invocation(world, mob, crate::spells::Invocation::Shout, roar);
        let job = crate::construction::Job::Restraints(RestraintJob::Break { cuffs });
        if !do_after(world, actor, mob, FAST_CUFFBREAK_DS, 0, job) {
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
            (Span::Warning, format!("{user} пытается сломать {item}!")),
            &[(
                mob,
                Span::Notice,
                format!(
                    "Вы пытаетесь сломать {item}... (Это займет примерно 5 секунд, и вы не должны двигаться.)"
                ),
            )],
        );
        return;
    }
    let job = crate::construction::Job::Restraints(job);
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
