//! Contracting, curing and spreading: contact, touch and airborne.

use super::*;

/// A human (`viable_mobtypes = /mob/living/carbon/human` for every ported disease).
pub(super) fn is_human(world: &World, mob: Entity) -> bool {
    world.get::<super::super::wounds::Limbs>(mob).is_some() && world.get::<Health>(mob).is_some()
}

/// `has_required_infectious_organ`: the organ is there and not robotic; a mob without
/// modelled organs has them all.
// shortcut: inorganic biology does not open robotic organs to the disease.
pub(crate) fn has_organ(world: &World, mob: Entity, slot: Slot) -> bool {
    world.get::<organs::OrganHost>(mob).is_none()
        || organs::state(world, mob, slot).is_some_and(|o| !o.robotic)
}

/// tg `CanContractDisease` (human).
pub fn can_contract(world: &World, mob: Entity, disease: &Disease) -> bool {
    if !is_human(world, mob) {
        return false;
    }
    if let Some(slot) = disease.def().required_organ
        && !has_organ(world, mob, slot)
    {
        return false;
    }
    if world.get::<Stat>(mob) == Some(&Stat::Dead) && !disease.def().process_dead {
        return false;
    }
    let id = disease.id();
    if world
        .get::<Diseases>(mob)
        .is_some_and(|d| d.resistances.contains(&id))
    {
        return false;
    }
    !has_disease(world, mob, disease)
}

/// tg `ForceContractDisease`: infects with a copy of `disease` unless the mob cannot catch it.
/// The API for events and admins.
pub fn force_contract(world: &mut World, mob: Entity, disease: &Disease) -> bool {
    can_contract(world, mob, disease) && try_infect(world, mob, disease)
}

/// tg `try_infect`; advanced diseases first push out a weaker advanced one (`DISEASE_LIMIT`).
pub(super) fn try_infect(world: &mut World, mob: Entity, disease: &Disease) -> bool {
    if let Some(advance) = &disease.advance {
        let mut rivals: Vec<(String, i32)> = of_mob(world, mob)
            .iter()
            .filter_map(|d| d.advance.as_ref().map(|a| (a.id(), a.props.resistance)))
            .collect();
        // `DISEASE_LIMIT` 1: all of them must go.
        rivals.sort_by_key(|r| r.1);
        for (id, resistance) in rivals {
            if advance.props.transmittable <= resistance {
                return false;
            }
            if let Some(index) = of_mob(world, mob).iter().position(|d| d.id() == id) {
                cure(world, mob, index, false);
            }
        }
    }
    infect(world, mob, disease.copy());
    true
}

/// tg `infect`: no checks.
pub(super) fn infect(world: &mut World, mob: Entity, disease: Disease) {
    let Ok(mut entity) = world.get_entity_mut(mob) else {
        return;
    };
    match entity.get_mut::<Diseases>() {
        Some(mut all) => all.list.push(disease),
        None => {
            entity.insert(Diseases {
                list: vec![disease],
                ..default()
            });
        }
    }
}

/// tg `cure(add_resistance)` of `mob`'s disease at `index`: an uncurable one stays.
pub fn cure(world: &mut World, mob: Entity, index: usize, add_resistance: bool) {
    let Some(mut all) = world.get_mut::<Diseases>(mob) else {
        return;
    };
    let Some(disease) = all.list.get(index) else {
        return;
    };
    if disease.severity == Severity::Uncurable {
        return;
    }
    let disease = all.list.remove(index);
    if !add_resistance {
        return;
    }
    if let Some(advance) = &disease.advance {
        for name in advance.resisted_symptoms() {
            if !all.symptom_resistances.contains(&name) {
                all.symptom_resistances.push(name);
            }
        }
    }
    if disease.def().disease_flags & CAN_RESIST == 0 {
        return;
    }
    let mut ids = vec![disease.id()];
    // "Buy one, get one free": the variant's antibodies come along.
    let extra = match disease.kind {
        Kind::Cold => Some(COLD9.path),
        Kind::Cold9 => Some(COLD.path),
        Kind::Flu => Some(FLU_SPANISH.path),
        Kind::FluSpanish => Some(FLU.path),
        _ => None,
    };
    ids.extend(extra.map(str::to_owned));
    for id in ids {
        if !all.resistances.contains(&id) {
            all.resistances.push(id);
        }
    }
}

