//! Clothing damage (server), after tg `/obj/item/clothing` `take_damage_zone`, `disable_zone`,
//! `atom_break`, `atom_destruction`, `repair` and `item_interaction` with cloth
//! (`code/modules/clothing/clothing.dm`).
//!
//! A slashing or burning hit on a bodypart that rolls for a wound (`check_woundings_mods` with
//! `wound_clothing`) damages every worn piece covering it that has `limb_integrity` and covers
//! more than one bodypart: a tenth of the wounding damage comes off the item's integrity, all of it
//! goes to the zone. A zone past `limb_integrity` is disabled (it stops covering that bodypart,
//! so armor and protection there are gone) and the item is renamed "damaged"/"mangy"/"tattered";
//! with nothing covered left, or its integrity at 0, it is shredded: it covers nothing, fits no
//! slot and falls off. Below `integrity_failure` it is damaged ("начинает распадаться"). Cloth
//! repairs it: one sheet at once when damaged, three over 6 s when shredded.
// shortcut: no armor of the item itself (tg `run_atom_armor` with no damage flag also skips it),
// no damage overlay on the item's own icon (only the worn uniform/mask overlay), no "bristle"
// reminder while moving, no moths eating clothes.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmClothing, DmItem, DmName};
use ss13_shared::chat::Span;

use crate::health::COMBAT_MESSAGE_RANGE;
use crate::inventory::{CarriedBy, Inventory};
use crate::medical::wounds::{CANT_WOUND, Hit, Limbs, WOUND_MINIMUM_DAMAGE, WoundType};
use crate::tools::{Actor, tell};
use crate::zone::BodyZone;

/// Body part bits (`code/__DEFINES/inventory.dm`).
pub const CHEST: u32 = 1 << 0;
pub const GROIN: u32 = 1 << 1;
pub const HEAD: u32 = 1 << 2;
pub const LEG_LEFT: u32 = 1 << 3;
pub const LEG_RIGHT: u32 = 1 << 4;
pub const FOOT_LEFT: u32 = 1 << 5;
pub const FOOT_RIGHT: u32 = 1 << 6;
pub const ARM_LEFT: u32 = 1 << 7;
pub const ARM_RIGHT: u32 = 1 << 8;
pub const HAND_LEFT: u32 = 1 << 9;
pub const HAND_RIGHT: u32 = 1 << 10;
/// `NO_ZONE_DISABLING` (`clothing_flags`).
pub const NO_ZONE_DISABLING: u32 = 1 << 14;
/// `/obj/item/clothing` `integrity_failure`.
const INTEGRITY_FAILURE: f32 = 0.4;
/// `DAMAGE_PRECISION`.
const DAMAGE_PRECISION: f32 = 0.1;
/// `repairable_by`.
pub const CLOTH: &str = "/obj/item/stack/sheet/cloth";
/// Deciseconds of a shredded item's repair, and the sheets it takes.
const REPAIR_DS: f32 = 60.0;
const SHREDDED_SHEETS: u32 = 3;

/// tg `damaged_clothes`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Wear {
    #[default]
    Pristine,
    Damaged,
    Shredded,
}

/// A piece of clothing that took damage: tg `atom_integrity`, `damage_by_parts`,
/// `zones_disabled`, `damaged_clothes`, and the `initial()` values `repair` restores.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct ClothingWear {
    pub integrity: f32,
    /// By [`BodyZone::index`].
    pub by_zone: [f32; 6],
    pub zones_disabled: u32,
    pub state: Wear,
    initial_covered: u32,
    initial_slot_flags: u32,
    initial_name: String,
}

/// A timed clothing step (`crate::construction::Job::Clothing`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClothingJob {
    /// Three cloth sheets on a shredded piece of clothing.
    Repair { item: Entity },
    /// The chameleon scanner's 0.5 s scan of `target` (`crate::clothing::chameleon`).
    ChameleonScan { target: Entity, equip: bool },
}

