//! Limbs as items (server), after tg `code/modules/surgery/bodyparts` (`dismemberment.dm`
//! `can_dismember`, `dismember`, `drop_limb`, `try_attach_limb`, `regenerate_limbs`;
//! `_bodyparts.dm` `drop_organs`; `stumps.dm`; `robot_bodyparts.dm`), `/datum/wound/loss` and the
//! carbon's `spill_organs`/`spread_bodyparts` of a gib (`carbon/death.dm`).
//!
//! An amputated or dismembered arm, leg or head leaves the body as its `/obj/item/bodypart` item,
//! carrying its brute and burn ([`Limb`]); a stump stays ([`super::wounds::Limb::missing`], with
//! no skin to cut: `SURGERY_SKIN_OPEN` is innate). Damage aimed at a missing part hits the chest,
//! a missing hand drops what it held and holds nothing (the cuffs fall off with an arm, the
//! gloves with the last hand), a missing leg counts as a disabled one and drops the shoes (no
//! legs floor the mob, `crate::medical::LimbLimits`), and the body is drawn without it.
//!
//! The head takes its organs (brain, eyes, ears, tongue) with it ([`InLimb`]): losing the brain
//! kills (`ORGAN_VITAL`); the worn head items fall. A head or limb item attached to its stump by
//! surgery brings its damage and organs back. Dismemberment ([`dismember`]) also hurts the chest
//! by half the limb's damage (15 to 50), bleeds 20-40 blood and throws the limb 1-2 tiles away; a
//! burning blow turns it to nothing. Heads and chests come off only in hard crit (BandaStation's
//! `can_dismember = TRUE` heads, `modular_bandastation/balance/code/bodyparts/head.dm`); a
//! "dismembered" chest spills its organs instead. A robotic prosthetic (`.../robot` types) is
//! drawn from `augments.dmi` and needs no anti-rejection toxin.
// shortcut: only human limb types are spawned (no species variants, wooden or surplus limbs, no
// arbitrary items as prosthetics); a missing hand still holds what other code puts there
// directly (only pickups check it); a missing head does not lock the head slots; no blood spray,
// mood events, memories, scars, phantom pain or limb-requiring mutations; the head item keeps
// the old body's name only in its item name (not its look or `real_name`).

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_shared::chat::Span;
use ss13_shared::protocol::InventorySlot;
use ss13_shared::{Dir, TilePos};

use super::organs::{self, Organ, Organs};
use super::wounds::{Limbs, WoundType, state};
use crate::combat::CombatRng;
use crate::health::{DamageType, Health, Stat};
use crate::inventory::Inventory;
use crate::obj_damage::is_under;
use crate::species::{Trait, has_trait};
use crate::zone::BodyZone;

/// `sound/effects/dismember.ogg`, `sound/misc/splort.ogg`.
const DISMEMBER_SOUND: &str = "sound/effects/dismember.ogg";
const SPLORT_SOUND: &str = "sound/misc/splort.ogg";

/// The damage a removed limb keeps (tg `brute_dam`, `burn_dam` of the item).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Limb {
    pub brute: f32,
    pub burn: f32,
}

/// On an organ inside a removed head (tg contents of `/obj/item/bodypart/head`): not on the map,
/// not replicated; despawning the head despawns it.
#[derive(Component, Debug)]
#[relationship(relationship_target = LimbOrgans)]
pub struct InLimb(pub Entity);

/// The organs a removed bodypart holds.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = InLimb, linked_spawn)]
pub struct LimbOrgans(Vec<Entity>);

impl LimbOrgans {
    pub fn iter(&self) -> impl Iterator<Item = Entity> + '_ {
        self.0.iter().copied()
    }
}

/// The human limb type of a zone (tg `/datum/species/human` `bodypart_overrides`).
pub fn limb_path(zone: BodyZone) -> Option<&'static str> {
    Some(match zone {
        BodyZone::Head => "/obj/item/bodypart/head",
        BodyZone::LArm => "/obj/item/bodypart/arm/left",
        BodyZone::RArm => "/obj/item/bodypart/arm/right",
        BodyZone::LLeg => "/obj/item/bodypart/leg/left",
        BodyZone::RLeg => "/obj/item/bodypart/leg/right",
        BodyZone::Chest => return None,
    })
}

