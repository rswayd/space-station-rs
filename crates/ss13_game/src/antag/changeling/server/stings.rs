//! Stings (`powers/tiny_prick.dm`): a sting button chooses or puts away the sting
//! (`chosen_sting`); a middle or alt click on a living mob within two tiles
//! ([`crate::click::StingClick`]) tries it (`try_to_sting`).
//!
//! - `can_sting`: the power's use checks, a carbon target (a human body here), the changeling on
//!   a tile, a path of at most [`crate::click::STING_RANGE`] steps; a changeling target feels the
//!   prick and the chemicals go a first time (tg's double charge).
//! - Transformation (33): the target becomes the chosen profile for 8 minutes, the timer paused
//!   while it is dead ([`TransStung`]); not twice, not a husk.
//! - False armblade (20): the target's held item drops, a dull blade grows in its hand for 1 min.
//! - Extract DNA (25): the target's profile, as absorbing would check it.
//! - Mute (20): 1 min of silence. Blind (25): eyes 80% damaged, 40 s blind, 80 s blurred; no or
//!   robotic eyes fail. Hallucination (10): 180 s of hallucinations 30–60 s later. Cryogenic
//!   (15): 30 u of frost oil.
// shortcut: the chosen sting shows on its button (`active`), not as tg's HUD sting icon; the path
// check is cardinal steps with up to four steps inside the two-tile range (tg pathfinds with
// diagonals); only human bodies are carbons here (no monkeys or xenos), so the transformation
// sting is never permanent (`TRAIT_LESSER_HUMANOID`).

use bevy::ecs::message::MessageCursor;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind};

use super::*;
use crate::click::{STING_RANGE, StingClick};

/// `sting_duration` of the transformation sting.
pub const TRANS_STING_SECS: f64 = 8.0 * 60.0;
/// `remove_fake` after `1 MINUTES`.
pub const FALSE_BLADE_SECS: f64 = 60.0;
const BLOB_ATTACK: &str = "sound/effects/blob/blobattack.ogg";

/// `/datum/status_effect/temporary_transformation/trans_sting` on the stung mob: the time left
/// and what it was.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct TransStung {
    pub remaining: f64,
    pub original: Profile,
}

/// A sting's delayed effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StingTimer {
    /// `false_armblade/remove_fake` of this blade.
    RemoveBlade(Entity),
    /// `lsd/hallucination_time`.
    Hallucinate,
}

/// The stings' `addtimer`s: when, on whom, what.
#[derive(Resource, Default)]
pub struct StingTimers(pub Vec<(f64, Entity, StingTimer)>);

/// The transformation sting's `select_dna` question, on its prompt entity.
#[derive(Component, Clone, Debug)]
struct StingDnaPrompt {
    mob: Entity,
    names: Vec<String>,
}

/// `sting/Trigger`: `set_sting` or `unset_sting`.
pub(super) fn toggle_sting(world: &mut World, client: Entity, mob: Entity, power: Power) {
    let Some(ling) = world.get::<Changeling>(mob) else {
        return;
    };
    if ling.chosen_sting.is_some() {
        unset_sting(world, mob);
        return;
    }
    if power == Power::TransformationSting {
        // `transformation/set_sting`: `select_dna()` first.
        let names: Vec<String> = ling.profiles.iter().map(|p| p.name.clone()).collect();
        if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
            ling.sting_dna = None;
        }
        let prompt = ss13_tgui::prompt::ask_world(
            world,
            Ask::new(
                client,
                mob,
                "Трансформация",
                "Выберите облик.",
                PromptKind::List {
                    items: names.clone(),
                    default: None,
                },
            ),
        );
        world
            .entity_mut(prompt)
            .insert(StingDnaPrompt { mob, names });
        return;
    }
    set_sting(world, mob, power);
}

fn set_sting(world: &mut World, mob: Entity, power: Power) {
    if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
        ling.chosen_sting = Some(power);
    }
    world.entity_mut(mob).insert(crate::click::StingReady);
    chat_to(
        world,
        mob,
        Span::Notice,
        "Готовим жало. Используйте Alt+Клик или СКМ по цели, чтобы ужалить ее.",
    );
    refresh(world, mob);
}