/// `body_zone2cover_flags`.
pub fn zone_cover(zone: BodyZone) -> u32 {
    match zone {
        BodyZone::Chest => CHEST | GROIN,
        BodyZone::Head => HEAD,
        BodyZone::LArm => ARM_LEFT | HAND_LEFT,
        BodyZone::RArm => ARM_RIGHT | HAND_RIGHT,
        BodyZone::LLeg => LEG_LEFT | FOOT_LEFT,
        BodyZone::RLeg => LEG_RIGHT | FOOT_RIGHT,
    }
}

/// `zone in cover_flags2body_zones(covered)`.
pub fn covers_zone(covered: u32, zone: BodyZone) -> bool {
    match zone {
        BodyZone::Chest => covered & CHEST != 0,
        zone => covered & zone_cover(zone) != 0,
    }
}

/// The zone-damage state of `item`, made on first use.
fn wear_of(world: &World, item: Entity) -> Option<ClothingWear> {
    if let Some(wear) = world.get::<ClothingWear>(item) {
        return Some(wear.clone());
    }
    let covered = world.get::<DmClothing>(item)?.body_parts_covered;
    let dm = world.get::<DmItem>(item)?;
    let wear = ClothingWear {
        integrity: dm.max_integrity,
        by_zone: [0.0; 6],
        zones_disabled: 0,
        state: Wear::Pristine,
        initial_covered: covered,
        initial_slot_flags: dm.slot_flags,
        initial_name: crate::security::name_of(world, item),
    };
    Some(wear)
}

fn set_name(world: &mut World, item: Entity, name: String) {
    if let Some(mut n) = world.get_mut::<DmName>(item) {
        n.name = name;
    }
}

/// The mob wearing (or holding) `item`.
fn wearer(world: &World, item: Entity) -> Option<Entity> {
    world.get::<CarriedBy>(item).map(|c| c.0)
}

/// tg `check_woundings_mods` with `wound_clothing`: the clothing on the hit bodypart takes zone
/// damage from a slashing or burning hit that rolls for a wound. Called before the wound roll.
pub fn after_hit(world: &mut World, victim: Entity, zone: BodyZone, hit: Hit) {
    let Some(limb) = world.get::<Limbs>(victim).map(|l| l.limb(zone).clone()) else {
        return;
    };
    if limb.robotic || limb.missing {
        return;
    }
    let Some((wounding, damage)) = crate::medical::wounds::wounding_of(hit, &limb) else {
        return;
    };
    if damage < WOUND_MINIMUM_DAMAGE || hit.wound_bonus == CANT_WOUND {
        return;
    }
    let burn = match wounding {
        WoundType::Slash => false,
        WoundType::Burn => true,
        _ => return,
    };
    // `check_wounding`: `damage = min(damage, WOUND_MAX_CONSIDERED_DAMAGE)`.
    let damage = damage.min(crate::medical::wounds::WOUND_MAX_CONSIDERED_DAMAGE);
    let Some(inventory) = world.get::<Inventory>(victim) else {
        return;
    };
    let bit = crate::chem::body_part_bit(zone);
    // `get_clothing_on_part`: everything worn covering the bodypart.
    let pieces: Vec<Entity> = inventory
        .equipped()
        .filter(|&i| {
            world
                .get::<DmClothing>(i)
                .is_some_and(|c| c.body_parts_covered & bit != 0)
        })
        .collect();
    for item in pieces {
        take_damage_zone(world, item, zone, damage, burn);
    }
}

/// tg `take_damage_zone`.
pub fn take_damage_zone(world: &mut World, item: Entity, zone: BodyZone, damage: f32, burn: bool) {
    let Some((limb_integrity, covered)) = world
        .get::<DmClothing>(item)
        .map(|c| (c.limb_integrity, c.body_parts_covered))
    else {
        return;
    };
    if limb_integrity <= 0.0 || !covers_zone(covered, zone) {
        return;
    }
    let Some(mut wear) = wear_of(world, item) else {
        return;
    };
    // `initial(body_parts_covered) in GLOB.bitflags`: one bodypart is not tracked.
    if wear.initial_covered.count_ones() <= 1 {
        return;
    }
    // `take_damage(damage * 0.1) * 10`.
    let dealt = integrity_damage(world, item, &mut wear, damage * 0.1, burn) * 10.0;
    if wear.state == Wear::Shredded {
        return;
    }
    wear.by_zone[zone.index()] += dealt;
    let disable = wear.by_zone[zone.index()] > limb_integrity;
    world.entity_mut(item).insert(wear);
    if disable {
        disable_zone(world, item, zone, burn);
    }
}