/// The `body_zone` of a limb item type (any species' or robotic variant).
pub fn zone_of_path(path: &str) -> Option<BodyZone> {
    BodyZone::ALL
        .into_iter()
        .find(|&z| limb_path(z).is_some_and(|root| is_under(path, root)))
}

/// A cyborg/prosthetic limb type (`/obj/item/bodypart/*/robot`, `bodytype = BODYTYPE_ROBOTIC`).
pub fn is_robotic_path(path: &str) -> bool {
    zone_of_path(path)
        .and_then(limb_path)
        .is_some_and(|root| is_under(path, &format!("{root}/robot")))
}

/// The hand a missing arm takes away.
fn hand_of(zone: BodyZone) -> Option<usize> {
    match zone {
        BodyZone::LArm => Some(0),
        BodyZone::RArm => Some(1),
        _ => None,
    }
}

/// The hand at `hand` is there to hold things (not amputated or paralyzed).
pub fn has_hand(world: &World, mob: Entity, hand: usize) -> bool {
    let zone = if hand == 0 {
        BodyZone::LArm
    } else {
        BodyZone::RArm
    };
    world
        .get::<Limbs>(mob)
        .is_none_or(|l| !l.limb(zone).missing && !l.limb(zone).paralyzed)
}

/// tg `can_dismember` of `mob`'s bodypart in `zone`: attached, not `TRAIT_NODISMEMBER`; a head
/// or chest only in hard crit (the chest also needs organs to spill).
pub fn can_dismember(world: &World, mob: Entity, zone: BodyZone) -> bool {
    let Some(limbs) = world.get::<Limbs>(mob) else {
        return false;
    };
    if limbs.limb(zone).missing || has_trait(world, mob, Trait::NoDismember) {
        return false;
    }
    let hard_crit = world.get::<Stat>(mob).is_some_and(|&s| s >= Stat::HardCrit);
    match zone {
        BodyZone::Head => hard_crit,
        BodyZone::Chest => hard_crit && !zone_organs(world, mob, BodyZone::Chest).is_empty(),
        _ => true,
    }
}