/// tg `fully_heal(HEAL_NEGATIVE_DISEASES)`: every disease but the positive ones, no immunity.
pub fn cure_negative(world: &mut World, mob: Entity) {
    let mut index = of_mob(world, mob).len();
    while index > 0 {
        index -= 1;
        if of_mob(world, mob)[index].severity != Severity::Positive {
            cure(world, mob, index, false);
        }
    }
}

/// tg `ContactContractDisease` of a carbon: a touch on `zone` (random by tg's weights without
/// one) passes the bio armour covering it.
pub fn contact_contract(
    world: &mut World,
    mob: Entity,
    disease: &Disease,
    zone: Option<BodyZone>,
    rng: &mut CombatRng,
) -> bool {
    if !can_contract(world, mob, disease) {
        return false;
    }
    // DM divides by zero here (a runtime): nothing spreads.
    if disease.spreading_modifier <= 0.0 || rng.chance(15.0 / disease.spreading_modifier) {
        return false;
    }
    let satiety = world
        .get::<crate::hunger::Hunger>(mob)
        .map_or(0.0, |h| h.satiety);
    if satiety > 0.0 && rng.chance(satiety / 2.0) {
        return false;
    }
    let zone = zone.unwrap_or_else(|| {
        // head 80, chest 100, hands 17.5 each, feet 7.5 each.
        let roll = rng.unit() * 230.0;
        match roll {
            r if r < 80.0 => BodyZone::Head,
            r if r < 180.0 => BodyZone::Chest,
            r if r < 197.5 => BodyZone::RArm,
            r if r < 215.0 => BodyZone::LArm,
            r if r < 222.5 => BodyZone::RLeg,
            _ => BodyZone::LLeg,
        }
    });
    if virus_resistant(world, mob) && rng.chance(75.0) {
        return false;
    }
    let inventory = world.get::<Inventory>(mob).cloned().unwrap_or_default();
    let covers = |item: Option<Entity>, bits: u32| {
        item.filter(|&i| {
            world
                .get::<DmClothing>(i)
                .is_some_and(|c| c.body_parts_covered & bits != 0)
        })
    };
    let layers: [Option<Entity>; 3] = match zone {
        BodyZone::Head => [inventory.head, inventory.mask, inventory.neck],
        BodyZone::Chest => [inventory.suit, inventory.uniform, None],
        BodyZone::LArm | BodyZone::RArm => [covers(inventory.suit, HANDS), inventory.gloves, None],
        BodyZone::LLeg | BodyZone::RLeg => [covers(inventory.suit, FEET), inventory.shoes, None],
    };
    for item in layers.into_iter().flatten() {
        if !rng.chance(100.0 - bio(world, Some(item))) {
            return false;
        }
    }
    try_infect(world, mob, disease)
}

/// tg `attack_hand`/`start_pulling` between two living mobs: their skin-contact diseases go
/// both ways.
pub fn touch(world: &mut World, a: Entity, b: Entity) {
    if a == b {
        return;
    }
    let skin = |world: &World, mob: Entity| -> Vec<Disease> {
        of_mob(world, mob)
            .iter()
            .filter(|d| d.spread_flags & SPREAD_SKIN != 0)
            .cloned()
            .collect()
    };
    let (from_a, from_b) = (skin(world, a), skin(world, b));
    if from_a.is_empty() && from_b.is_empty() {
        return;
    }
    let mut rng = super::super::fork_rng(world);
    for disease in &from_b {
        contact_contract(world, a, disease, None, &mut rng);
    }
    for disease in &from_a {
        contact_contract(world, b, disease, None, &mut rng);
    }
}

/// The bio rating of a worn item.
pub(super) fn bio(world: &World, item: Option<Entity>) -> f32 {
    item.and_then(|i| world.get::<DmArmor>(i))
        .map_or(0.0, |a| a.bio)
}