/// tg `take_damage` on the item's integrity: `atom_break` below `integrity_failure`,
/// `atom_destruction` at 0. The damage dealt.
fn integrity_damage(
    world: &mut World,
    item: Entity,
    wear: &mut ClothingWear,
    amount: f32,
    burn: bool,
) -> f32 {
    let indestructible = world
        .get::<DmItem>(item)
        .is_some_and(|i| i.resistance_flags & crate::obj_damage::INDESTRUCTIBLE != 0);
    let amount = (amount / DAMAGE_PRECISION).round() * DAMAGE_PRECISION;
    if indestructible || wear.integrity <= 0.0 || amount < DAMAGE_PRECISION {
        return 0.0;
    }
    let max = world.get::<DmItem>(item).map_or(200.0, |i| i.max_integrity);
    let failure = INTEGRITY_FAILURE * max;
    let before = wear.integrity;
    wear.integrity = (before - amount).max(0.0);
    if wear.integrity <= 0.0 {
        world.entity_mut(item).insert(wear.clone());
        shred(world, item, burn);
        wear.state = Wear::Shredded;
        return amount;
    }
    if before > failure && wear.integrity <= failure {
        wear.state = Wear::Damaged;
        if let Some(mob) = wearer(world, item)
            && let Some(client) = world.get::<crate::server::PlayerMob>(mob).map(|p| p.client)
        {
            let name = crate::forensics::capitalize(&crate::security::name_of(world, item));
            tell(
                world,
                client,
                Span::Warning,
                format!("{name} начинает распадаться на части!"),
            );
        }
    }
    amount
}

/// tg `disable_zone`.
fn disable_zone(world: &mut World, item: Entity, zone: BodyZone, burn: bool) {
    let Some(mut wear) = world.get::<ClothingWear>(item).cloned() else {
        return;
    };
    let Some(mut clothing) = world.get::<DmClothing>(item).cloned() else {
        return;
    };
    if !covers_zone(clothing.body_parts_covered, zone) {
        return;
    }
    if let Some(mob) = wearer(world, item) {
        let verb = if burn {
            "прогорает"
        } else {
            "рвётся"
        };
        let part = crate::forensics::capitalize(zone.ru().0);
        let name = crate::security::name_of(world, item);
        let who = crate::security::name_of(world, mob);
        crate::medical::visible_range(
            world,
            mob,
            &format!("{part} {verb} на вашем {name}!"),
            &format!("{part} {verb} на {name} у {who}!"),
            Span::Danger,
            COMBAT_MESSAGE_RANGE,
        );
    }
    wear.zones_disabled += 1;
    // tg tests `clothing_flags & NO_ZONE_DISABLING` the wrong way round: the flag is meant to keep
    // the coverage (plasmaman jumpsuits), so it is ported as meant.
    if clothing.clothing_flags & NO_ZONE_DISABLING == 0 {
        clothing.body_parts_covered &= !zone_cover(zone);
        world.entity_mut(item).insert(clothing.clone());
    }
    if clothing.body_parts_covered == 0 {
        world.entity_mut(item).insert(wear);
        shred(world, item, burn);
        return;
    }
    let prefix = match wear.zones_disabled {
        1 => "damaged",
        2 => "mangy",
        _ => "tattered",
    };
    let name = format!("{prefix} {}", wear.initial_name);
    wear.state = Wear::Damaged;
    world.entity_mut(item).insert(wear);
    set_name(world, item, name);
    refresh_wearer(world, item);
}