/// `mob`'s organs that sit in `zone`'s bodypart.
fn zone_organs(world: &World, mob: Entity, zone: BodyZone) -> Vec<Entity> {
    world
        .get::<Organs>(mob)
        .map(|o| {
            o.iter()
                .filter(|&e| {
                    world
                        .get::<Organ>(e)
                        .is_some_and(|o| o.slot.bodypart() == zone)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// tg `/datum/wound/loss/apply_dismember` and `get_dismember_message`: the message, then
/// [`dismember`] (a burn turns the limb to ash).
pub fn apply_dismember(
    world: &mut World,
    mob: Entity,
    zone: BodyZone,
    wounding: WoundType,
    outright: bool,
) -> bool {
    if !can_dismember(world, mob, zone) {
        return false;
    }
    let (others, own) = if zone == BodyZone::Chest {
        (
            "рассекается, вызывая выпадение внутренних органов!",
            "рассекается, вызывая выпадение ваших внутренних органов!",
        )
    } else {
        let text = match (wounding, outright) {
            (WoundType::Burn, _) => "сгорает, превратившись в пыль!",
            (WoundType::Blunt, true) => "разбивается в отвратительное месиво, отрываясь!",
            (WoundType::Slash, true) => "чисто срезается, полностью отсекаясь!",
            (WoundType::Pierce, true) => "разрывается, полностью отсекаясь!",
            (WoundType::Blunt, false) => {
                "теряет от удара последнюю кость, что удерживала конечность!"
            }
            (WoundType::Slash, false) => {
                "теряет от разреза последнюю ткань, что удерживала конечность!"
            }
            (WoundType::Pierce, false) => {
                "теряет от прокалывания последнюю ткань, что удерживала конечность!"
            }
        };
        (text, text)
    };
    let name = super::name_of(world, mob);
    let part = zone.ru().0;
    super::visible(
        world,
        mob,
        &format!("Ваша {part} {own}"),
        &format!("{} у {name} {others}", capitalize(part)),
        Span::Danger,
    );
    dismember(world, mob, zone, wounding == WoundType::Burn)
}

/// tg `dismember(dam_type, silent = TRUE)`: the chest takes half the limb's damage (15 to 50
/// brute), the victim screams, the limb drops and flies 1-2 tiles off, the stump bleeds 20-40;
/// `burn` destroys the limb instead. The chest spills its organs (`chest/dismember`).
pub fn dismember(world: &mut World, mob: Entity, zone: BodyZone, burn: bool) -> bool {
    dismember_limb(world, mob, zone, burn).is_some()
}

/// [`dismember`], with the limb item that came off (`None` for the chest, a burnt limb or a
/// limb type without its prototype).
pub fn dismember_limb(
    world: &mut World,
    mob: Entity,
    zone: BodyZone,
    burn: bool,
) -> Option<Option<Entity>> {
    if !can_dismember(world, mob, zone) {
        return None;
    }
    let &pos = world.get::<TilePos>(mob)?;
    if zone == BodyZone::Chest {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, SPLORT_SOUND, 80.0, true),
        );
        for organ in zone_organs(world, mob, BodyZone::Chest) {
            organs::remove(world, organ, false);
            crate::storage::take_out(world, organ, pos);
        }
        return Some(None);
    }
    if let Some(part) = world.get::<Health>(mob).map(|h| h.part(zone)) {
        let brute = (part.brute / 2.0).clamp(15.0, 50.0);
        let burn_dam = (part.burn / 2.0).clamp(0.0, 50.0);
        super::hurt_limb(world, mob, BodyZone::Chest, DamageType::Brute, brute);
        super::hurt_limb(world, mob, BodyZone::Chest, DamageType::Burn, burn_dam);
    }
    super::diseases::emote(world, mob, "scream");
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, DISMEMBER_SOUND, 80.0, true),
    );
    let robotic = world
        .get::<Limbs>(mob)
        .is_some_and(|l| l.limb(zone).robotic);
    let Some(item) = drop_limb(world, mob, zone) else {
        return Some(None);
    };
    if burn {
        world.entity_mut(item).despawn();
        return Some(None);
    }
    let mut rng = super::fork_rng(world);
    if !robotic {
        super::blood::bleed(world, mob, rng.range(20, 40) as f32);
    }
    // `throw_at` a cardinal `rand(2, max(throw_range / 2, 2)) - 1` steps off (throw_range 7).
    let dir = [Dir::North, Dir::South, Dir::East, Dir::West][rng.range(0, 3) as usize];
    let mut target = pos;
    for _ in 1..rng.range(2, 3) {
        target = target.step(dir);
    }
    crate::throw::throw_at(world, item, target, 7, 2.0);
    Some(Some(item))
}

/// tg `drop_limb()`: `zone` leaves `mob` as an item on its tile with the part's damage (the head
/// with its organs). `None` for the chest, a missing part or a mob without limbs; the part is
/// gone even when its item type has no prototype (its organs then fall to the floor).
pub fn drop_limb(world: &mut World, mob: Entity, zone: BodyZone) -> Option<Entity> {
    let root = limb_path(zone)?;
    let pos = *world.get::<TilePos>(mob)?;
    let mut limbs = world.get::<Limbs>(mob)?.clone();
    if limbs.limb(zone).missing {
        return None;
    }
    let limb = limbs.limb_mut(zone);
    while !limb.wounds.is_empty() {
        super::wounds::remove_wound(limb, 0);
    }
    let robotic = std::mem::take(&mut limb.robotic);
    limb.gauze = None;
    limb.missing = true;
    limb.surgery = state::SKIN_OPEN;
    world.entity_mut(mob).insert(limbs);
    let mut damage = Limb::default();
    if let Some(mut health) = world.get::<Health>(mob).cloned() {
        let part = health.part(zone);
        damage = Limb {
            brute: part.brute,
            burn: part.burn,
        };
        health.heal_zone(zone, part.brute, part.burn);
        health.missing |= 1 << zone.index();
        world.entity_mut(mob).insert(health);
    }
    let fallen: &[InventorySlot] = match zone {
        // `head/drop_limb`: `ITEM_SLOT_EYES | ITEM_SLOT_EARS | ITEM_SLOT_MASK | ITEM_SLOT_HEAD`.
        BodyZone::Head => &[
            InventorySlot::Eyes,
            InventorySlot::Ears,
            InventorySlot::Mask,
            InventorySlot::Head,
        ],
        // `leg/drop_limb`: the shoes.
        BodyZone::LLeg | BodyZone::RLeg => &[InventorySlot::Shoes],
        _ => &[],
    };
    drop_slots(world, mob, pos, fallen);
    if let Some(hand) = hand_of(zone) {
        if let Some(held) = world.get::<Inventory>(mob).and_then(|i| i.hands[hand]) {
            crate::inventory::put_down(world, held, pos);
        }
        // `arm/drop_limb`: the cuffs fall off; with no hand left, the gloves.
        crate::restraints::uncuff(world, mob, pos);
        let handless = world
            .get::<Limbs>(mob)
            .is_some_and(|l| l.limb(BodyZone::LArm).missing && l.limb(BodyZone::RArm).missing);
        if handless {
            drop_slots(world, mob, pos, &[InventorySlot::Gloves]);
        }
    }
    let path = if robotic {
        format!("{root}/robot")
    } else {
        root.to_owned()
    };
    let item = crate::construction::spawn_obj(world, &path, pos);
    let held_organs = if zone == BodyZone::Head {
        zone_organs(world, mob, zone)
    } else {
        Vec::new()
    };
    for organ in held_organs {
        organs::remove(world, organ, false);
        match item {
            Some(head) => {
                world.entity_mut(organ).insert(InLimb(head));
            }
            None => crate::storage::take_out(world, organ, pos),
        }
    }
    set_drawn(world, mob, zone, false, false);
    let item = item?;
    world.entity_mut(item).insert(damage);
    if zone == BodyZone::Head {
        // `ru_names_rename(... suffix = " [owner.real_name]")`.
        let name = format!(
            "{} {}",
            super::name_of(world, item),
            super::name_of(world, mob)
        );
        world.entity_mut(item).insert(Name::new(name));
    }
    Some(item)
}

/// Worn items of `slots` fall to `pos` (tg `dropItemToGround(force = TRUE)`).
fn drop_slots(world: &mut World, mob: Entity, pos: TilePos, slots: &[InventorySlot]) {
    for &slot in slots {
        if let Some(item) = world.get::<Inventory>(mob).and_then(|i| i.slot(slot)) {
            crate::inventory::unequip(world, mob, pos, slot);
            crate::inventory::put_down(world, item, pos);
        }
    }
}

/// tg `try_attach_limb()`: the limb item onto `mob`'s stump of its zone, its damage and organs
/// with it. False if the zone is not a stump.
pub fn attach_limb(world: &mut World, mob: Entity, item: Entity) -> bool {
    let Some(path) = world.get::<DmItem>(item).map(|i| i.path.clone()) else {
        return false;
    };
    let Some(zone) = zone_of_path(&path) else {
        return false;
    };
    let Some(mut limbs) = world.get::<Limbs>(mob).cloned() else {
        return false;
    };
    if !limbs.limb(zone).missing {
        return false;
    }
    let robotic = is_robotic_path(&path);
    let limb = limbs.limb_mut(zone);
    limb.missing = false;
    limb.surgery = 0;
    limb.robotic = robotic;
    world.entity_mut(mob).insert(limbs);
    let damage = world.get::<Limb>(item).copied().unwrap_or_default();
    if let Some(mut health) = world.get::<Health>(mob).cloned() {
        health.missing &= !(1 << zone.index());
        health.damage_zone(zone, DamageType::Brute, damage.brute);
        health.damage_zone(zone, DamageType::Burn, damage.burn);
        world.entity_mut(mob).insert(health);
    }
    let held: Vec<Entity> = world
        .get::<LimbOrgans>(item)
        .map(|o| o.iter().collect())
        .unwrap_or_default();
    let pos = world.get::<TilePos>(mob).copied();
    for organ in held {
        world.entity_mut(organ).remove::<InLimb>();
        // A slot the body already filled meanwhile: the old organ drops (`insert`).
        if !organs::insert(world, organ, mob)
            && let Some(pos) = pos
        {
            crate::storage::take_out(world, organ, pos);
        }
    }
    set_drawn(world, mob, zone, true, robotic);
    world.entity_mut(item).despawn();
    true
}

/// tg `drop_organs()` of a removed bodypart: its organs fall to `pos`. The number dropped.
pub fn drop_organs(world: &mut World, item: Entity, pos: TilePos) -> usize {
    let held: Vec<Entity> = world
        .get::<LimbOrgans>(item)
        .map(|o| o.iter().collect())
        .unwrap_or_default();
    for &organ in &held {
        world.entity_mut(organ).remove::<InLimb>();
        crate::storage::take_out(world, organ, pos);
    }
    if !held.is_empty() {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, SPLORT_SOUND, 50.0, true),
        );
    }
    held.len()
}

