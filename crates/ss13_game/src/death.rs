//! What follows a living mob's death (server), after tg `death.dm` (`timeofdeath`, BandaStation
//! `update_time_of_death`), carbon `updatehealth` (`carbon.dm`), `status_procs.dm`
//! (`become_husk`, `cure_husk`), the succumb verb (`living.dm`) and its alert
//! (`/atom/movable/screen/alert/succumb`):
//!
//! - [`TimeOfDeath`]: when the mob died (also into fake death, which keeps [`Stat::Dead`]); gone
//!   once it lives again. Stasis stops the clock (`/datum/status_effect/grouped/stasis`
//!   `update_time_of_death`). The health analyzer prints it (`crate::health_scan`).
//! - Burn husking: a dead carbon whose burn exceeds `maxHealth` plus its chest's `max_damage`
//!   becomes a [`Husk`] (`become_husk(BURN)`) unless its species is `TRAIT_UNHUSKABLE`. A husk is
//!   not defibrillated (`DEFIB_FAIL_HUSK`) nor revived or filtered by surgery; [`cure_husk`] takes
//!   the burn source away (an admin heal every source).
//! - Succumbing: in soft or hard crit (`CAN_SUCCUMB`) the mob has the succumb alert; its click asks
//!   for last words (`tgui_input_text`), whispers them through the say code (`say("#...")`) and,
//!   once they were said, dies of oxygen damage down to the death threshold. A whisper from hard
//!   crit is cut by the health left and makes the mob succumb itself (`ss13_chat`'s
//!   [`LastWords`], tg `succumbed` in `/mob/living/say`).
// shortcut: no `TRAIT_NODEATH`/`TRAIT_SUCCUMB_OVERRIDE` sources are ported (memento mori,
// penthrite, voidwalker), so every crit mob may succumb; synthflesh and rezadone do not unhusk
// (reagent `expose_mob` is not ported, `crate::chem`); no `persistent_client` time of death (no
// respawn timer).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_chat::server::{LastWords, SaySystems};
use ss13_net::server::IntentReceived;
use ss13_shared::chat::{MAX_MESSAGE_LEN, Span};
use ss13_shared::protocol::ClientIntent;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems, ask_world};
use ss13_tgui::server::{UiActed, UiSystems};

use crate::breath::{AlertCategory, AlertClick, Alerts, clicked_alert};
use crate::combat::CombatRng;
use crate::health::{
    DamageType, DeathComa, HEALTH_THRESHOLD_DEAD, Health, HealthSystems, Stat, apply_damage,
};
use crate::hud::AlertHudUi;
use crate::human::{HumanLook, Husk};
use crate::inventory::Inventory;
use crate::server::PlayerMob;
use crate::species::Trait;
use crate::tools::tell;
use crate::zone::BodyZone;

/// tg `timeofdeath`: `Time::elapsed_secs_f64` of the death, moved on while in stasis.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct TimeOfDeath(pub f64);

/// The `BURN` source of tg `TRAIT_HUSK`; `only`: the body was no husk before (a changeling's
/// drain is the other source, `crate::antag::changeling`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct BurnHusk {
    pub only: bool,
}

/// On a last-words prompt entity (`tgui_input_text` of the succumb alert).
#[derive(Component, Clone, Copy, Debug)]
struct SuccumbPrompt;

/// On a mob whose last words from the succumb alert went to the say code: it succumbs after
/// they were said. Armed by the first pass of [`succumb_after_words`], so the say was handled
/// whichever way the systems ran in the frame it was asked.
#[derive(Component, Clone, Copy, Debug, Default)]
struct SuccumbAfterWords {
    armed: bool,
}

/// `/atom/movable/screen/alert/succumb` `death_titles`.
const DEATH_TITLES: [&str; 8] = [
    "Покойной ночи, милый принц",
    "Игра окончена, чувак",
    "Конечная",
    "Живи долго и счастливо",
    "Увидимся, космический ковбой...",
    "Это было честью",
    "Занавес. Титры",
    "Всё хорошее когда-нибудь заканчивается",
];

pub struct DeathPlugin;

impl Plugin for DeathPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (track_death, burn_husk, stasis_clock).after(HealthSystems),
        )
        .add_systems(
            Update,
            (succumb_clicks, succumb_answers.after(PromptSystems::Answer))
                .in_set(UiSystems::Handle),
        )
        .add_systems(
            Update,
            (last_words_succumb, succumb_after_words)
                .chain()
                .after(SaySystems),
        );
    }
}