/// tg `atom_destruction` of clothing (not acid, fire or bombs): it covers nothing, fits no slot
/// and falls off its wearer.
fn shred(world: &mut World, item: Entity, _burn: bool) {
    let Some(mut wear) = world.get::<ClothingWear>(item).cloned() else {
        return;
    };
    wear.state = Wear::Shredded;
    let name = crate::security::name_of(world, item);
    world.entity_mut(item).insert(wear.clone());
    if let Some(mut c) = world.get_mut::<DmClothing>(item) {
        c.body_parts_covered = 0;
    }
    if let Some(mut i) = world.get_mut::<DmItem>(item) {
        i.slot_flags = 0;
    }
    if is_under(&path_of(world, item), "/obj/item/clothing/under") {
        super::sensors::break_sensors(world, item);
    }
    // Jumpsuits and (BandaStation) gloves drop what is pinned to them.
    super::accessories::dump(world, item);
    if let Some(mob) = wearer(world, item) {
        let worn = world
            .get::<Inventory>(mob)
            .is_some_and(|i| i.equipped().any(|e| e == item));
        let who = crate::security::name_of(world, mob);
        let cap = crate::forensics::capitalize(&name);
        if worn {
            crate::medical::visible_range(
                world,
                mob,
                &format!("{cap} спадает с вас, полностью развалившись на кусочки!"),
                &format!("{cap} спадает c {who}, полностью развалившись на кусочки!"),
                Span::Danger,
                COMBAT_MESSAGE_RANGE,
            );
            if let (Some(&pos), Some(slot)) = (
                world.get::<ss13_shared::TilePos>(mob),
                world.get::<Inventory>(mob).and_then(|i| i.slot_of(item)),
            ) {
                crate::inventory::unequip(world, mob, pos, slot);
                crate::inventory::put_down(world, item, pos);
            }
        } else {
            crate::medical::visible_range(
                world,
                mob,
                &format!("{cap} разваливается на кусочки!"),
                &format!("{cap} разваливается на кусочки!"),
                Span::Danger,
                COMBAT_MESSAGE_RANGE,
            );
        }
    }
    set_name(world, item, format!("shredded {}", wear.initial_name));
    refresh_wearer(world, item);
}

fn path_of(world: &World, item: Entity) -> String {
    world
        .get::<DmItem>(item)
        .map(|i| i.path.clone())
        .unwrap_or_default()
}

fn refresh_wearer(world: &mut World, item: Entity) {
    if let Some(mob) = wearer(world, item) {
        crate::inventory::refresh_look(world, mob);
    }
}

/// tg `repair`: integrity, coverage, slots and name back, zone damage gone.
pub fn repair(world: &mut World, client: Option<Entity>, item: Entity) {
    let Some(wear) = world.get::<ClothingWear>(item).cloned() else {
        return;
    };
    world.entity_mut(item).remove::<ClothingWear>();
    if let Some(mut c) = world.get_mut::<DmClothing>(item) {
        c.body_parts_covered = wear.initial_covered;
    }
    if let Some(mut i) = world.get_mut::<DmItem>(item) {
        i.slot_flags = wear.initial_slot_flags;
    }
    set_name(world, item, wear.initial_name.clone());
    // `/obj/item/clothing/under/update_clothes_damaged_state(CLOTHING_PRISTINE)`.
    super::sensors::repair_broken(world, item);
    if let Some(client) = client {
        tell(
            world,
            client,
            Span::Notice,
            format!("Вы починили повреждения на {}.", wear.initial_name),
        );
    }
    refresh_wearer(world, item);
}

