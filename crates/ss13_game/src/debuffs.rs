//! Debuff status effects without a home of their own (server), after tg
//! `code/datums/status_effects/debuffs/jitteriness.dm` (its moodlet), `debuffs.dm`
//! (`/datum/status_effect/convulsing`), `drug_effects.dm` (`woozy`, `high_blood_pressure`,
//! `seizure`) and `tox_vomit.dm`.
//!
//! - Jitter ([`crate::status_effects::Timed::Jitter`]) gives the `jittery` moodlet (-2) while it
//!   lasts ([`Jittery`]).
//! - [`Convulsing`]: 15 s (a new one refreshes it) of shaky hands with their alert; every second
//!   40% to drop the active hand's item, with 10 s more jitter. The defibrillator's disarm
//!   (`crate::medical::defib`) gives it.
//! - [`seizure`]: a carbon drops for 1-3 s (paralysis) with 100 s of jitter and the seizure
//!   alert ([`Seizure`]).
//! - [`Woozy`]: every click cooldown (`next_move`) is 1.5 times as long (`nextmove_modifier`).
//! - [`HighBloodPressure`]: bleeding is 1.25 times as fast (`PHYS_COEFF_BLEED`,
//!   `crate::medical::bioware::bleed_mod`).
//! - [`ToxVomit`]: a human over 45 toxin damage with more than 20 nutrition throws up about
//!   every 150 s (a 30% a second count to 50 every 2 s), until the damage or the food drops.
// Woozy, high blood pressure and the seizure come from drug withdrawal (`crate::addictions`).
// shortcut: no `paranoid` personality (the jittery moodlet is always -2). The tox vomit
// starts on any change of a human's health above the threshold (tg: toxin damage taken), and
// it does not count in stasis.

use bevy::prelude::*;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use crate::breath::{AlertCategory, Alerts};
use crate::chem::{name_of, tell_mob, visible};
use crate::combat::{CombatRng, NextMove};
use crate::health::{Health, HealthSystems, Stat};
use crate::inventory::Inventory;
use crate::mood::MoodEvent;
use crate::status_effects::{self, Incap, StatusEffects, StatusSystems, Timed};

/// `/datum/status_effect/convulsing::duration`, its `tick_interval`, the drop chance.
pub const CONVULSING_SECS: f64 = 15.0;
const CONVULSING_TICK_SECS: f64 = 1.0;
const CONVULSING_DROP_PERCENT: f32 = 40.0;
/// `/datum/status_effect/woozy/nextmove_modifier`.
pub const WOOZY_NEXTMOVE_MODIFIER: f64 = 1.5;
/// `/datum/status_effect/high_blood_pressure`: `MODIFY_PHYSIOLOGY(PHYS_COEFF_BLEED, 1.25)`.
pub const HIGH_BLOOD_PRESSURE_BLEED: f32 = 1.25;
/// `AT_TOXIN_VOMIT_THRESHOLD`: toxin damage and nutrition above these.
pub const TOX_VOMIT_TOX: f32 = 45.0;
pub const TOX_VOMIT_NUTRITION: f32 = 20.0;
/// `tox_vomit`: `tick_interval`, `SPT_PROB(30)` a second, the count that makes one vomit,
/// `lost_nutrition`.
const TOX_VOMIT_TICK_SECS: f64 = 2.0;
const TOX_VOMIT_PERCENT: f32 = 30.0;
const TOX_VOMIT_COUNT: u32 = 50;
const TOX_VOMIT_NUTRITION_LOST: f32 = 20.0;

/// `/datum/mood_event/jittery` (BandaStation's text).
const JITTERY: MoodEvent = MoodEvent {
    id: "jittery",
    description: "Я на нервах и на стрёме и не могу стоять на месте!!",
    mood_change: -2.0,
    timeout: 0.0,
    hidden: false,
};

/// The jitter's moodlet is given.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Jittery;

/// `/datum/status_effect/convulsing`: until this game time, ticking at `next_tick`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Convulsing {
    pub until: f64,
    next_tick: f64,
}

/// `/datum/status_effect/seizure` until this game time.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Seizure {
    pub until: f64,
}

