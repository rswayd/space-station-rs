//! Shapeshifting (server) after tg `spell_types/shapeshift/` (`_shapeshift.dm`,
//! `_shape_status.dm`).
//!
//! The caster's mind goes into a new mob of the chosen form, the caster waits inside it
//! (`crate::closets::put_mob_inside`, tg `forceMove(owner)` + stasis) and its spells follow
//! ([`Spells`] moves to the shape, tg `source_spell.Grant(owner)`). The same spell cast in the
//! form turns it back; another shapeshift spell refuses. The damage share carries over both
//! ways as brute. When the form dies the caster comes back and dies too (`revert_on_death` and
//! `die_with_shapeshifted_form`, tg's defaults).
//! Wild Shapeshift asks for its form once ([`trigger_wild`]) and keeps it.
// shortcut: the form choice is a list (tg: a radial menu of icons); no ventcrawling or
// wabbajack rules, no blood volume carry-over, the shapeshifted alert is missing and the
// waiting body does not die with the form's owner in the other direction (`on_caster_death`:
// nothing reaches a body inside). The player's other action buttons stay on the old body.
// `crate::transform::transform_into_mob` is not used: it deletes the old body for good, a
// shapeshift has to give it back.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_shared::chat::Span;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};

use super::{CastOn, Spell, Spells, can_cast, cast as cast_spell, chat_to, knows, start_cooldown};
use crate::click::ClickResolved;
use crate::health::{DamageType, Health, Stat};
use crate::server::PlayerMob;

const WILD_SHAPES: [(&str, &str); 6] = [
    ("/mob/living/basic/carp/magic/chaos", "chaos magicarp"),
    (
        "/mob/living/basic/construct/juggernaut/mystic",
        "mystic juggernaut",
    ),
    ("/mob/living/basic/mouse", "mouse"),
    ("/mob/living/basic/pet/dog/corgi", "corgi"),
    (
        "/mob/living/basic/spider/giant/viper/wizard",
        "viper spider",
    ),
    ("/mob/living/basic/bot/secbot/ed209", "ED-209"),
];

const WILD_PATHS: [&str; 6] = [
    WILD_SHAPES[0].0,
    WILD_SHAPES[1].0,
    WILD_SHAPES[2].0,
    WILD_SHAPES[3].0,
    WILD_SHAPES[4].0,
    WILD_SHAPES[5].0,
];

/// `possible_shapes` of `spell`; `None`: not a shapeshift.
pub fn possible_shapes(spell: Spell) -> Option<&'static [&'static str]> {
    Some(match spell {
        Spell::DragonForm => &["/mob/living/simple_animal/hostile/megafauna/dragon/lesser"],
        Spell::GorillaForm => &["/mob/living/basic/gorilla"],
        Spell::PolarBearForm => &["/mob/living/basic/mining/polarbear/lesser"],
        Spell::WolfForm => &["/mob/living/basic/mining/wolf"],
        Spell::WildShapeshift => return Some(&WILD_PATHS),
        _ => return None,
    })
}

pub(super) fn handles(spell: Spell) -> bool {
    possible_shapes(spell).is_some()
}

/// On a shape: its caster inside and the spell that made it.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shapeshifted {
    pub caster: Entity,
    pub spell: Spell,
}

/// A `tgui_input_list` for the Wild Shapeshift form.
#[derive(Component, Clone, Copy, Debug)]
pub struct ShapePrompt;

/// `shapeshift_type` of a known spell, the only one when there is a single choice.
fn chosen_shape(world: &World, mob: Entity, spell: Spell) -> Option<&'static str> {
    if let Some(shape) = world
        .get::<Spells>(mob)
        .and_then(|s| s.get(spell))
        .and_then(|k| k.shape)
    {
        return Some(shape);
    }
    match possible_shapes(spell)? {
        [only] => Some(only),
        _ => None,
    }
}

/// The Wild Shapeshift button: "Once you've made your choice, it cannot be changed."
pub(super) fn trigger_wild(world: &mut World, client: Entity, mob: Entity) {
    if world.get::<Shapeshifted>(mob).is_some()
        || chosen_shape(world, mob, Spell::WildShapeshift).is_some()
    {
        cast_spell(world, client, mob, Spell::WildShapeshift, CastOn::Caster);
        return;
    }
    let items = WILD_SHAPES
        .iter()
        .map(|&(_, name)| name.to_owned())
        .collect();
    let ask = Ask::new(
        client,
        mob,
        "Wild Shapeshift",
        "Choose a form.",
        PromptKind::List {
            items,
            default: None,
        },
    );
    let prompt = ask_world(world, ask);
    world.entity_mut(prompt).insert(ShapePrompt);
}

pub(super) fn answers(world: &mut World, mut answers: Local<MessageCursor<PromptAnswered>>) {
    let pending: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in pending {
        if world.get::<ShapePrompt>(prompt).is_none() {
            continue;
        }
        let Some(PromptAnswer::Item(i)) = answer else {
            continue;
        };
        let Some(&(shape, _)) = WILD_SHAPES.get(i) else {
            continue;
        };
        // After the wait: the same player, the spell known and castable (`can_cast_spell`).
        if world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
            || !knows(world, actor, Spell::WildShapeshift)
            || can_cast(world, actor, Spell::WildShapeshift).is_err()
        {
            continue;
        }
        if let Some(mut spells) = world.get_mut::<Spells>(actor)
            && let Some(known) = spells.get_mut(Spell::WildShapeshift)
        {
            known.shape = Some(shape);
        }
        cast_spell(world, client, actor, Spell::WildShapeshift, CastOn::Caster);
    }
}