/// tg `/obj/item/clothing/item_interaction` with `repairable_by` (cloth): a damaged piece is
/// mended with one sheet at once, a shredded one with three over 6 s. False for other items.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if !is_under(&path_of(world, held), CLOTH) || world.get::<DmClothing>(target).is_none() {
        return false;
    }
    let Some(state) = world.get::<ClothingWear>(target).map(|w| w.state) else {
        return false;
    };
    match state {
        Wear::Pristine => false,
        Wear::Damaged => {
            if crate::tools::use_stack(world, held, 1) {
                repair(world, Some(client), target);
            }
            true
        }
        Wear::Shredded => {
            let name = crate::security::name_of(world, target);
            if crate::tools::stack_amount(world, held) < SHREDDED_SHEETS {
                tell(
                    world,
                    client,
                    Span::Warning,
                    format!("Вам требуется три ткани, чтобы починить {name}."),
                );
                return true;
            }
            tell(
                world,
                client,
                Span::Notice,
                format!("Вы начинаете чинить повреждения на {name}..."),
            );
            // The do_after stands on the mob: the item may be in a hand.
            let actor = Actor {
                client,
                mob,
                item: Some(held),
            };
            let on = if world.get::<ss13_shared::TilePos>(target).is_some() {
                target
            } else {
                mob
            };
            crate::tools::do_after(
                world,
                actor,
                on,
                REPAIR_DS,
                0,
                crate::construction::Job::Clothing(ClothingJob::Repair { item: target }),
            );
            true
        }
    }
}

/// The end of a [`ClothingJob`], revalidated: the item is still shredded and in reach, the cloth
/// still in hand with three sheets.
pub fn complete(world: &mut World, mob: Entity, action: &crate::tools::DoAfter, job: ClothingJob) {
    let item = match job {
        ClothingJob::Repair { item } => item,
        ClothingJob::ChameleonScan { target, equip } => {
            super::chameleon::complete_scan(world, mob, action, target, equip);
            return;
        }
    };
    let Some(cloth) = action.item else {
        return;
    };
    let shredded = world
        .get::<ClothingWear>(item)
        .is_some_and(|w| w.state == Wear::Shredded);
    let reachable = wearer(world, item) == Some(mob) || crate::storage::in_reach(world, mob, item);
    let held = world
        .get::<Inventory>(mob)
        .is_some_and(|i| i.active_item() == Some(cloth));
    if !shredded || !reachable || !held || !crate::tools::use_stack(world, cloth, SHREDDED_SHEETS) {
        return;
    }
    repair(world, Some(action.client), item);
}

/// tg `/obj/item/clothing/examine`: shredded, or how torn each damaged zone is.
pub fn examine_lines(world: &World, item: Entity, lines: &mut Vec<crate::examine::Line>) {
    let (Some(wear), Some(clothing)) = (
        world.get::<ClothingWear>(item),
        world.get::<DmClothing>(item),
    ) else {
        return;
    };
    if wear.state == Wear::Shredded {
        lines.push((
            Span::Warning,
            "Материалы одежды повреждены и требуют починки, прежде чем можно будет носить!"
                .to_owned(),
        ));
        return;
    }
    if clothing.limb_integrity <= 0.0 {
        return;
    }
    for zone in BodyZone::ALL {
        let pct = wear.by_zone[zone.index()] / clothing.limb_integrity * 100.0;
        let part = crate::forensics::capitalize(zone.ru().0);
        if pct >= 100.0 {
            lines.push((Span::Warning, format!("{part} требует починки!")));
        } else if pct >= 60.0 {
            lines.push((Span::Warning, format!("{part} сильно изорвана!")));
        } else if pct >= 30.0 {
            lines.push((Span::Danger, format!("{part} частично изорвана.")));
        }
    }
}