/// `unset_sting`.
pub fn unset_sting(world: &mut World, mob: Entity) {
    if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
        ling.chosen_sting = None;
    }
    if let Ok(mut e) = world.get_entity_mut(mob) {
        e.remove::<crate::click::StingReady>();
    }
    chat_to(
        world,
        mob,
        Span::Warning,
        "Мы убираем свое жало, пока что мы не можем никого ужалить.",
    );
    refresh(world, mob);
}

/// The profile picked for the transformation sting: checked again (another sting chosen or
/// another pick meanwhile), then chosen.
pub(super) fn sting_dna_picked(
    world: &mut World,
    mut answers: Local<MessageCursor<PromptAnswered>>,
) {
    let pending: Vec<(Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.answer.clone()))
        .collect();
    for (prompt, answer) in pending {
        let Some(asked) = world.get::<StingDnaPrompt>(prompt).cloned() else {
            continue;
        };
        let Some(PromptAnswer::Item(i)) = answer else {
            continue;
        };
        let Some(name) = asked.names.get(i) else {
            continue;
        };
        let Some(ling) = world.get::<Changeling>(asked.mob) else {
            continue;
        };
        if ling.fallen
            || ling.chosen_sting.is_some()
            || ling.sting_dna.is_some()
            || !ling.has(Power::TransformationSting)
        {
            continue;
        }
        let Some(profile) = ling.profiles.iter().find(|p| &p.name == name).cloned() else {
            continue;
        };
        if let Some(mut ling) = world.get_mut::<Changeling>(asked.mob) {
            ling.sting_dna = Some(profile);
        }
        set_sting(world, asked.mob, Power::TransformationSting);
    }
}

pub(super) fn sting_clicks(world: &mut World, mut clicks: Local<MessageCursor<StingClick>>) {
    let clicks: Vec<StingClick> = clicks
        .read(world.resource::<Messages<StingClick>>())
        .copied()
        .collect();
    for click in clicks {
        if world.get::<PlayerMob>(click.mob).map(|p| p.client) != Some(click.client) {
            continue;
        }
        let Some(power) = world
            .get::<Changeling>(click.mob)
            .filter(|c| !c.fallen)
            .and_then(|c| c.chosen_sting)
        else {
            // Readapted or fallen meanwhile: the clicks are its own again.
            world
                .entity_mut(click.mob)
                .remove::<crate::click::StingReady>();
            continue;
        };
        try_sting(world, click.client, click.mob, click.target, power);
    }
}

/// `get_path_to(user, target, max_distance = sting_range)`: a short walkable path.
fn reachable(world: &World, mob: Entity, target: Entity) -> bool {
    let (Some(&from), Some(&to)) = (world.get::<TilePos>(mob), world.get::<TilePos>(target)) else {
        return false;
    };
    if from.chebyshev(to).is_none_or(|d| d > STING_RANGE) {
        return false;
    }
    let index = world.resource::<ss13_world::SpatialIndex>();
    let pathing = crate::mobs::ai::movement::Pathing {
        max_path: 2 * STING_RANGE as usize,
        max_nodes: 64,
        ..crate::mobs::ai::movement::Pathing::BASIC
    };
    crate::mobs::ai::movement::find_path(index, mob, from, to, 0, &pathing).is_some()
}