/// A sharp item used on a removed head out of combat mode empties it of its organs.
// shortcut: tg runs the loose bodypart's organ manipulation surgery
// (`OPERATION_NO_PATIENT_REQUIRED`); here any sharp item spills every organ at once.
pub fn item_interaction(world: &mut World, mob: Entity, held: Entity, target: Entity) -> bool {
    if world.get::<LimbOrgans>(target).is_none()
        || world
            .get::<crate::combat::CombatMode>(mob)
            .is_some_and(|c| c.0)
        || world
            .get::<ss13_mapload::DmWounding>(held)
            .is_none_or(|w| w.sharpness == 0)
    {
        return false;
    }
    let Some(pos) = world
        .get::<TilePos>(target)
        .or_else(|| world.get::<TilePos>(mob))
        .copied()
    else {
        return false;
    };
    let limb = super::name_of(world, target);
    if drop_organs(world, target, pos) > 0 {
        let user = super::name_of(world, mob);
        super::visible(
            world,
            mob,
            &format!("Вы вскрываете {limb} и вынимаете органы."),
            &format!("{user} вскрывает {limb} и вынимает органы!"),
            Span::Warning,
        );
    }
    true
}

/// tg carbon `spill_organs(drop_bitflags)` and `spread_bodyparts` of a gib: the dropped organs
/// (only the chest's when limbs drop: the head carries its own) and limbs fly 1-3 tiles off; any
/// other organ goes with the body.
pub fn spill_remains(world: &mut World, mob: Entity, pos: TilePos, flags: u8) {
    use crate::health::{DROP_BODYPARTS, DROP_BRAIN, DROP_ORGANS};
    if world.get::<Limbs>(mob).is_none() {
        return;
    }
    let mut rng = super::fork_rng(world);
    let mut thrown = Vec::new();
    let drop_limbs = flags & DROP_BODYPARTS != 0;
    let brain = organs::organ(world, mob, organs::Slot::Brain);
    for zone in BodyZone::ALL {
        for organ in zone_organs(world, mob, zone) {
            let wanted = if Some(organ) == brain {
                DROP_BRAIN
            } else {
                DROP_ORGANS
            };
            if flags & wanted != 0 {
                if !drop_limbs || zone == BodyZone::Chest {
                    organs::remove(world, organ, true);
                    crate::storage::take_out(world, organ, pos);
                    thrown.push(organ);
                }
            } else {
                organs::remove(world, organ, true);
                if let Ok(organ) = world.get_entity_mut(organ) {
                    organ.despawn();
                }
            }
        }
    }
    if drop_limbs {
        for zone in [
            BodyZone::Head,
            BodyZone::LArm,
            BodyZone::RArm,
            BodyZone::LLeg,
            BodyZone::RLeg,
        ] {
            // `spread_bodyparts`: the head stays unless the brain is wanted.
            if zone == BodyZone::Head && flags & DROP_BRAIN == 0 {
                continue;
            }
            thrown.extend(drop_limb(world, mob, zone));
        }
    }
    for item in thrown {
        throw_away(world, &mut rng, item, pos);
    }
}