/// tg `CAN_SUCCUMB`: soft or hard crit.
pub fn can_succumb(stat: Stat) -> bool {
    matches!(stat, Stat::SoftCrit | Stat::HardCrit)
}

/// tg `updatehealth`: a dead carbon is husked by burns past `maxHealth` + the chest's
/// `max_damage`.
pub fn burns_husk(health: &Health) -> bool {
    health.basic.is_none() && health.max - health.burn < -BodyZone::Chest.max_damage()
}

/// The time of death and the succumb alert follow the stat (`death()`, `revive()`,
/// `update_succumb_action` of `set_stat`).
fn track_death(
    mut mobs: Query<(Entity, &Stat, Has<TimeOfDeath>, Option<&mut Alerts>), Changed<Stat>>,
    time: Res<Time>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    for (mob, stat, dated, alerts) in &mut mobs {
        let dead = *stat == Stat::Dead;
        if dead && !dated {
            commands.entity(mob).try_insert(TimeOfDeath(now));
        } else if !dead && dated {
            commands.entity(mob).try_remove::<TimeOfDeath>();
        }
        if let Some(mut alerts) = alerts {
            let shown = alerts.get(AlertCategory::Succumb).is_some();
            if can_succumb(*stat) && !shown {
                alerts.throw(AlertCategory::Succumb, "template", 0);
            } else if !can_succumb(*stat) && shown {
                alerts.clear(AlertCategory::Succumb);
            }
        }
    }
}

/// Stasis stops the clock of the dead (`update_time_of_death` each stasis tick).
fn stasis_clock(
    mut mobs: Query<&mut TimeOfDeath, With<crate::medical::stasis::InStasis>>,
    time: Res<Time>,
) {
    let delta = time.delta_secs_f64();
    for mut died in &mut mobs {
        died.0 += delta;
    }
}

/// carbon `updatehealth`: `become_husk(BURN)` of the badly burnt dead (not fake death).
fn burn_husk(
    mobs: Query<
        (Entity, &Health, &Stat),
        (
            Or<(Changed<Health>, Changed<Stat>)>,
            With<HumanLook>,
            Without<BurnHusk>,
            Without<DeathComa>,
        ),
    >,
    mut commands: Commands,
) {
    for (mob, health, stat) in &mobs {
        if *stat == Stat::Dead && burns_husk(health) {
            commands.queue(move |world: &mut World| {
                become_husk(world, mob);
            });
        }
    }
}

/// tg `become_husk(BURN)`: the [`Husk`] and its look; `false` for a `TRAIT_UNHUSKABLE` species.
pub fn become_husk(world: &mut World, mob: Entity) -> bool {
    if crate::species::has_trait(world, mob, Trait::Unhuskable) {
        return false;
    }
    let Ok(mut entity) = world.get_entity_mut(mob) else {
        return false;
    };
    if entity.contains::<BurnHusk>() {
        return true;
    }
    let only = !entity.contains::<Husk>();
    entity.insert((Husk, BurnHusk { only }));
    set_husk_look(world, mob, true);
    true
}

/// tg `cure_husk(BURN)` (`burn_only`) or `cure_husk()` (every source): `true` when the body is
/// no husk afterwards.
pub fn cure_husk(world: &mut World, mob: Entity, burn_only: bool) -> bool {
    let Ok(mut entity) = world.get_entity_mut(mob) else {
        return false;
    };
    let burn = entity.take::<BurnHusk>();
    if !entity.contains::<Husk>() {
        return true;
    }
    if burn_only && !burn.is_some_and(|b| b.only) {
        return false;
    }
    entity.remove::<Husk>();
    set_husk_look(world, mob, false);
    true
}

/// human `update_body_parts` of a (no longer) husked body.
fn set_husk_look(world: &mut World, mob: Entity, husk: bool) {
    if let Some(mut look) = world.get_mut::<HumanLook>(mob)
        && look.husk != husk
    {
        look.husk = husk;
    }
    // The appearance is rebuilt on an inventory change (`crate::inventory`).
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        inventory.set_changed();
    }
}

