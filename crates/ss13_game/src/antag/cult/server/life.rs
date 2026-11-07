//! Clicks on mobs, and holy water in a cultist's blood.

use super::*;

/// A blood magic aura or the dagger on a mob, before melee: the spells, and no hurting other
/// cultists with the dagger (it purges their holy water instead).
pub(super) fn cult_clicks(
    world: &mut World,
    mut clicks: Local<MessageCursor<crate::click::ClickResolved>>,
) {
    let clicks: Vec<crate::click::ClickResolved> = clicks
        .read(world.resource::<Messages<crate::click::ClickResolved>>())
        .copied()
        .collect();
    for click in clicks {
        let (mob, target) = (click.mob, click.target);
        if world
            .resource::<crate::security::MeleeHandled>()
            .0
            .contains(&mob)
            || world.get::<Health>(target).is_none()
        {
            continue;
        }
        let Some(held) = world.get::<Inventory>(mob).and_then(Inventory::active_item) else {
            continue;
        };
        let t = now(world);
        if world.get::<NextMove>(mob).is_some_and(|n| t < n.0) {
            continue;
        }
        if let Some(aura) = world.get::<BloodAura>(held).copied() {
            world
                .resource_mut::<crate::security::MeleeHandled>()
                .0
                .insert(mob);
            cast_aura(world, click.client, mob, held, aura, target);
            continue;
        }
        if path_of(world, held).as_deref() == Some(DAGGER)
            && world.get::<Cultist>(mob).is_some()
            && world.get::<Cultist>(target).is_some()
        {
            world
                .resource_mut::<crate::security::MeleeHandled>()
                .0
                .insert(mob);
            purge_holy_water(world, mob, target, held);
        }
    }
}

/// `do_purge_holywater`: the holy water in the target's blood turns into unholy water.
pub(super) fn purge_holy_water(world: &mut World, mob: Entity, target: Entity, dagger: Entity) {
    let Some(chem) = world
        .get_resource::<crate::chem::Chem>()
        .map(|c| c.0.clone())
    else {
        return;
    };
    let Some(id) = chem.id(HOLY_WATER) else {
        return;
    };
    let Some(mut reagents) = world.get::<crate::chem::MobReagents>(target).cloned() else {
        return;
    };
    let amount = reagents.blood.amount(id) + reagents.stomach.amount(id);
    if amount <= 0.0 {
        return;
    }
    reagents.blood.remove_reagent(id, f32::MAX);
    reagents.stomach.remove_reagent(id, f32::MAX);
    if let Some(unholy) = chem.id("/datum/reagent/fuel/unholywater") {
        reagents
            .blood
            .add_reagent(&chem, unholy, amount, 310.15, None, None, false, false);
    }
    world.entity_mut(target).insert(reagents);
    if let Some(mut c) = world.get_mut::<Cultist>(target) {
        c.holy_secs = 0.0;
    }
    let (victim, tool) = (name_of(world, target), name_of(world, dagger));
    chat_to(
        world,
        mob,
        Span::Cult,
        format!("You remove the taint from {victim} using {tool}."),
    );
}

// ---------------------------------------------------------------------------------------------
// Life: holy water

/// Every `Life()` (2 s): holy water in a cultist's blood (`holywater/on_mob_life`).
pub(super) fn cult_life(world: &mut World, mut last_fired: Local<Option<u64>>) {
    let fired = world.resource::<crate::breath::MobsClock>().times_fired;
    let fires = fired.saturating_sub(last_fired.unwrap_or(fired));
    *last_fired = Some(fired);
    if fires == 0 {
        return;
    }
    let secs = crate::breath::SSMOBS_WAIT_SECS as f32 * fires as f32;
    let mut q = world.query_filtered::<Entity, With<Cultist>>();
    let mut mobs: Vec<Entity> = q.iter(world).collect();
    mobs.sort();
    for mob in mobs {
        holy_water_tick(world, mob, secs);
    }
}