/// Bio armour over the mouth: helmet and mask (`is_mouth_covered`).
// shortcut: any head or mask item counts as covering the mouth (tg: `HEADCOVERSMOUTH` /
// `MASKCOVERSMOUTH`); items without bio armour add nothing either way.
pub(super) fn mouth_bio(world: &World, mob: Entity) -> f32 {
    world
        .get::<Inventory>(mob)
        .map_or(0.0, |inv| bio(world, inv.head) + bio(world, inv.mask))
}

/// `TRAIT_VIRUS_RESISTANCE`: spaceacillin in the blood.
pub(crate) fn virus_resistant(world: &World, mob: Entity) -> bool {
    blood_has(world, mob, SPACEACILLIN, false)
}

/// tg `has_reagent` on the blood (`check_subtypes`).
pub(crate) fn blood_has(world: &World, mob: Entity, path: &str, subtypes: bool) -> bool {
    let (Some(chem), Some(m)) = (
        world.get_resource::<crate::chem::Chem>(),
        world.get::<crate::chem::MobReagents>(mob),
    ) else {
        return false;
    };
    m.blood.reagents.iter().any(|r| {
        r.volume > 0.0
            && if subtypes {
                chem.0.is_type(r.id, path)
            } else {
                chem.0.def(r.id).path == path
            }
    })
}

/// Removes up to `amount` of every blood reagent of type `path` (`include_subtypes`); returns
/// the units removed.
pub(crate) fn remove_blood(world: &mut World, mob: Entity, path: Option<&str>, amount: f32) -> f32 {
    let Some(chem) = world
        .get_resource::<crate::chem::Chem>()
        .map(|c| c.0.clone())
    else {
        return 0.0;
    };
    let Some(mut m) = world.get_mut::<crate::chem::MobReagents>(mob) else {
        return 0.0;
    };
    let ids: Vec<_> = m
        .blood
        .reagents
        .iter()
        .filter(|r| path.is_none_or(|p| chem.is_type(r.id, p)))
        .map(|r| r.id)
        .collect();
    let mut removed = 0.0;
    for id in ids {
        removed += m.blood.remove_reagent(id, amount);
    }
    removed
}

pub(super) fn losebreath(world: &World, mob: Entity) -> f32 {
    world
        .get::<crate::breath::Respiration>(mob)
        .map_or(0.0, |r| r.losebreath)
}

pub(super) fn on_internals(world: &World, mob: Entity) -> bool {
    world
        .get::<crate::internals::Internals>(mob)
        .is_some_and(|i| i.0.is_some())
}

/// tg `can_spread_airborne_diseases` (carbon).
pub(super) fn can_spread_airborne(world: &World, mob: Entity, rng: &mut CombatRng) -> bool {
    if on_internals(world, mob) || losebreath(world, mob) >= 1.0 {
        return false;
    }
    if world
        .get::<Health>(mob)
        .is_some_and(|h| !h.has_part(BodyZone::Head))
    {
        return true;
    }
    !rng.chance(2.0 * mouth_bio(world, mob))
}

/// tg `can_be_spread_airborne_disease` (carbon).
pub(super) fn can_catch_airborne(world: &World, mob: Entity, rng: &mut CombatRng) -> bool {
    if on_internals(world, mob) && rng.chance(75.0) {
        return false;
    }
    if losebreath(world, mob) >= 1.0 {
        return false;
    }
    if virus_resistant(world, mob) && rng.chance(75.0) {
        return false;
    }
    !rng.chance(mouth_bio(world, mob))
}

/// tg `contract_airborne_disease`.
pub(super) fn contract_airborne(
    world: &mut World,
    mob: Entity,
    disease: &Disease,
    rng: &mut CombatRng,
) {
    if !can_catch_airborne(world, mob, rng)
        || !rng.chance((50.0 * disease.spreading_modifier - 1.0).min(50.0))
        || !has_organ(world, mob, Slot::Lungs)
    {
        return;
    }
    force_contract(world, mob, disease);
}

