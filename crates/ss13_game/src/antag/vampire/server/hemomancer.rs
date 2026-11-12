//! The hemomancer's blood magic (`vampire_powers/hemomancer_powers.dm`): blood tendrils,
//! predator senses and blood eruption.
// shortcut: no vampiric claws (an abstract two-handed weapon with its own attack), blood
// barrier (a structure passable to hemomancers), blood pool (ethereal jaunt) or the blood
// bringer's rite (its aura); they stay in the info panel until those systems exist.

use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};

use super::*;
use crate::effects::temp_visual::{self, Visual};
use crate::health::DamageType;
use crate::sound::{PlaySound, playsound};

const EFFECTS_DMI: &str = "modular_bandastation/vampire/icons/effects/vampire_effects.dmi";
/// `/obj/effect/temp_visual/blood_tendril` (the default `duration`) and its `long` kind.
const TENDRIL: Visual = Visual::new(EFFECTS_DMI, "blood_tendril", 1.0);
const TENDRIL_LONG: Visual = Visual::new(EFFECTS_DMI, "blood_tendril", 2.0);
/// `/obj/effect/temp_visual/blood_spike`.
const SPIKE: Visual = Visual::new(EFFECTS_DMI, "bloodspike_white", 0.3);
const ENTER_BLOOD_SOUND: &str = "sound/effects/magic/enter_blood.ogg";
const SPIKE_SOUND: &str = "modular_bandastation/vampire/sound/misc/demon_attack1.ogg";
const BLOOD_DECAL: &str = "/obj/effect/decal/cleanable/blood";
/// `area_of_affect` of the tendrils, the `apply_slowdown` timer.
const TENDRIL_RADIUS: i32 = 1;
const TENDRIL_DELAY_SECS: f64 = 0.5;
/// `/datum/status_effect/blood_tendril_slow`: duration, `multiplicative_slowdown`.
const TENDRIL_SLOW_SECS: f64 = 6.0;
const TENDRIL_SLOWDOWN: f32 = 0.5;
/// `aoe_radius` and damage of the eruption.
const ERUPTION_RADIUS: u32 = 4;
const ERUPTION_DAMAGE: f32 = 50.0;

/// The tendrils' `addtimer(apply_slowdown)`: due at `at` around `center`.
#[derive(Component, Clone, Copy, Debug)]
pub struct TendrilStrike {
    pub at: f64,
    pub center: TilePos,
    pub caster: Entity,
}

/// `/datum/status_effect/blood_tendril_slow` on a victim, until `until`.
#[derive(Component, Clone, Copy, Debug)]
pub struct TendrilSlow {
    pub until: f64,
}

/// An open predator senses prompt: the prey offered, by name.
#[derive(Component, Clone, Debug)]
struct PredatorPrompt {
    prey: Vec<(String, Entity)>,
}

pub(super) fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        (
            tendril_strikes.run_if(any_with_component::<TendrilStrike>),
            tendril_slows_end.run_if(any_with_component::<TendrilSlow>),
            predator_answers.after(ss13_tgui::prompt::PromptSystems::Answer),
        ),
    );
}

/// `vampire_blood_tendrils/cast`: tendrils on the open tiles around `center`, the slowdown half
/// a second later.
pub(super) fn blood_tendrils(world: &mut World, mob: Entity, center: TilePos) {
    playsound(world, PlaySound::new(center, ENTER_BLOOD_SOUND, 20.0, true));
    for dx in -TENDRIL_RADIUS..=TENDRIL_RADIUS {
        for dy in -TENDRIL_RADIUS..=TENDRIL_RADIUS {
            let pos = TilePos::new(center.x + dx, center.y + dy, center.z);
            let open = turf_at(world, pos)
                .and_then(|t| path_of(world, t))
                .is_some_and(|p| p.starts_with("/turf/open"));
            if open {
                temp_visual::spawn(world, pos, TENDRIL);
            }
        }
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    world.spawn(TendrilStrike {
        at: now + TENDRIL_DELAY_SECS,
        center,
        caster: mob,
    });
}

/// `apply_slowdown`: the living around the centre the vampire may affect are slowed for 6 s.
fn tendril_strikes(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let due: Vec<(Entity, TendrilStrike)> = world
        .query::<(Entity, &TendrilStrike)>()
        .iter(world)
        .filter(|(_, s)| s.at <= now)
        .map(|(e, s)| (e, *s))
        .collect();
    for (strike, TendrilStrike { center, caster, .. }) in due {
        world.despawn(strike);
        let victims: Vec<Entity> = world
            .resource::<SpatialIndex>()
            .in_radius(center, TENDRIL_RADIUS as u32)
            .filter(|&e| world.get::<Health>(e).is_some())
            .collect();
        for victim in victims {
            if !affects_vampire(world, victim, caster) {
                continue;
            }
            slow(world, victim, now + TENDRIL_SLOW_SECS);
            let text = format!(
                "{} опутан кровавыми щупальцами!",
                ss13_shared::capitalize(&name_of(world, victim))
            );
            visible(world, victim, &text, &text, Span::Warning);
            if let Some(&pos) = world.get::<TilePos>(victim) {
                temp_visual::spawn(world, pos, TENDRIL_LONG);
            }
        }
    }
}

/// `set_timed_status_effect(6 SECONDS, blood_tendril_slow)`: a running one is only prolonged.
pub fn slow(world: &mut World, victim: Entity, until: f64) {
    if let Some(mut slow) = world.get_mut::<TendrilSlow>(victim) {
        slow.until = slow.until.max(until);
        return;
    }
    let current = world.get::<MoveSpeedModifier>(victim).map_or(0.0, |m| m.0);
    world.entity_mut(victim).insert((
        TendrilSlow { until },
        MoveSpeedModifier(current + TENDRIL_SLOWDOWN),
    ));
}

/// `blood_tendril_slow/on_remove`.
fn tendril_slows_end(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let over: Vec<Entity> = world
        .query::<(Entity, &TendrilSlow)>()
        .iter(world)
        .filter(|(_, s)| s.until <= now)
        .map(|(e, _)| e)
        .collect();
    for victim in over {
        world.entity_mut(victim).remove::<TendrilSlow>();
        if let Some(mut modifier) = world.get_mut::<MoveSpeedModifier>(victim) {
            modifier.0 -= TENDRIL_SLOWDOWN;
        }
    }
}

/// `vampire_predator_senses/cast`: the living humans with a mind on the vampire's level, to
/// pick one from.
pub(super) fn predator_senses(world: &mut World, client: Entity, mob: Entity) {
    let Some(&at) = world.get::<TilePos>(mob) else {
        return;
    };
    let mut prey: Vec<(String, Entity)> = world
        .query_filtered::<(Entity, &TilePos, &Stat), (
            With<crate::human::HumanLook>,
            With<PlayerMob>,
        )>()
        .iter(world)
        .filter(|(e, pos, stat)| *e != mob && pos.z == at.z && **stat != Stat::Dead)
        .map(|(e, ..)| e)
        .collect::<Vec<_>>()
        .into_iter()
        .map(|e| (name_of(world, e), e))
        .collect();
    if prey.is_empty() {
        tell(world, client, Span::Warning, "Здесь нет добычи для охоты.");
        return;
    }
    prey.sort_by(|a, b| a.0.cmp(&b.0));
    // `prey[target.real_name] = target`: one entry per name.
    prey.dedup_by(|a, b| a.0 == b.0);
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Запах крови",
            "Кого выследить",
            PromptKind::List {
                items: prey.iter().map(|(name, _)| name.clone()).collect(),
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(PredatorPrompt { prey });
}

/// The prey picked: where it is, checked again after the wait.
fn predator_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.actor, a.answer.clone()))
        .collect();
    for (prompt, mob, answer) in answers {
        let Some(PredatorPrompt { prey }) = world.get::<PredatorPrompt>(prompt).cloned() else {
            continue;
        };
        let Some(PromptAnswer::Item(i)) = answer else {
            continue;
        };
        let Some((name, target)) = prey.get(i).cloned() else {
            continue;
        };
        report_prey(world, mob, &name, target);
    }
}