/// `/datum/status_effect/woozy`.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Woozy;

/// `/datum/status_effect/high_blood_pressure`.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct HighBloodPressure;

/// `/datum/status_effect/tox_vomit`: `puke_counter` and the next tick.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ToxVomit {
    pub puke_counter: u32,
    next_tick: f64,
}

pub struct DebuffsPlugin;

impl Plugin for DebuffsPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(woozy_next_move).add_systems(
            Update,
            (
                jitter_mood,
                convulsing_tick,
                seizure_upkeep,
                tox_vomit_start,
                tox_vomit_tick,
            )
                .chain()
                .after(StatusSystems)
                .before(HealthSystems),
        );
    }
}

use crate::tools::now;

fn throw_alert(world: &mut World, mob: Entity, category: AlertCategory, state: &'static str) {
    if let Some(mut alerts) = world.get_mut::<Alerts>(mob) {
        alerts.throw(category, state, 0);
    }
}

fn clear_alert(world: &mut World, mob: Entity, category: AlertCategory) {
    if let Some(mut alerts) = world.get_mut::<Alerts>(mob)
        && alerts.get(category).is_some()
    {
        alerts.clear(category);
    }
}

/// `jitter/on_apply` and `on_remove`: the moodlet follows the jitter.
fn jitter_mood(world: &mut World) {
    let mut started = world.query_filtered::<(Entity, &StatusEffects), Without<Jittery>>();
    let started: Vec<Entity> = started
        .iter(world)
        .filter(|(_, e)| e.jitter > 0.0)
        .map(|(e, _)| e)
        .collect();
    let mut ended = world.query_filtered::<(Entity, Option<&StatusEffects>), With<Jittery>>();
    let ended: Vec<Entity> = ended
        .iter(world)
        .filter(|(_, e)| e.is_none_or(|e| e.jitter <= 0.0))
        .map(|(e, _)| e)
        .collect();
    for mob in started {
        world.entity_mut(mob).insert(Jittery);
        crate::mood::add_event(world, mob, "jitter", &JITTERY);
    }
    for mob in ended {
        world.entity_mut(mob).remove::<Jittery>();
        crate::mood::clear_event(world, mob, "jitter");
    }
}

/// tg `apply_status_effect(/datum/status_effect/convulsing)` on a living mob: a new one tells
/// the mob and throws its alert, an existing one starts its 15 s over (`STATUS_EFFECT_REFRESH`).
pub fn apply_convulsing(world: &mut World, mob: Entity) {
    if world.get::<Stat>(mob).is_none() {
        return;
    }
    let now = now(world);
    if let Some(mut c) = world.get_mut::<Convulsing>(mob) {
        c.until = now + CONVULSING_SECS;
        return;
    }
    world.entity_mut(mob).insert(Convulsing {
        until: now + CONVULSING_SECS,
        next_tick: now + CONVULSING_TICK_SECS,
    });
    throw_alert(world, mob, AlertCategory::Convulsing, "convulsing");
    tell_mob(
        world,
        mob,
        Span::BoldWarning,
        "Вы чувствуете, как по телу проходит разряд! Ваши руки начинают трястись!",
    );
}

/// `convulsing/tick` every second; the effect ends with its time. Cost: per tick, one pass
/// over convulsing mobs only.
fn convulsing_tick(world: &mut World) {
    let now = now(world);
    let mobs: Vec<(Entity, Convulsing)> = world
        .query::<(Entity, &Convulsing)>()
        .iter(world)
        .map(|(e, c)| (e, *c))
        .collect();
    for (mob, c) in mobs {
        if now >= c.until {
            world.entity_mut(mob).remove::<Convulsing>();
            clear_alert(world, mob, AlertCategory::Convulsing);
            continue;
        }
        if now < c.next_tick {
            continue;
        }
        if let Some(mut c) = world.get_mut::<Convulsing>(mob) {
            c.next_tick += CONVULSING_TICK_SECS;
        }
        if world
            .get_resource_or_init::<CombatRng>()
            .chance(CONVULSING_DROP_PERCENT)
        {
            convulse_drop(world, mob);
        }
    }
}