/// `shapeshift/cast`: the shift back or forth.
pub(super) fn cast(world: &mut World, _client: Entity, mob: Entity, spell: Spell) -> bool {
    if let Some(shift) = world.get::<Shapeshifted>(mob).copied() {
        if shift.spell != spell {
            chat_to(
                world,
                mob,
                Span::Warning,
                "This spell won't un-shapeshift you from this form!",
            );
            return false;
        }
        restore(world, mob, shift, false, true);
        return true;
    }
    // tg `TRAIT_NO_TRANSFORM`: no shapeshifting in the middle of `crate::transform`.
    if crate::transform::is_transforming(world, mob) {
        return false;
    }
    let Some(path) = chosen_shape(world, mob, spell) else {
        return false;
    };
    do_shapeshift(world, mob, spell, path)
}

/// The damage share of `health`.
fn damage_share(health: &Health) -> f32 {
    if health.max <= 0.0 {
        return 0.0;
    }
    ((health.max - health.health()) / health.max).clamp(0.0, 1.0)
}

fn do_shapeshift(world: &mut World, mob: Entity, spell: Spell, path: &str) -> bool {
    let Some(pos) = crate::explosions::turf_of(world, mob) else {
        return false;
    };
    let Some(shape) = crate::mobs::spawn::spawn_mob(world, path, pos) else {
        chat_to(
            world,
            mob,
            Span::Warning,
            "You can't shapeshift in this form!",
        );
        return false;
    };
    let share = world.get::<Health>(mob).map_or(0.0, damage_share);
    if let Some(mut health) = world.get::<Health>(shape).cloned() {
        health.adjust(DamageType::Brute, health.max * share);
        world.entity_mut(shape).insert(health);
    }
    if !crate::medical::brainmob::transfer_mind(world, mob, shape) {
        world.despawn(shape);
        return false;
    }
    crate::closets::put_mob_inside(world, mob, shape);
    world
        .entity_mut(shape)
        .insert(Shapeshifted { caster: mob, spell });
    if let Some(spells) = world.entity_mut(mob).take::<Spells>() {
        world.entity_mut(shape).insert(spells);
        // `spells::cast` starts the cooldown on the mob that no longer holds the spell.
        start_cooldown(world, shape, spell, None);
    }
    info!("{mob} shapeshifted into {path} ({shape})");
    true
}

/// `restore_caster` + `after_unchange`. `delete` despawns the shape now (else later, its
/// own death code still runs on it).
fn restore(world: &mut World, shape: Entity, shift: Shapeshifted, kill: bool, delete: bool) {
    let caster = shift.caster;
    let pos = crate::explosions::turf_of(world, shape);
    if world.get_entity(caster).is_err() {
        if delete {
            world.despawn(shape);
        }
        return;
    }
    if let Some(pos) = pos {
        world.entity_mut(caster).insert(pos);
    }
    crate::closets::let_out(world, caster);
    let share = world.get::<Health>(shape).map_or(0.0, damage_share);
    if let Some(health) = world.get::<Health>(caster).cloned() {
        let mut healed = health.healed();
        healed.adjust(DamageType::Brute, healed.max * share);
        world.entity_mut(caster).insert(healed);
    }
    if let Some(spells) = world.entity_mut(shape).take::<Spells>() {
        world.entity_mut(caster).insert(spells);
    }
    crate::medical::brainmob::transfer_mind(world, shape, caster);
    start_cooldown(world, caster, shift.spell, None);
    world.entity_mut(shape).remove::<Shapeshifted>();
    if kill {
        crate::medical::organs::kill(world, caster);
    }
    if delete {
        world.despawn(shape);
    } else {
        let t = super::now(world);
        world.entity_mut(shape).insert(crate::traps::DeleteAt(t));
    }
}

/// A form fights like its mob: a plain click bites the living (`melee_attack`).
pub(super) fn form_clicks(world: &mut World, mut clicks: Local<MessageCursor<ClickResolved>>) {
    let clicks: Vec<ClickResolved> = clicks
        .read(world.resource::<Messages<ClickResolved>>())
        .copied()
        .collect();
    for click in clicks {
        let (mob, target) = (click.mob, click.target);
        if click.modifiers.right
            || mob == target
            || world.get::<Shapeshifted>(mob).is_none()
            || world.get::<Stat>(mob) != Some(&Stat::Conscious)
            || world.get::<Health>(target).is_none()
        {
            continue;
        }
        crate::mobs::defense::melee_attack(world, mob, target);
    }
}

/// `on_shape_death`: the form died, the caster comes back and dies with it.
pub fn on_shape_death(world: &mut World, shape: Entity) {
    if let Some(shift) = world.get::<Shapeshifted>(shape).copied() {
        restore(world, shape, shift, true, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_shapeshift_spells_have_shapes() {
        assert!(handles(Spell::WolfForm));
        assert_eq!(
            possible_shapes(Spell::WildShapeshift).map(<[_]>::len),
            Some(6)
        );
        assert!(possible_shapes(Spell::Fireball).is_none());
    }

    #[test]
    fn damage_share_is_a_fraction() {
        let mut health = Health::basic(100.0, [1.0; 5]);
        health.adjust(DamageType::Brute, 25.0);
        assert!((damage_share(&health) - 0.25).abs() < 1e-3);
    }
}