/// The hunt's result for the vampire `mob` (still one, alive) on `target`.
pub fn report_prey(world: &mut World, mob: Entity, name: &str, target: Entity) {
    if world.get::<Vampire>(mob).is_none() || world.get::<Stat>(mob) == Some(&Stat::Dead) {
        return;
    }
    let (Some(&from), Some(&to)) = (world.get::<TilePos>(mob), world.get::<TilePos>(target)) else {
        chat_to(world, mob, Span::Warning, "Запах крови этой цели угас.");
        return;
    };
    if from.z != to.z {
        chat_to(world, mob, Span::Warning, "Запах крови этой цели угас.");
        return;
    }
    let area = crate::bots::area_name(world, to);
    let dir = dir_to(from, to).map_or("", crate::mining::orm::dir_text);
    let mut message = format!("{name} находится в {area}, {dir} от вас.");
    let damaged = world
        .get::<Health>(target)
        .is_some_and(|h| h.max - h.health() >= 40.0);
    let lying = world.get::<BodyPosition>(target).is_some_and(|b| b.lying());
    let bleeding = world
        .get::<crate::medical::wounds::Limbs>(target)
        .is_some_and(|l| l.bleed_rate(lying) > 0.0);
    if damaged || bleeding {
        message.push_str(" Цель ранена.");
    }
    chat_to(world, mob, Span::Notice, message);
}

/// `vampire_blood_eruption`: a spike of living blood under everyone within 4 tiles the vampire
/// may affect, with a player, standing on blood.
pub(super) fn blood_eruption(world: &mut World, mob: Entity) {
    let Some(&at) = world.get::<TilePos>(mob) else {
        return;
    };
    let targets: Vec<(Entity, TilePos)> = world
        .resource::<SpatialIndex>()
        .in_radius(at, ERUPTION_RADIUS)
        .filter(|&e| world.get::<Health>(e).is_some() && world.get::<PlayerMob>(e).is_some())
        .filter_map(|e| Some((e, *world.get::<TilePos>(e)?)))
        .collect();
    for (target, pos) in targets {
        if !affects_vampire(world, target, mob) || !blood_under(world, pos) {
            continue;
        }
        temp_visual::spawn(world, pos, SPIKE);
        playsound(world, PlaySound::new(pos, SPIKE_SOUND, 50.0, true));
        if let (Some(mut health), Some(&stat)) = (
            world.get::<Health>(target).cloned(),
            world.get::<Stat>(target),
        ) {
            let mut new = stat;
            crate::health::apply_damage_zone(
                &mut health,
                &mut new,
                BodyZone::Chest,
                DamageType::Brute,
                ERUPTION_DAMAGE,
            );
            world.entity_mut(target).insert(health);
            if new != stat {
                world.entity_mut(target).insert(new);
            }
        }
        let text = format!(
            "{} пронзён шипом из живой крови!",
            ss13_shared::capitalize(&name_of(world, target))
        );
        visible(world, target, &text, &text, Span::Warning);
        info!("{mob} speared {target} with a blood spike (vampire)");
    }
}

/// `locate(/obj/effect/decal/cleanable/blood) in get_turf(target)`.
fn blood_under(world: &World, pos: TilePos) -> bool {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| path_of(world, e).is_some_and(|p| is_under(&p, BLOOD_DECAL)))
}