/// `try_to_sting` of a sting: `can_sting`, `sting_action`, then `sting_feedback` and the cost.
pub fn try_sting(
    world: &mut World,
    client: Entity,
    mob: Entity,
    target: Entity,
    power: Power,
) -> bool {
    if target == mob || !checked(world, client, mob, power) {
        return false;
    }
    // `sting/can_sting`.
    if world.get::<HumanLook>(target).is_none()
        || world.get::<crate::closets::InCloset>(mob).is_some()
        || !reachable(world, mob, target)
    {
        return false;
    }
    if world.get::<Changeling>(target).is_some() {
        sting_feedback(world, mob, target);
        if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
            ling.chem_charges -= power.def().chemical_cost as f32;
            ling.chem_charges = ling.chem_charges.max(0.0);
        }
    }
    let done = match power {
        Power::TransformationSting => transformation_sting(world, client, mob, target),
        Power::FalseArmbladeSting => false_armblade_sting(world, client, mob, target),
        Power::ExtractDnaSting => extract_dna_sting(world, mob, target),
        Power::MuteSting => {
            crate::status_effects::adjust_timed(
                world,
                target,
                crate::status_effects::Timed::Silence,
                60.0,
                None,
            );
            true
        }
        Power::BlindSting => blind_sting(world, client, target),
        Power::LsdSting => {
            let delay = f64::from(
                world
                    .get_resource_mut::<crate::combat::CombatRng>()
                    .map_or(30, |mut r| r.range(30, 60)),
            );
            let at = world.resource::<Time>().elapsed_secs_f64() + delay;
            world
                .resource_mut::<StingTimers>()
                .0
                .push((at, target, StingTimer::Hallucinate));
            true
        }
        Power::CryoSting => {
            add_reagent(world, target, FROSTOIL, 30.0);
            true
        }
        _ => false,
    };
    if done {
        info!("{mob} stung {target} with {power:?} (changeling)");
        sting_feedback(world, mob, target);
        spend(world, mob, power);
    }
    done
}

/// `sting/sting_feedback`.
fn sting_feedback(world: &mut World, mob: Entity, target: Entity) {
    let victim = name_of(world, target);
    chat_to(
        world,
        mob,
        Span::Notice,
        format!("Мы незаметно жалим {victim}."),
    );
    if world.get::<Changeling>(target).is_some() {
        chat_to(
            world,
            target,
            Span::Warning,
            "Вы чувствуете небольшой укол.",
        );
    }
}

fn add_reagent(world: &mut World, mob: Entity, path: &str, amount: f32) {
    if let (Some(chem), Some(mut reagents)) = (
        world
            .get_resource::<crate::chem::Chem>()
            .map(|c| c.0.clone()),
        world.get::<crate::chem::MobReagents>(mob).cloned(),
    ) && let Some(id) = chem.id(path)
    {
        reagents
            .blood
            .add_reagent(&chem, id, amount, 310.15, None, None, false, false);
        world.entity_mut(mob).insert(reagents);
    }
}

/// `transformation/can_sting` + `sting_action`.
fn transformation_sting(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    let Some(profile) = world
        .get::<Changeling>(mob)
        .and_then(|c| c.sting_dna.clone())
    else {
        return false;
    };
    if world.get::<Husk>(target).is_some() {
        tell(world, client, Span::Warning, "несовместимое ДНК!");
        return false;
    }
    if world.get::<TransStung>(target).is_some() {
        tell(world, client, Span::Warning, "уже трансформирован!");
        return false;
    }
    let (name, look) = dna_of(world, target);
    let victim = name_of(world, target);
    transform(world, target, &profile);
    world.entity_mut(target).insert(TransStung {
        remaining: TRANS_STING_SECS,
        original: Profile {
            name,
            look,
            protected: false,
        },
    });
    chat_to(
        world,
        mob,
        Span::Notice,
        format!("Мы трансформируем {victim} в {}.", profile.name),
    );
    true
}

/// `false_armblade/sting_action`.
fn false_armblade_sting(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<Husk>(target).is_some() {
        tell(world, client, Span::Warning, "несовместимое ДНК!");
        return false;
    }
    let (Some(inventory), Some(&pos)) = (
        world.get::<Inventory>(target).cloned(),
        world.get::<TilePos>(target),
    ) else {
        return false;
    };
    if let Some(held) = inventory.active_item() {
        if no_drop(world, held) {
            let item = name_of(world, held);
            chat_to(
                world,
                mob,
                Span::Warning,
                format!(
                    "У цели нельзя снять {item}, вы не можете наложить на цель ложный армблейд!"
                ),
            );
            return false;
        }
        world.entity_mut(held).remove::<CarriedBy>().insert(pos);
    }
    let Some(blade) = grow(world, target, FALSE_ARM_BLADE) else {
        return false;
    };
    if let Some(mut inventory) = world.get_mut::<Inventory>(target) {
        let hand = inventory.active;
        inventory.hands[hand] = Some(blade);
    }
    let victim = name_of(world, target);
    visible(
        world,
        target,
        "Ваша рука скручивается и мутирует, превращаясь в ужасающее чудовище!",
        &format!("Страшный клинок образуется вокруг руки {victim}!"),
        Span::Warning,
    );
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, BLOB_ATTACK, 30.0, true),
    );
    let at = world.resource::<Time>().elapsed_secs_f64() + FALSE_BLADE_SECS;
    world
        .resource_mut::<StingTimers>()
        .0
        .push((at, target, StingTimer::RemoveBlade(blade)));
    true
}

