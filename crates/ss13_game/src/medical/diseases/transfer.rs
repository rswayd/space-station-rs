//! The scanner's disease block and diseases carried in drawn blood.

use super::*;

/// tg `healthscan`'s disease block: one entry per disease the scanner sees.
pub fn scan_lines(world: &World, mob: Entity) -> Vec<(Span, String)> {
    let mut lines = Vec::new();
    for disease in of_mob(world, mob) {
        if disease.visibility & HIDDEN_SCANNER != 0 {
            continue;
        }
        lines.push((
            Span::BoldWarning,
            format!("Внимание: {} обнаружена", disease.form()),
        ));
        lines.push((Span::Warning, format!("Название: {}.", disease.name)));
        lines.push((
            Span::Warning,
            format!("Распространение: {}.", disease.spread_text),
        ));
        lines.push((
            Span::Warning,
            format!("Stage: {}/{}.", disease.stage, disease.max_stages),
        ));
        lines.push((
            Span::Warning,
            format!("Возможное лекарство: {}", disease.cure_text(2)),
        ));
    }
    lines
}

/// tg `get_blood_data` into a container: a copy of each of the donor's diseases and its
/// antibodies join the container's blood (`blood_reagent` `on_merge`).
pub fn blood_drawn(world: &mut World, donor: Entity, container: Entity) {
    let Some(all) = world.get::<Diseases>(donor) else {
        return;
    };
    let data = BloodDiseases {
        viruses: all.list.iter().map(Disease::copy).collect(),
        resistances: all.resistances.clone(),
    };
    merge_blood(world, container, data);
}

/// Blood moved from container `from` to container `to`: its viruses go along.
pub fn carry_blood(world: &mut World, from: Entity, to: Entity) {
    let Some(data) = world.get::<BloodDiseases>(from).cloned() else {
        return;
    };
    if blood_volume(world, to) > 0.0 {
        merge_blood(world, to, data);
    }
    forget_if_bloodless(world, from);
}

/// tg `blood_reagent` `on_merge`: simple diseases are kept, advanced ones mixed into one
/// (`Advance_Mix`); antibodies add up.
pub(super) fn merge_blood(world: &mut World, container: Entity, data: BloodDiseases) {
    if data.viruses.is_empty() && data.resistances.is_empty() {
        return;
    }
    let mut rng = super::super::fork_rng(world);
    let Ok(mut entity) = world.get_entity_mut(container) else {
        return;
    };
    let Some(mut held) = entity.get_mut::<BloodDiseases>() else {
        entity.insert(data);
        return;
    };
    if !held.viruses.is_empty() && !data.viruses.is_empty() {
        let mut keep: Vec<Disease> = Vec::new();
        let mut mix: Vec<Disease> = Vec::new();
        for d in held.viruses.drain(..).chain(data.viruses) {
            if d.advance.is_some() {
                mix.push(d);
            } else if !keep.iter().any(|k| k.id() == d.id()) {
                keep.push(d);
            }
        }
        keep.extend(symptoms::advance_mix(mix, &mut rng));
        held.viruses = keep;
    } else if held.viruses.is_empty() {
        held.viruses = data.viruses;
    }
    for id in data.resistances {
        if !held.resistances.contains(&id) {
            held.resistances.push(id);
        }
    }
}

pub(super) fn blood_volume(world: &World, container: Entity) -> f32 {
    let (Some(chem), Some(reagents)) = (
        world.get_resource::<crate::chem::Chem>(),
        world.get::<crate::chem::Reagents>(container),
    ) else {
        return 0.0;
    };
    chem.0
        .id(super::super::blood::BLOOD_REAGENT)
        .map_or(0.0, |id| reagents.0.amount(id))
}

pub(super) fn forget_if_bloodless(world: &mut World, container: Entity) {
    if blood_volume(world, container) <= 0.0
        && let Ok(mut entity) = world.get_entity_mut(container)
    {
        entity.remove::<BloodDiseases>();
    }
}

/// tg `blood_reagent` `on_expose_mob` for blood from `container` that went into `mob`
/// (`INGEST` into the stomach, else `INJECT`): every contagious virus infects (through the
/// stomach or the heart), antibodies cure the diseases they match.
pub fn blood_exposed(world: &mut World, container: Entity, mob: Entity, ingest: bool) {
    let Some(data) = world.get::<BloodDiseases>(container).cloned() else {
        return;
    };
    let organ = if ingest { Slot::Stomach } else { Slot::Heart };
    for strain in &data.viruses {
        if strain.spread_flags & (SPREAD_SPECIAL | SPREAD_NON_CONTAGIOUS) != 0 {
            continue;
        }
        if has_organ(world, mob, organ) {
            force_contract(world, mob, strain);
        }
    }
    if !data.resistances.is_empty() {
        let mut index = of_mob(world, mob).len();
        while index > 0 {
            index -= 1;
            let d = &of_mob(world, mob)[index];
            if !d.def().bypasses_immunity && data.resistances.contains(&d.id()) {
                cure(world, mob, index, false);
            }
        }
    }
    forget_if_bloodless(world, container);
}

/// tg `/datum/reagent/vaccine/expose_mob` (`INGEST` or `INJECT`): the diseases whose id the
/// vaccine names are cured with a resistance, and the mob resists those ids from now on.
pub fn vaccinate(world: &mut World, mob: Entity, ids: &[String]) {
    if world.get::<Diseases>(mob).is_none() || ids.is_empty() {
        return;
    }
    let mut index = of_mob(world, mob).len();
    while index > 0 {
        index -= 1;
        if ids.contains(&of_mob(world, mob)[index].id()) {
            cure(world, mob, index, true);
        }
    }
    if let Some(mut all) = world.get_mut::<Diseases>(mob) {
        for id in ids {
            if !all.resistances.contains(id) {
                all.resistances.push(id.clone());
            }
        }
    }
}