/// What holy water does to a cultist after `holy_secs` of it: (spells cleared, babbling stage,
/// deconverted).
pub fn holy_water_stage(holy_secs: f32) -> (bool, bool) {
    (
        holy_secs >= HOLY_WATER_STUTTER_SECS,
        holy_secs >= HOLY_WATER_DECONVERT_SECS,
    )
}

// shortcut: the time counts every `Life()` the holy water is in the blood, not by the
// metabolized share (`metabolization_ratio`); a microdose counts like a gulp.
pub(super) fn holy_water_tick(world: &mut World, mob: Entity, secs: f32) {
    if world.get::<Stat>(mob) == Some(&Stat::Dead) {
        return;
    }
    if !has_holy_water(world, mob) {
        if let Some(mut c) = world.get_mut::<Cultist>(mob)
            && c.holy_secs != 0.0
        {
            c.holy_secs = 0.0;
        }
        return;
    }
    let first = world
        .get::<Cultist>(mob)
        .is_some_and(|c| c.holy_secs == 0.0);
    if first {
        chat_to(
            world,
            mob,
            Span::BoldWarning,
            "A vile holiness begins to spread its shining tendrils through your mind, purging the \
             Geometer of Blood's influence!",
        );
    }
    let had_spells = world
        .get::<Cultist>(mob)
        .is_some_and(|c| !c.spells.is_empty());
    if had_spells {
        drop_auras(world, mob);
        if let Some(mut c) = world.get_mut::<Cultist>(mob) {
            c.spells.clear();
        }
        chat_to(
            world,
            mob,
            Span::CultLarge,
            "Your blood rites falter as holy water scours your body!",
        );
        refresh(world, mob);
    }
    let total = {
        let Some(mut c) = world.get_mut::<Cultist>(mob) else {
            return;
        };
        c.holy_secs += secs;
        c.holy_secs
    };
    let (babbling, deconverted) = holy_water_stage(total);
    {
        use crate::status_effects::{Timed, adjust_timed, set_timed};
        adjust_timed(world, mob, Timed::Jitter, 2.0 * secs, Some(20.0));
        if babbling {
            adjust_timed(world, mob, Timed::Stutter, 2.0 * secs, Some(20.0));
            set_timed(world, mob, Timed::Dizziness, 10.0, true);
        }
    }
    if deconverted {
        remove_cultist(world, mob, false);
        crate::status_effects::apply(world, mob, Incap::Unconscious, 10.0);
        if let (Some(chem), Some(mut reagents)) = (
            world
                .get_resource::<crate::chem::Chem>()
                .map(|c| c.0.clone()),
            world.get::<crate::chem::MobReagents>(mob).cloned(),
        ) && let Some(id) = chem.id(HOLY_WATER)
        {
            reagents.blood.remove_reagent(id, f32::MAX);
            world.entity_mut(mob).insert(reagents);
        }
        return;
    }
    // `SPT_PROB(10, seconds_per_tick)`.
    let babble = babbling
        && world
            .get_resource_mut::<crate::combat::CombatRng>()
            .is_some_and(|mut r| r.chance(100.0 * (1.0 - 0.9f32.powf(secs))));
    if babble {
        const WORDS: [&str; 8] = [
            "Av'te Nar'Sie",
            "Pa'lid Mors",
            "INO INO ORA ANA",
            "SAT ANA!",
            "Daim'niodeis Arc'iai Le'eones",
            "R'ge Na'sie",
            "Diabo us Vo'iscum",
            "Eld' Mon Nobis",
        ];
        let i = world
            .get_resource_mut::<crate::combat::CombatRng>()
            .map_or(0, |mut r| r.range(0, 7) as usize);
        say(world, mob, WORDS[i]);
        let seizure = world
            .get_resource_mut::<crate::combat::CombatRng>()
            .is_some_and(|mut r| r.chance(10.0));
        if seizure {
            let name = name_of(world, mob);
            crate::chem::visible(
                world,
                mob,
                "You have a seizure!",
                &format!("{name} starts having a seizure!"),
                Span::Danger,
            );
            crate::status_effects::apply(world, mob, Incap::Unconscious, 12.0);
            chat_to(world, mob, Span::CultLarge, "Do not forget your place.");
        }
    }
}
