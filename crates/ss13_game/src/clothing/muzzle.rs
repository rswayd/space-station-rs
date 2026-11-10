//! Muzzles and tape over the mouth (server), after tg `code/modules/clothing/masks/muzzle.dm`
//! and `/datum/element/muffles_speech` (`code/datums/elements/muffles_speech.dm`).
//!
//! - A muzzle, the breath muzzle or a tape piece worn in the mask slot muffles its wearer
//!   ([`ss13_chat::speech::Muffled`]: words become "mmmf", audible emotes "a noise"); muzzles and
//!   tape cover the mouth (`MASKCOVERSMOUTH`: no eating or drinking, `crate::food`).
//! - A tape piece used on someone (combat mode off) tapes their mouth after its
//!   `equip_delay_other` (surgical 3 s, others 4 s), unless headgear covers the mouth or the mask
//!   slot is taken.
//! - Tape leaving the mouth by any path rips off with a sound; pointy tape hurts the head (10,
//!   the super one 20) and makes them scream.
// shortcut: taking a muzzle off someone uses the strip menu's default delay (per-item
// `strip_delay` is not converted); no monkey `attack_paw` refusal, no spell invocation block (no
// invocation spells), no greyscale tape colours.

use bevy::prelude::*;
use ss13_chat::speech::Muffled;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_shared::chat::Span;
use ss13_shared::protocol::InventorySlot;

use super::cover::HEADCOVERSMOUTH;
use crate::combat::CombatMode;
use crate::health::{DamageType, Health, Stat};
use crate::inventory::{CarriedBy, Inventory, can_equip};
use crate::security::name_of;
use crate::server::PlayerMob;
use crate::sound::{PlaySound, playsound};
use crate::tools::{Actor, DoAfter, do_after, tell};
use crate::zone::BodyZone;

pub const MUZZLE: &str = "/obj/item/clothing/mask/muzzle";
const BREATH_MUZZLE: &str = "/obj/item/clothing/mask/breath/muzzle";
pub const TAPE: &str = "/obj/item/clothing/mask/muzzle/tape";
const SURGICAL_TAPE: &str = "/obj/item/clothing/mask/muzzle/tape/surgical";
const POINTY_TAPE: &str = "/obj/item/clothing/mask/muzzle/tape/pointy";
const SUPER_POINTY_TAPE: &str = "/obj/item/clothing/mask/muzzle/tape/pointy/super";
const TAPE_RIP_SOUND: &str = "sound/items/duct_tape/duct_tape_rip.ogg";

/// On a tape piece over `0`'s mouth: tg `dropped` checks the mask slot it leaves.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct TapedOn(pub Entity);

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // Before the cuffs (`security::item_interaction`): tape onto someone's mouth.
    app.add_click_handler(ClickPhase::HeldOnTarget, 4285, |w, c| {
        c.held
            .is_some_and(|held| attack(w, c.client, c.mob, held, c.target))
    })
    .add_systems(
        Update,
        sync_muzzles.after(crate::inventory::inventory_intents),
    );
}

fn path(world: &World, item: Entity) -> Option<&str> {
    world.get::<DmItem>(item).map(|d| d.path.as_str())
}

/// `AddElement(/datum/element/muffles_speech)`.
pub fn muffles(path: &str) -> bool {
    is_under(path, MUZZLE) || is_under(path, BREATH_MUZZLE)
}

/// `stripping_damage` of a tape piece with `harmful_strip`.
pub fn stripping_damage(path: &str) -> f32 {
    if is_under(path, SUPER_POINTY_TAPE) {
        20.0
    } else if is_under(path, POINTY_TAPE) {
        10.0
    } else {
        0.0
    }
}

/// `equip_delay_other` of a tape piece, deciseconds.
fn tape_delay(path: &str) -> f32 {
    if is_under(path, SURGICAL_TAPE) {
        30.0
    } else {
        40.0
    }
}

/// tg `tape/attack` on `victim`: the taping starts. False when it is not tape, the user is in
/// combat mode or the victim has no mouth slot (the click goes on).
pub fn attack(
    world: &mut World,
    client: Entity,
    mob: Entity,
    tape: Entity,
    victim: Entity,
) -> bool {
    let Some(p) = path(world, tape).map(str::to_owned) else {
        return false;
    };
    if !is_under(&p, TAPE)
        || world.get::<CombatMode>(mob).is_some_and(|c| c.0)
        || world.get::<crate::human::HumanLook>(victim).is_none()
    {
        return false;
    }
    let Some(inventory) = world.get::<Inventory>(victim).cloned() else {
        return false;
    };
    let whom = name_of(world, victim);
    // `is_mouth_covered(ITEM_SLOT_HEAD)`.
    let head_covers = inventory.head.is_some_and(|h| {
        world
            .get::<ss13_mapload::DmClothing>(h)
            .is_some_and(|c| c.flags_cover & HEADCOVERSMOUTH != 0)
    });
    if head_covers {
        tell(world, client, Span::Notice, format!("Рот {whom} закрыт."));
        return true;
    }
    if inventory.mask.is_some() || !can_equip(world, victim, &inventory, tape, InventorySlot::Mask)
    {
        tell(
            world,
            client,
            Span::Notice,
            format!("На лице {whom} уже что-то надето."),
        );
        return true;
    }
    let actor = Actor {
        client,
        mob,
        item: Some(tape),
    };
    let job = crate::construction::Job::Clothing(super::damage::ClothingJob::TapeMouth { tape });
    if !do_after(world, actor, victim, tape_delay(&p), 0, job) {
        return true;
    }
    tell(world, client, Span::Notice, "заклеивание рта...");
    let who = name_of(world, mob);
    if let Some(c) = world.get::<PlayerMob>(victim).map(|p| p.client) {
        tell(
            world,
            c,
            Span::BoldWarning,
            format!("{who} пытается заклеить вам рот!"),
        );
    }
    true
}