/// `/mob/living/verb/succumb`: oxygen damage down to the death threshold, then `death()`.
pub fn succumb(world: &mut World, mob: Entity, whispered: bool) {
    let client = world.get::<PlayerMob>(mob).map(|p| p.client);
    let (Some(mut health), Some(mut stat)) = (
        world.get::<Health>(mob).cloned(),
        world.get::<Stat>(mob).copied(),
    ) else {
        return;
    };
    if !can_succumb(stat) {
        if let Some(client) = client {
            tell(
                world,
                client,
                Span::Warning,
                "Вы не можете сдаться смерти! Эта жизнь продолжается.",
            );
        }
        return;
    }
    info!(
        "{mob} {} with {} health",
        if whispered {
            "whispered its final words"
        } else {
            "succumbed to death"
        },
        health.health()
    );
    let amount = health.health() - HEALTH_THRESHOLD_DEAD;
    apply_damage(&mut health, &mut stat, DamageType::Oxy, amount);
    // `death()` whatever the rounding left.
    stat = Stat::Dead;
    world.entity_mut(mob).insert((health, stat));
    if !whispered && let Some(client) = client {
        tell(world, client, Span::Notice, "Вы сдались и отдались смерти.");
    }
}

/// A click on the succumb alert asks for last words.
fn succumb_clicks(world: &mut World, mut acts: Local<MessageCursor<UiActed<AlertHudUi>>>) {
    let acts: Vec<(Entity, Entity, AlertClick)> = acts
        .read(world.resource::<Messages<UiActed<AlertHudUi>>>())
        .map(|a| (a.actor, a.source, a.act.clone()))
        .collect();
    for (actor, source, click) in acts {
        if actor != source || clicked_alert(world, actor, &click) != Some(AlertCategory::Succumb) {
            continue;
        }
        // Checked again with the answer (`succumb`).
        if !world.get::<Stat>(actor).is_some_and(|s| can_succumb(*s)) {
            continue;
        }
        let Some(client) = world.get::<PlayerMob>(actor).map(|p| p.client) else {
            continue;
        };
        let pick = world
            .get_resource_or_init::<CombatRng>()
            .range(0, DEATH_TITLES.len() as u32 - 1) as usize;
        let ask = Ask::new(
            client,
            actor,
            DEATH_TITLES[pick],
            "Что Вы хотите сказать напоследок?",
            PromptKind::Text {
                default: String::new(),
                max_length: MAX_MESSAGE_LEN as u32,
                multiline: false,
            },
        );
        let prompt = ask_world(world, ask);
        world.entity_mut(prompt).insert(SuccumbPrompt);
    }
}

/// The last words: cancelled does nothing, empty succumbs, anything else is whispered first.
fn succumb_answers(world: &mut World, mut answers: Local<MessageCursor<PromptAnswered>>) {
    let pending: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in pending {
        if world.get::<SuccumbPrompt>(prompt).is_none() {
            continue;
        }
        // `isnull(last_whisper)`.
        let Some(PromptAnswer::Text(words)) = answer else {
            continue;
        };
        // The player may have left the body or recovered while typing.
        if world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
            || !world.get::<Stat>(actor).is_some_and(|s| can_succumb(*s))
        {
            continue;
        }
        // `length(last_whisper)`: nothing to say succumbs at once.
        if ss13_chat::server::sanitize_say(&words).is_err() {
            succumb(world, actor, false);
            continue;
        }
        // `say("#[last_whisper]")`, then `succumb(whispered = TRUE)`.
        world.write_message(IntentReceived {
            client,
            intent: ClientIntent::Say(format!("#{words}")),
        });
        world.entity_mut(actor).insert(SuccumbAfterWords::default());
    }
}

/// tg `succumb(TRUE)` after a whisper from hard crit.
fn last_words_succumb(world: &mut World, mut said: Local<MessageCursor<LastWords>>) {
    let Some(messages) = world.get_resource::<Messages<LastWords>>() else {
        return;
    };
    let dying: Vec<Entity> = said.read(messages).map(|l| l.speaker).collect();
    for mob in dying {
        if world.get_entity(mob).is_ok() {
            succumb(world, mob, true);
        }
    }
}