/// `dropItemToGround(get_active_held_item())` with its messages and jitter.
fn convulse_drop(world: &mut World, mob: Entity) {
    let (Some(item), Some(&pos)) = (
        world.get::<Inventory>(mob).and_then(Inventory::active_item),
        world.get::<TilePos>(mob),
    ) else {
        return;
    };
    if crate::inventory::no_drop(world, item) {
        return;
    }
    crate::inventory::put_down(world, item, pos);
    let (name, thing) = (name_of(world, mob), name_of(world, item));
    visible(
        world,
        mob,
        "Ваша рука сильно трясётся, и вы роняете то, что держали!",
        &format!("Рука {name} трясётся, и он роняет {thing}!"),
        Span::Notice,
    );
    status_effects::adjust_timed(world, mob, Timed::Jitter, 10.0, None);
}

/// tg `apply_status_effect(/datum/status_effect/seizure)`: carbons only, one at a time; 1-3 s
/// of paralysis, 100 s of jitter. True when it started.
pub fn seizure(world: &mut World, mob: Entity) -> bool {
    if world.get::<crate::chem::MobReagents>(mob).is_none()
        || world.get::<Seizure>(mob).is_some()
        || world.get::<Stat>(mob).is_none()
    {
        return false;
    }
    let (secs, line) = {
        let mut rng = world.get_resource_or_init::<CombatRng>();
        // `rand(1 SECONDS, 3 SECONDS)`: whole deciseconds.
        let secs = rng.range(10, 30) as f32 / 10.0;
        const LINES: [&str; 4] = [
            "Вы не можете собраться с мыслями...",
            "У вас внезапно сильно кружится голова...",
            "Вы не можете ясно мыслить...",
            "Вы больше не можете нормально двигать лицом...",
        ];
        (secs, LINES[rng.range(0, 3) as usize])
    };
    let until = now(world) + f64::from(secs);
    world.entity_mut(mob).insert(Seizure { until });
    throw_alert(world, mob, AlertCategory::Seizure, "paralysis");
    status_effects::set_timed(world, mob, Timed::Jitter, 100.0, true);
    status_effects::apply(world, mob, Incap::Paralyze, secs);
    let name = name_of(world, mob);
    visible(
        world,
        mob,
        line,
        &format!("{name} падает на землю в судорогах."),
        Span::Warning,
    );
    true
}

fn seizure_upkeep(world: &mut World) {
    let now = now(world);
    let ended: Vec<Entity> = world
        .query::<(Entity, &Seizure)>()
        .iter(world)
        .filter(|(_, s)| now >= s.until)
        .map(|(e, _)| e)
        .collect();
    for mob in ended {
        world.entity_mut(mob).remove::<Seizure>();
        clear_alert(world, mob, AlertCategory::Seizure);
    }
}

/// tg `apply_status_effect`/`remove_status_effect(/datum/status_effect/woozy)`.
pub fn set_woozy(world: &mut World, mob: Entity, on: bool) {
    if on && world.get::<Stat>(mob).is_some() {
        world.entity_mut(mob).insert(Woozy);
        throw_alert(world, mob, AlertCategory::Woozy, "woozy");
    } else if world.get::<Woozy>(mob).is_some() {
        world.entity_mut(mob).remove::<Woozy>();
        clear_alert(world, mob, AlertCategory::Woozy);
    }
}

/// `changeNext_move`: `next_move_modifier` stretches every new cooldown of a woozy mob.
fn woozy_next_move(
    insert: On<Insert<NextMove>>,
    time: Res<Time>,
    mut mobs: Query<&mut NextMove, With<Woozy>>,
) {
    let Ok(mut next) = mobs.get_mut(insert.entity) else {
        return;
    };
    let now = time.elapsed_secs_f64();
    if next.0 > now {
        next.0 = now + (next.0 - now) * WOOZY_NEXTMOVE_MODIFIER;
    }
}