/// `throw_at(get_edge_target_turf(src, pick(GLOB.alldirs)), rand(1, 3), 5)`.
fn throw_away(world: &mut World, rng: &mut CombatRng, item: Entity, pos: TilePos) {
    const ALL_DIRS: [Dir; 8] = [
        Dir::North,
        Dir::South,
        Dir::East,
        Dir::West,
        Dir::NorthEast,
        Dir::NorthWest,
        Dir::SouthEast,
        Dir::SouthWest,
    ];
    let dir = ALL_DIRS[rng.range(0, 7) as usize];
    let range = rng.range(1, 3);
    let mut target = pos;
    for _ in 0..range {
        target = target.step(dir);
    }
    crate::throw::throw_at(world, item, target, range, 5.0);
}

/// tg `regenerate_limbs()` of `fully_heal(HEAL_LIMBS)`: every stump grows a fresh limb.
// shortcut: a regrown head has no organs until `organs::regenerate` (`HEAL_ORGANS`) refills it.
pub fn regenerate_limbs(world: &mut World, mob: Entity) {
    let Some(mut limbs) = world.get::<Limbs>(mob).cloned() else {
        return;
    };
    let grown: Vec<BodyZone> = BodyZone::ALL
        .into_iter()
        .filter(|&z| limbs.limb(z).missing)
        .collect();
    if grown.is_empty() {
        return;
    }
    for &zone in &grown {
        let limb = limbs.limb_mut(zone);
        limb.missing = false;
        limb.surgery = 0;
    }
    world.entity_mut(mob).insert(limbs);
    if let Some(mut health) = world.get_mut::<Health>(mob) {
        health.missing = 0;
    }
    for zone in grown {
        set_drawn(world, mob, zone, true, false);
    }
}