/// The succumb alert's `succumb(whispered = TRUE)` once its words were said; a mob the words
/// already killed (hard crit) or that recovered meanwhile is left be.
fn succumb_after_words(world: &mut World) {
    let pending: Vec<(Entity, bool)> = world
        .query::<(Entity, &SuccumbAfterWords)>()
        .iter(world)
        .map(|(e, p)| (e, p.armed))
        .collect();
    for (mob, armed) in pending {
        if !armed {
            world
                .entity_mut(mob)
                .insert(SuccumbAfterWords { armed: true });
            continue;
        }
        world.entity_mut(mob).remove::<SuccumbAfterWords>();
        if world.get::<Stat>(mob).is_some_and(|s| can_succumb(*s)) {
            succumb(world, mob, true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    fn burnt(burn: f32) -> Health {
        let mut health = Health::default();
        for zone in BodyZone::ALL {
            health.damage_zone(zone, DamageType::Burn, burn);
        }
        health
    }

    #[test]
    fn burns_past_the_chest_husk_the_dead() {
        // Every part maxed: 2 * 200 + 4 * 50 = 600 burn, health 100 - 600.
        assert!(burns_husk(&burnt(200.0)));
        // 300 burn: 100 - 300 = -200 is not below -200.
        let mut health = Health::default();
        health.damage_zone(BodyZone::Chest, DamageType::Burn, 200.0);
        health.damage_zone(BodyZone::Head, DamageType::Burn, 100.0);
        assert!(!burns_husk(&health));
        health.damage_zone(BodyZone::Head, DamageType::Burn, 0.5);
        assert!(burns_husk(&health));
        assert!(!burns_husk(&Health::basic(100.0, [1.0; 5])));

        let mut world = World::new();
        let dead = world
            .spawn((burnt(200.0), Stat::Dead, HumanLook::random_assistant(1)))
            .id();
        let alive = world
            .spawn((
                burnt(200.0),
                Stat::Conscious,
                HumanLook::random_assistant(2),
            ))
            .id();
        let faking = world
            .spawn((
                burnt(200.0),
                Stat::Dead,
                DeathComa,
                HumanLook::random_assistant(3),
            ))
            .id();
        world.run_system_once(burn_husk).unwrap();
        assert!(world.get::<Husk>(dead).is_some());
        assert!(world.get::<HumanLook>(dead).unwrap().husk);
        assert!(world.get::<Husk>(alive).is_none());
        assert!(world.get::<Husk>(faking).is_none());
    }

    #[test]
    fn cure_takes_only_its_own_source() {
        let mut world = World::new();
        let burnt = world.spawn(HumanLook::random_assistant(1)).id();
        assert!(become_husk(&mut world, burnt));
        assert_eq!(world.get::<BurnHusk>(burnt), Some(&BurnHusk { only: true }));
        assert!(cure_husk(&mut world, burnt, true));
        assert!(world.get::<Husk>(burnt).is_none());
        assert!(!world.get::<HumanLook>(burnt).unwrap().husk);

        // A drained husk (changeling) stays one when its burns are cured.
        let mut look = HumanLook::random_assistant(2);
        look.husk = true;
        let drained = world.spawn((look, Husk)).id();
        assert!(become_husk(&mut world, drained));
        assert!(!cure_husk(&mut world, drained, true));
        assert!(world.get::<Husk>(drained).is_some());
        // An admin heal cures every source.
        assert!(cure_husk(&mut world, drained, false));
        assert!(world.get::<Husk>(drained).is_none());
        assert!(!world.get::<HumanLook>(drained).unwrap().husk);
    }

    #[test]
    fn succumbing_kills_only_in_crit() {
        let mut world = World::new();
        let mut health = Health::default();
        health.adjust(DamageType::Brute, 110.0);
        let crit = world.spawn((health, Stat::SoftCrit)).id();
        let fine = world.spawn((Health::default(), Stat::Conscious)).id();
        succumb(&mut world, crit, false);
        assert_eq!(world.get::<Stat>(crit), Some(&Stat::Dead));
        let health = world.get::<Health>(crit).unwrap();
        assert!(
            health.health() <= HEALTH_THRESHOLD_DEAD,
            "{}",
            health.health()
        );
        assert!((health.oxy - 90.0).abs() < 1e-3, "{}", health.oxy);
        succumb(&mut world, fine, false);
        assert_eq!(world.get::<Stat>(fine), Some(&Stat::Conscious));
        assert!(can_succumb(Stat::HardCrit) && !can_succumb(Stat::Dead));
    }

    #[test]
    fn time_of_death_follows_the_stat_and_stops_in_stasis() {
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        let mob = world.spawn((Stat::Dead, Alerts::default())).id();
        let crit = world.spawn((Stat::HardCrit, Alerts::default())).id();
        world.run_system_once(track_death).unwrap();
        assert_eq!(world.get::<TimeOfDeath>(mob), Some(&TimeOfDeath(0.0)));
        assert!(world.get::<Alerts>(mob).unwrap().0.is_empty());
        assert!(
            world
                .get::<Alerts>(crit)
                .unwrap()
                .get(AlertCategory::Succumb)
                .is_some()
        );
        // Revived and out of crit.
        world.entity_mut(mob).insert(Stat::Conscious);
        world.entity_mut(crit).insert(Stat::Conscious);
        world.run_system_once(track_death).unwrap();
        assert!(world.get::<TimeOfDeath>(mob).is_none());
        assert!(world.get::<Alerts>(crit).unwrap().0.is_empty());
    }
}