/// tg `airborne_spread(spread_range, force_spread, require_facing)` from `mob`: every human in
/// view within `range` with an air path.
// tg returns early when the host *can* spread (`if(affected_mob.can_spread_airborne_diseases())
// return FALSE`), an inverted check; the intent (masks and internals stop the spreader) is
// ported.
pub(crate) fn airborne_spread(
    world: &mut World,
    mob: Entity,
    disease: &Disease,
    range: u32,
    force: bool,
    require_facing: bool,
    rng: &mut CombatRng,
) -> bool {
    if disease.spread_flags & SPREAD_AIRBORNE == 0 && !force {
        return false;
    }
    if !can_spread_airborne(world, mob, rng)
        || !has_organ(world, mob, Slot::Lungs)
        || virus_resistant(world, mob)
    {
        return false;
    }
    let Some(&from) = world.get::<TilePos>(mob) else {
        return false;
    };
    let Some(index) = world.get_resource::<SpatialIndex>() else {
        return false;
    };
    let facing = world.get::<Facing>(mob).map(|f| f.0);
    let lying = world.get::<BodyPosition>(mob).is_some_and(|b| b.lying());
    let targets: Vec<Entity> = index
        .in_radius(from, range)
        .filter(|&e| e != mob && is_human(world, e))
        .filter(|&e| {
            let Some(&to) = world.get::<TilePos>(e) else {
                return false;
            };
            (!require_facing || (!lying && facing.is_some_and(|d| faces(d, from, to))))
                && crate::mobs::ai::behaviors::can_see(world, from, to, range)
                && air_path(world, from, to)
        })
        .collect();
    for target in targets {
        contract_airborne(world, target, disease, rng);
    }
    true
}

/// tg `is_source_facing_target`: the target is in front or 45 degrees off.
pub(super) fn faces(dir: Dir, from: TilePos, to: TilePos) -> bool {
    const RING: [Dir; 8] = [
        Dir::North,
        Dir::NorthEast,
        Dir::East,
        Dir::SouthEast,
        Dir::South,
        Dir::SouthWest,
        Dir::West,
        Dir::NorthWest,
    ];
    let Some(goal) = dir_of((to.x - from.x).signum(), (to.y - from.y).signum()) else {
        return false;
    };
    let at = |d: Dir| RING.iter().position(|&r| r == d).unwrap_or(0);
    matches!((at(goal) + 8 - at(dir)) % 8, 0 | 1 | 7)
}

pub(super) fn dir_of(dx: i32, dy: i32) -> Option<Dir> {
    let bits = (u8::from(dy > 0))
        | (u8::from(dy < 0) << 1)
        | (u8::from(dx > 0) << 2)
        | (u8::from(dx < 0) << 3);
    Dir::from_bits(bits)
}

/// tg `disease_air_spread_walk`: from `end` towards `start`, every step between turfs that
/// share air (`TURFS_CAN_SHARE`). Without simulated air every path is open.
pub(super) fn air_path(world: &World, start: TilePos, end: TilePos) -> bool {
    let Some(level) = world
        .get_resource::<ss13_atmos::Atmos>()
        .and_then(|a| a.level(start.z))
    else {
        return true;
    };
    let mut at = end;
    while at != start {
        let (dx, dy) = ((start.x - at.x).signum(), (start.y - at.y).signum());
        let next = TilePos::new(at.x + dx, at.y + dy, at.z);
        if !shares_air(level, at, next) {
            return false;
        }
        at = next;
    }
    true
}

/// `TURFS_CAN_SHARE(a, b)` of neighbours: adjacent for air, or (diagonal) sharing an
/// air-adjacent turf.
pub(super) fn shares_air(level: &ss13_atmos::LevelAir, a: TilePos, b: TilePos) -> bool {
    let (Some(ta), Some(tb)) = (level.tile(a.x, a.y), level.tile(b.x, b.y)) else {
        return false;
    };
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let (Some(h), Some(v)) = (dir_of(dx, 0), dir_of(0, dy)) else {
        return dir_of(dx, dy).is_some_and(|d| level.adjacent(ta, d));
    };
    (level.adjacent(ta, h) && level.adjacent(tb, v.reverse()))
        || (level.adjacent(ta, v) && level.adjacent(tb, h.reverse()))
}