/// `extract_dna/can_sting` (`can_absorb_dna`) + `sting_action`.
fn extract_dna_sting(world: &mut World, mob: Entity, target: Entity) -> bool {
    if !can_absorb(world, mob, target, true) {
        return false;
    }
    let (name, look) = dna_of(world, target);
    if let Some(mut ling) = world.get_mut::<Changeling>(mob)
        && !ling.has_profile(&name, look.as_ref())
    {
        ling.add_profile(Profile {
            name,
            look,
            protected: false,
        });
    }
    true
}

/// `blind/sting_action`.
fn blind_sting(world: &mut World, client: Entity, target: Entity) -> bool {
    use crate::medical::organs::{Slot, damage_organ, organ, state};
    let Some(eyes) = organ(world, target, Slot::Eyes) else {
        tell(world, client, Span::Warning, "нет глаз!");
        return false;
    };
    if state(world, target, Slot::Eyes).is_some_and(|o| o.robotic) {
        tell(world, client, Span::Warning, "роботизированные глаза!");
        return false;
    }
    chat_to(world, target, Span::Danger, "Ваши глаза ужасно горят!");
    let max = Slot::Eyes.spec().max_health;
    damage_organ(world, eyes, max * 0.8, max);
    crate::vision::adjust_temp_blindness(world, target, 40.0);
    crate::vision::set_eye_blur_if_lower(world, target, 80.0);
    true
}

/// The stings' timers and the transformations running out (paused while dead).
// Cost: per tick, the pending sting timers and the transformed mobs (a handful).
pub(super) fn sting_timers(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let delta = f64::from(world.resource::<Time>().delta_secs());
    let due: Vec<(Entity, StingTimer)> = {
        let mut timers = world.resource_mut::<StingTimers>();
        let due = timers
            .0
            .iter()
            .filter(|(at, ..)| *at <= now)
            .map(|&(_, e, t)| (e, t))
            .collect();
        timers.0.retain(|(at, ..)| *at > now);
        due
    };
    for (mob, timer) in due {
        if world.get_entity(mob).is_err() {
            continue;
        }
        match timer {
            StingTimer::Hallucinate => {
                crate::hallucination::adjust_hallucinations(world, mob, 180.0);
            }
            StingTimer::RemoveBlade(blade) => remove_fake(world, mob, blade),
        }
    }
    let stung: Vec<(Entity, TransStung, bool)> = world
        .query::<(Entity, &TransStung, Option<&Stat>)>()
        .iter(world)
        .map(|(e, t, s)| (e, t.clone(), s == Some(&Stat::Dead)))
        .collect();
    for (mob, mut stung, dead) in stung {
        if !dead {
            stung.remaining -= delta;
        }
        if stung.remaining > 0.0 {
            world.entity_mut(mob).insert(stung);
            continue;
        }
        world.entity_mut(mob).remove::<TransStung>();
        transform(world, mob, &stung.original);
    }
}

/// `false_armblade/remove_fake`.
fn remove_fake(world: &mut World, mob: Entity, blade: Entity) {
    if world.get_entity(blade).is_err() {
        return;
    }
    let victim = name_of(world, mob);
    visible(
        world,
        mob,
        "Армблейд возвращается в нормальное состояние.",
        &format!("С болезненным хрустом, {victim} превращает армблейд в руку!"),
        Span::Warning,
    );
    if let Some(&pos) = world.get::<TilePos>(mob) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, BLOB_ATTACK, 30.0, true),
        );
    }
    world.despawn(blade);
}
