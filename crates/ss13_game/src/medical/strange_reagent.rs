//! Strange reagent revival (server), after tg `/datum/reagent/medicine/strange_reagent/expose_mob`
//! (`medicine_reagents.dm`) and `do_strange_reagent_revival` (`living.dm`, `carbon.dm`).
//!
//! A dead organic carbon that swallows strange reagent ([`expose`], from
//! `crate::chem::transfer_to_mob`) is revived 7 s later when its brute + burn are at most twice
//! its max health and the dose covers what it takes to bring its health above 0 (5 damage a
//! unit); a suicide, a husk or a too damaged or underdosed body only reacts. The healing goes
//! half to brute, the rest to burn; what is left (the excess dose counts at 80%) heals oxygen
//! and toxin damage in `revive`. The body jitters at once, after 4 s and after 8 s.
// shortcut: only carbons (tg also revives splashed simple mobs: basic mobs have no biotypes
// here); no fishy reagent; the reagent's random brute/burn while metabolized is chemistry's.

use bevy::prelude::*;
use ss13_shared::chat::Span;

use crate::chem::{name_of, visible};
use crate::health::{DamageType, Health, Stat};
use crate::medical::organs::{self, Slot};

pub const STRANGE_REAGENT: &str = "/datum/reagent/medicine/strange_reagent";
/// `healing_per_reagent_unit`, `excess_healing_ratio`, `max_revive_damage_ratio`.
const HEALING_PER_UNIT: f32 = 5.0;
const EXCESS_HEALING_RATIO: f32 = 0.8;
const MAX_REVIVE_DAMAGE_RATIO: f32 = 2.0;
/// The `addtimer`s of `expose_mob`: jitter at 4 s, revival at 7 s, jitter at 8 s.
const JITTER_AGAIN: f64 = 4.0;
const REVIVE_AFTER: f64 = 7.0;
const JITTER_LAST: f64 = 8.0;

/// A revival on its way: the time `expose_mob` ran, the healing, the timers already run.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct StrangeRevival {
    pub start: f64,
    pub healing: f32,
    /// 0: none yet, 1: the 4 s jitter, 2: the revival, 3: done.
    pub step: u8,
}

pub(crate) fn build(app: &mut App) {
    app.add_systems(
        Update,
        timers
            .run_if(any_with_component::<StrangeRevival>)
            .before(crate::health::HealthSystems),
    );
}

/// `calculate_amount_needed_to_revive`.
pub fn amount_needed(health: &Health) -> f32 {
    let their_health = health.max - (health.brute + health.burn);
    if their_health > 0.0 {
        return 1.0;
    }
    (-their_health / HEALING_PER_UNIT * 10.0).round() / 10.0
}

/// The healing `volume` units give: the needed dose at full, the excess at 80%.
pub fn healing_of(health: &Health, volume: f32) -> f32 {
    let needed = amount_needed(health);
    needed * HEALING_PER_UNIT + (volume - needed) * HEALING_PER_UNIT * EXCESS_HEALING_RATIO
}