/// tg `apply_status_effect`/`remove_status_effect(/datum/status_effect/high_blood_pressure)`.
pub fn set_high_blood_pressure(world: &mut World, mob: Entity, on: bool) {
    if on && world.get::<Stat>(mob).is_some() {
        world.entity_mut(mob).insert(HighBloodPressure);
        throw_alert(
            world,
            mob,
            AlertCategory::HighBloodPressure,
            "highbloodpressure",
        );
    } else if world.get::<HighBloodPressure>(mob).is_some() {
        world.entity_mut(mob).remove::<HighBloodPressure>();
        clear_alert(world, mob, AlertCategory::HighBloodPressure);
    }
}

/// The high blood pressure's share of `PHYS_COEFF_BLEED` (1 without it).
pub fn bleed_mod(world: &World, mob: Entity) -> f32 {
    if world.get::<HighBloodPressure>(mob).is_some() {
        HIGH_BLOOD_PRESSURE_BLEED
    } else {
        1.0
    }
}

/// `AT_TOXIN_VOMIT_THRESHOLD`.
fn at_tox_vomit_threshold(health: &Health, hunger: &crate::hunger::Hunger) -> bool {
    health.tox > TOX_VOMIT_TOX && hunger.nutrition > TOX_VOMIT_NUTRITION
}

/// `adjust_tox_loss` on a human over the threshold applies the tox vomit
/// (`STATUS_EFFECT_UNIQUE`). Cost: per tick, the humans whose health changed.
fn tox_vomit_start(
    time: Res<Time>,
    mobs: Query<(Entity, &Health, &crate::hunger::Hunger), (Changed<Health>, Without<ToxVomit>)>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    for (mob, health, hunger) in &mobs {
        if at_tox_vomit_threshold(health, hunger) {
            commands.entity(mob).insert(ToxVomit {
                puke_counter: 0,
                next_tick: now + TOX_VOMIT_TICK_SECS,
            });
        }
    }
}