/// The end of the taping `do_after` (`crate::tools` checked that neither moved and the tape
/// is still in the user's hand): `equip_to_slot_if_possible(src, ITEM_SLOT_MASK)`.
pub fn complete_tape(world: &mut World, mob: Entity, action: &DoAfter, tape: Entity) {
    let victim = action.target;
    let held = world
        .get::<Inventory>(mob)
        .is_some_and(|i| i.active_item() == Some(tape));
    let Some(inventory) = world.get::<Inventory>(victim).cloned() else {
        return;
    };
    if !held
        || world.get::<Stat>(mob) != Some(&Stat::Conscious)
        || inventory.mask.is_some()
        || !can_equip(world, victim, &inventory, tape, InventorySlot::Mask)
    {
        return;
    }
    world.entity_mut(tape).remove::<CarriedBy>();
    world.entity_mut(tape).insert(CarriedBy(victim));
    if let Some(mut inventory) = world.get_mut::<Inventory>(victim) {
        inventory.mask = Some(tape);
    }
}

/// Keeps [`Muffled`] on mobs with a muzzle on and rips tape that left the mouth (tg
/// `muffles_speech/equipped`, `dropped`, `tape/dropped`).
fn sync_muzzles(world: &mut World) {
    let mut changed = world.query_filtered::<(Entity, &Inventory), Changed<Inventory>>();
    let mobs: Vec<(Entity, Option<Entity>)> = changed
        .iter(world)
        .map(|(mob, inventory)| (mob, inventory.mask))
        .collect();
    if mobs.is_empty() {
        return;
    }
    for &(mob, mask) in &mobs {
        let muffled = mask.and_then(|m| path(world, m)).is_some_and(muffles);
        let has = world.get::<Muffled>(mob).is_some();
        if muffled && !has {
            world.entity_mut(mob).insert(Muffled);
        } else if !muffled && has {
            world.entity_mut(mob).remove::<Muffled>();
        }
        if let Some(m) = mask
            && path(world, m).is_some_and(|p| is_under(p, TAPE))
            && world.get::<TapedOn>(m) != Some(&TapedOn(mob))
        {
            world.entity_mut(m).insert(TapedOn(mob));
        }
    }
    let mut taped = world.query::<(Entity, &TapedOn)>();
    let off: Vec<(Entity, Entity)> = taped
        .iter(world)
        .filter(|(tape, on)| {
            world
                .get::<Inventory>(on.0)
                .is_none_or(|i| i.mask != Some(*tape))
        })
        .map(|(tape, on)| (tape, on.0))
        .collect();
    for (tape, mob) in off {
        world.entity_mut(tape).remove::<TapedOn>();
        rip(world, mob, tape);
    }
}

/// tg `tape/dropped` from the mask slot: the rip, and pointy tape's damage and scream.
fn rip(world: &mut World, mob: Entity, tape: Entity) {
    if world.get_entity(mob).is_err() {
        return;
    }
    if let Some(pos) = crate::explosions::turf_of(world, mob) {
        playsound(world, PlaySound::new(pos, TAPE_RIP_SOUND, 50.0, true));
    }
    let damage = path(world, tape).map_or(0.0, stripping_damage);
    if damage <= 0.0 {
        return;
    }
    if let Ok(mut entity) = world.get_entity_mut(mob)
        && let Ok((mut health, mut stat)) = entity.get_components_mut::<(&mut Health, &mut Stat)>()
    {
        crate::health::apply_damage_zone(
            &mut health,
            &mut stat,
            BodyZone::Head,
            DamageType::Brute,
            damage,
        );
    }
    crate::medical::diseases::emote(world, mob, "scream");
    crate::tools::tell_mob(
        world,
        mob,
        Span::BoldWarning,
        "Вы чувствуете сильную боль, когда сотни крошечных шипов отрываются от вашего лица!",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn muzzles_muffle_and_pointy_tape_hurts() {
        assert!(muffles("/obj/item/clothing/mask/muzzle/tape/super"));
        assert!(muffles(BREATH_MUZZLE));
        assert!(!muffles("/obj/item/clothing/mask/breath"));
        assert_eq!(stripping_damage(TAPE), 0.0);
        assert_eq!(stripping_damage(POINTY_TAPE), 10.0);
        assert_eq!(stripping_damage(SUPER_POINTY_TAPE), 20.0);
        assert_eq!(tape_delay(SURGICAL_TAPE), 30.0);
        assert_eq!(
            tape_delay("/obj/item/clothing/mask/muzzle/tape/super"),
            40.0
        );
    }
}