/// `expose_mob` with `INGEST` of `volume` units on `mob`.
pub fn expose(world: &mut World, mob: Entity, volume: f32) {
    if volume <= 0.0
        || world.get::<Stat>(mob) != Some(&Stat::Dead)
        || world.get::<StrangeRevival>(mob).is_some()
        || crate::species::def_of(world, mob).biotypes & crate::species::biotype::ORGANIC == 0
    {
        return;
    }
    let Some(health) = world.get::<Health>(mob).cloned() else {
        return;
    };
    let name = name_of(world, mob);
    let say = |world: &mut World, text: String| visible(world, mob, &text, &text, Span::Warning);
    if world.get::<crate::round::Suicided>(mob).is_some() {
        say(world, format!("Тело {name} не реагирует..."));
        return;
    }
    if world.get::<crate::human::Husk>(mob).is_some() {
        say(world, format!("Тело {name} испускает облачко дыма..."));
        return;
    }
    if health.brute + health.burn > health.max * MAX_REVIVE_DAMAGE_RATIO {
        say(
            world,
            format!("Тело {name} яростно содрогается и затихает..."),
        );
        return;
    }
    if volume < amount_needed(&health) {
        say(
            world,
            format!("Тело {name} слегка содрогается и снова затихает."),
        );
        crate::animation::do_jitter_animation(world, mob, 10.0);
        return;
    }
    say(world, format!("Тело {name} начинает биться в конвульсиях!"));
    // `notify_revival`: the ghost hears it.
    if let Some(ghost) = crate::mind::get_ghost(world, mob, false) {
        crate::tools::tell_mob(
            world,
            ghost,
            Span::BoldNotice,
            "Ваше тело оживляют странным реагентом! Вернитесь в тело, если хотите ожить.",
        );
    }
    crate::animation::do_jitter_animation(world, mob, 10.0);
    let start = world.resource::<Time>().elapsed_secs_f64();
    world.entity_mut(mob).insert(StrangeRevival {
        start,
        healing: healing_of(&health, volume),
        step: 0,
    });
}

/// The `addtimer`s of `expose_mob`.
fn timers(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let pending: Vec<(Entity, StrangeRevival)> = world
        .query::<(Entity, &StrangeRevival)>()
        .iter(world)
        .map(|(e, r)| (e, *r))
        .collect();
    for (mob, mut r) in pending {
        let elapsed = now - r.start;
        if r.step == 0 && elapsed >= JITTER_AGAIN {
            r.step = 1;
            crate::animation::do_jitter_animation(world, mob, 1.0);
        }
        if r.step == 1 && elapsed >= REVIVE_AFTER {
            r.step = 2;
            revive(world, mob, r.healing);
        }
        if r.step == 2 && elapsed >= JITTER_LAST {
            crate::animation::do_jitter_animation(world, mob, 1.0);
            world.entity_mut(mob).remove::<StrangeRevival>();
            continue;
        }
        world.entity_mut(mob).insert(r);
    }
}

/// `do_strange_reagent_revival(healing)`: half of it on brute, the rest on burn, then
/// `revive(excess_healing)` (oxygen and toxin) for a body that can be revived.
pub fn revive(world: &mut World, mob: Entity, mut healing: f32) {
    let Some(mut health) = world.get::<Health>(mob).cloned() else {
        return;
    };
    // `set_heartattack(FALSE)`.
    organs::set_heartattack(world, mob, false);
    if health.brute > 0.0 {
        let brute = (healing * 0.5).min(health.brute);
        health.adjust(DamageType::Brute, -brute);
        healing = (healing - brute).max(0.0);
    }
    if health.burn > 0.0 && healing > 0.0 {
        let burn = healing.min(health.burn);
        health.adjust(DamageType::Burn, -burn);
        healing = (healing - burn).max(0.0);
    }
    // `revive(excess_healing)`.
    if healing > 0.0 {
        health.adjust(DamageType::Oxy, -healing);
        health.adjust(DamageType::Tox, -healing);
    }
    world.entity_mut(mob).insert(health);
    // `grab_ghost(FALSE)`.
    crate::mind::grab_ghost(world, mob, false);
    // carbon `can_be_revived`: a brain (or no need of one), no husk.
    if world.get::<crate::human::Husk>(mob).is_some() || organs::lacks(world, mob, Slot::Brain) {
        return;
    }
    if super::defib::revive(world, mob) && healing > 0.0 {
        crate::medical::diseases::emote(world, mob, "gasp");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dose_follows_the_damage() {
        let mut health = Health::default();
        // 100 max health, 150 brute: 50 below zero -> 10 units.
        health.adjust(DamageType::Brute, 150.0);
        assert_eq!(amount_needed(&health), 10.0);
        // 15 units: 10 * 5 + 5 * 5 * 0.8 = 70.
        assert_eq!(healing_of(&health, 15.0), 70.0);
        // Health above zero: one unit is enough.
        let healthy = Health::default();
        assert_eq!(amount_needed(&healthy), 1.0);
    }
}