/// `tox_vomit/tick` every 2 s. Cost: per tick, one pass over the mobs with it.
fn tox_vomit_tick(world: &mut World) {
    let now = now(world);
    let due: Vec<Entity> = world
        .query::<(Entity, &ToxVomit)>()
        .iter(world)
        .filter(|(_, t)| now >= t.next_tick)
        .map(|(e, _)| e)
        .collect();
    for mob in due {
        let over = match (
            world.get::<Health>(mob),
            world.get::<crate::hunger::Hunger>(mob),
        ) {
            (Some(health), Some(hunger)) => at_tox_vomit_threshold(health, hunger),
            _ => false,
        };
        if !over {
            world.entity_mut(mob).remove::<ToxVomit>();
            continue;
        }
        if let Some(mut t) = world.get_mut::<ToxVomit>(mob) {
            t.next_tick += TOX_VOMIT_TICK_SECS;
        }
        if world.get::<Stat>(mob) == Some(&Stat::Dead)
            || world.get::<crate::medical::stasis::InStasis>(mob).is_some()
        {
            continue;
        }
        let counted = {
            let mut rng = world.get_resource_or_init::<CombatRng>();
            crate::medical::wounds::spt_prob(
                &mut rng,
                TOX_VOMIT_PERCENT,
                TOX_VOMIT_TICK_SECS as f32,
            )
        };
        let Some(mut t) = world.get_mut::<ToxVomit>(mob) else {
            continue;
        };
        t.puke_counter += u32::from(counted);
        if t.puke_counter < TOX_VOMIT_COUNT {
            continue;
        }
        t.puke_counter = 0;
        // `VOMIT_CATEGORY_DEFAULT`: the message, the harm and the stun.
        crate::medical::diseases::vomit(world, mob, TOX_VOMIT_NUTRITION_LOST, true, false, 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn world() -> World {
        let mut world = World::new();
        world.init_resource::<Time>();
        world.insert_resource(CombatRng(3));
        world
    }

    fn advance(world: &mut World, secs: f32) {
        world
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(secs));
    }

    #[test]
    fn convulsing_lasts_15_seconds_refreshes_and_clears_its_alert() {
        let mut world = world();
        let mob = world.spawn((Stat::Conscious, Alerts::default())).id();
        apply_convulsing(&mut world, mob);
        let alert = world
            .get::<Alerts>(mob)
            .unwrap()
            .get(AlertCategory::Convulsing);
        assert_eq!(alert.unwrap().state, "convulsing");
        advance(&mut world, 10.0);
        apply_convulsing(&mut world, mob);
        assert_eq!(world.get::<Convulsing>(mob).unwrap().until, 25.0);
        advance(&mut world, 15.0);
        convulsing_tick(&mut world);
        assert!(world.get::<Convulsing>(mob).is_none());
        assert!(
            world
                .get::<Alerts>(mob)
                .unwrap()
                .get(AlertCategory::Convulsing)
                .is_none()
        );
    }

    #[test]
    fn seizure_paralyzes_one_to_three_seconds_and_jitters() {
        let mut world = world();
        let mob = world
            .spawn((
                Stat::Conscious,
                Alerts::default(),
                crate::chem::MobReagents {
                    blood: crate::chem::mob_holder(),
                    stomach: crate::chem::mob_holder(),
                    drunk: 0.0,
                },
            ))
            .id();
        assert!(seizure(&mut world, mob));
        assert!(!seizure(&mut world, mob), "one at a time");
        let effects = world.get::<StatusEffects>(mob).unwrap();
        assert!(
            (1.0..=3.0).contains(&effects.paralyze),
            "{}",
            effects.paralyze
        );
        assert_eq!(effects.jitter, 100.0);
        let until = world.get::<Seizure>(mob).unwrap().until;
        assert_eq!(until, f64::from(effects.paralyze));
        advance(&mut world, 3.0);
        seizure_upkeep(&mut world);
        assert!(world.get::<Seizure>(mob).is_none());
        let animal = world.spawn(Stat::Conscious).id();
        assert!(!seizure(&mut world, animal), "carbons only");
    }

    #[test]
    fn woozy_stretches_click_cooldowns_and_blood_pressure_the_bleeding() {
        let mut world = world();
        world.add_observer(woozy_next_move);
        advance(&mut world, 10.0);
        let mob = world.spawn((Stat::Conscious, Alerts::default())).id();
        world.entity_mut(mob).insert(NextMove(10.8));
        assert_eq!(world.get::<NextMove>(mob).unwrap().0, 10.8);
        set_woozy(&mut world, mob, true);
        world.entity_mut(mob).insert(NextMove(10.8));
        assert!((world.get::<NextMove>(mob).unwrap().0 - 11.2).abs() < 1e-9);
        set_woozy(&mut world, mob, false);
        assert!(
            world
                .get::<Alerts>(mob)
                .unwrap()
                .get(AlertCategory::Woozy)
                .is_none()
        );
        assert_eq!(bleed_mod(&world, mob), 1.0);
        set_high_blood_pressure(&mut world, mob, true);
        assert_eq!(bleed_mod(&world, mob), 1.25);
        set_high_blood_pressure(&mut world, mob, false);
        assert_eq!(bleed_mod(&world, mob), 1.0);
    }

    #[test]
    fn tox_vomit_needs_both_toxins_and_food() {
        let hungry = crate::hunger::Hunger::new(10.0);
        let fed = crate::hunger::Hunger::new(300.0);
        let mut health = Health {
            tox: 46.0,
            ..Health::default()
        };
        assert!(at_tox_vomit_threshold(&health, &fed));
        assert!(!at_tox_vomit_threshold(&health, &hungry));
        health.tox = 45.0;
        assert!(!at_tox_vomit_threshold(&health, &fed));
    }

    #[test]
    fn jitter_gives_the_jittery_moodlet_while_it_lasts() {
        let mut world = world();
        let mob = world.spawn(Stat::Conscious).id();
        status_effects::set_timed(&mut world, mob, Timed::Jitter, 10.0, false);
        jitter_mood(&mut world);
        assert!(world.get::<Jittery>(mob).is_some());
        world.entity_mut(mob).remove::<StatusEffects>();
        jitter_mood(&mut world);
        assert!(world.get::<Jittery>(mob).is_none());
    }
}