/// Whether the worn item shows tg's damage overlay (`damaged_clothes`).
pub fn damaged(world: &World, item: Entity) -> bool {
    world
        .get::<ClothingWear>(item)
        .is_some_and(|w| w.state != Wear::Pristine)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zones_follow_cover_flags2body_zones() {
        // A jumpsuit: CHEST|GROIN|LEGS|ARMS.
        let jumpsuit = CHEST | GROIN | LEG_LEFT | LEG_RIGHT | ARM_LEFT | ARM_RIGHT;
        assert!(covers_zone(jumpsuit, BodyZone::Chest));
        assert!(covers_zone(jumpsuit, BodyZone::LArm));
        assert!(!covers_zone(jumpsuit, BodyZone::Head));
        // Only the groin is not the chest zone.
        assert!(!covers_zone(GROIN, BodyZone::Chest));
        // Gloves cover the arm zones through the hands.
        assert!(covers_zone(HAND_LEFT | HAND_RIGHT, BodyZone::RArm));
        assert_eq!(
            jumpsuit & !zone_cover(BodyZone::Chest),
            jumpsuit & !(CHEST | GROIN)
        );
    }

    fn jumpsuit(world: &mut World) -> Entity {
        world
            .spawn((
                DmClothing {
                    body_parts_covered: CHEST | GROIN | LEG_LEFT | LEG_RIGHT | ARM_LEFT | ARM_RIGHT,
                    limb_integrity: 30.0,
                    ..default()
                },
                DmItem {
                    path: "/obj/item/clothing/under/color/grey".into(),
                    slot_flags: 1 << 1,
                    max_integrity: 200.0,
                    ..default()
                },
                DmName {
                    name: "grey jumpsuit".into(),
                    desc: String::new(),
                },
            ))
            .id()
    }

    #[test]
    fn a_zone_tears_then_the_item_shreds() {
        let mut world = World::new();
        let suit = jumpsuit(&mut world);
        // 20 damage: 2 off the integrity, 20 to the chest zone (limit 30).
        take_damage_zone(&mut world, suit, BodyZone::Chest, 20.0, false);
        let wear = world.get::<ClothingWear>(suit).unwrap().clone();
        assert!((wear.integrity - 198.0).abs() < 1e-3);
        assert!((wear.by_zone[BodyZone::Chest.index()] - 20.0).abs() < 1e-3);
        assert_eq!(wear.state, Wear::Pristine);
        // Past 30: the chest is disabled and the jumpsuit renamed.
        take_damage_zone(&mut world, suit, BodyZone::Chest, 20.0, false);
        let covered = world.get::<DmClothing>(suit).unwrap().body_parts_covered;
        assert_eq!(covered & (CHEST | GROIN), 0);
        assert_eq!(
            world.get::<ClothingWear>(suit).unwrap().state,
            Wear::Damaged
        );
        assert_eq!(
            world.get::<DmName>(suit).unwrap().name,
            "damaged grey jumpsuit"
        );
        // The chest no longer counts; the arms and legs go one by one.
        take_damage_zone(&mut world, suit, BodyZone::Chest, 20.0, false);
        assert_eq!(world.get::<ClothingWear>(suit).unwrap().zones_disabled, 1);
        for zone in [
            BodyZone::LArm,
            BodyZone::RArm,
            BodyZone::LLeg,
            BodyZone::RLeg,
        ] {
            take_damage_zone(&mut world, suit, zone, 25.0, true);
            take_damage_zone(&mut world, suit, zone, 25.0, true);
        }
        let wear = world.get::<ClothingWear>(suit).unwrap().clone();
        assert_eq!(wear.state, Wear::Shredded);
        assert_eq!(world.get::<DmItem>(suit).unwrap().slot_flags, 0);
        assert_eq!(
            world.get::<DmName>(suit).unwrap().name,
            "shredded grey jumpsuit"
        );
        // Repaired: all back.
        repair(&mut world, None, suit);
        assert!(world.get::<ClothingWear>(suit).is_none());
        assert_eq!(world.get::<DmItem>(suit).unwrap().slot_flags, 1 << 1);
        assert_eq!(world.get::<DmName>(suit).unwrap().name, "grey jumpsuit");
        assert_ne!(
            world.get::<DmClothing>(suit).unwrap().body_parts_covered & CHEST,
            0
        );
    }

    #[test]
    fn single_part_items_and_no_limb_integrity_are_not_tracked() {
        let mut world = World::new();
        let hat = world
            .spawn((
                DmClothing {
                    body_parts_covered: HEAD,
                    limb_integrity: 30.0,
                    ..default()
                },
                DmItem::default(),
            ))
            .id();
        take_damage_zone(&mut world, hat, BodyZone::Head, 50.0, false);
        assert!(world.get::<ClothingWear>(hat).is_none());
        let suit = jumpsuit(&mut world);
        world.get_mut::<DmClothing>(suit).unwrap().limb_integrity = 0.0;
        take_damage_zone(&mut world, suit, BodyZone::Chest, 50.0, false);
        assert!(world.get::<ClothingWear>(suit).is_none());
    }
}