fn set_drawn(world: &mut World, mob: Entity, zone: BodyZone, drawn: bool, robotic: bool) {
    if let Some(mut look) = world.get_mut::<crate::human::HumanLook>(mob) {
        let bit = 1 << zone.index();
        let missing = if drawn {
            look.missing & !bit
        } else {
            look.missing | bit
        };
        let robotic = if robotic {
            look.robotic | bit
        } else {
            look.robotic & !bit
        };
        if look.missing != missing || look.robotic != robotic {
            look.missing = missing;
            look.robotic = robotic;
        }
    }
    crate::inventory::refresh_look(world, mob);
}

use ss13_chat::capitalize;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limb_types_map_to_zones() {
        assert_eq!(
            zone_of_path("/obj/item/bodypart/head"),
            Some(BodyZone::Head)
        );
        assert_eq!(
            zone_of_path("/obj/item/bodypart/arm/left/robot/surplus"),
            Some(BodyZone::LArm)
        );
        assert_eq!(zone_of_path("/obj/item/bodypart/chest"), None);
        assert_eq!(zone_of_path("/obj/item/bodypart/arm/leftover"), None);
        assert!(is_robotic_path("/obj/item/bodypart/leg/right/robot"));
        assert!(is_robotic_path("/obj/item/bodypart/head/robot"));
        assert!(!is_robotic_path("/obj/item/bodypart/leg/right"));
        assert!(!is_robotic_path("/obj/item/bodypart/leg/right/lizard"));
    }
}
