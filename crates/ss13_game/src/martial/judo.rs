//! Corporate judo (BandaStation `modular_bandastation/martial_arts/code/judo/`, the judo belt):
//! nonlethal throws and holds for security. Punches deal 6 brute; help (`E`), shove, grab and
//! punch streaks make the judo throw, the armbar, the wheel throw, the eye poke and the golden
//! blast; a button readies the "discombobulate" slap. Batons slip out of a judoka's hands.
// shortcut: no spin animation, sparks or battle cry of the golden blast, no pepper-proof eye
// check of the eye poke; add them with animations, speech and eye protection.

use bevy::prelude::*;
use ss13_shared::chat::Span;

use super::{
    AttackKind, NEAR, Outcome, add_to_streak, check_block, hurt, incap, lying, message, name,
    play, reset_streak, roll, streak, tell_mob, with_art,
};
use crate::combat::{CLICK_CD_MELEE, NextMove};
use crate::health::DamageType;
use crate::status_effects::{Incap, effects_mut};

const ARMBAR: &str = "DDG";
const EYEPOKE: &str = "DH";
const GOLDENBLAST: &str = "EDEGDDGEDDGE";
const JUDOTHROW: &str = "GD";
const WHEELTHROW: &str = "GDH";
/// `DISCOMBOBULATE_COMBO`: the whole streak, set by the button.
const DISCOMBOBULATE: &str = "discombobulate";

pub(super) const DISCOMBOBULATE_BUTTON: &str = "martial:discombobulate";

/// `/datum/status_effect/judo_armbar`: 5 s.
const ARMBAR_SECS: f64 = 5.0;

pub(super) const HELP: &[&str] = &[
    "Вы взываете к техникам корпоратского дзюдо.",
    "Бросок: Grab Shove. Возьмите врага в захват и бросьте на пол. Наносит урон стамине.",
    "Большое колесо: Grab Shove Punch. Перекиньте противника, захваченного 'рычагом', через себя или прижмите его к полу.",
    "Тычок в глаза: Shove Punch. Ударьте противника в глаза, мгновенно ослепляя его.",
    "Рычаг: Shove Shove Grab. Возьмите лежащего противника в захват.",
    "Золотой взрыв: Help Shove Help Grab Shove Shove Grab Help Shove Shove Grab Help. Используя боевые искусства, вы можете оглушить противника жизненной энергией. Или перегрузив наниты пояса.",
    "Сбить с толку: Нанесите противнику удар по уху, ненадолго сбив его с толку.",
];

/// A mob in a judo armbar until this time (`/datum/status_effect/judo_armbar`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct JudoArmbar(pub f64);

/// `check_baton`: a baton in the hands of a judoka is dropped.
pub(super) fn drop_batons(world: &mut World, mob: Entity) {
    let held = world
        .get::<crate::inventory::Inventory>(mob)
        .map(|i| i.hands)
        .unwrap_or_default();
    let Some(&at) = world.get::<ss13_shared::TilePos>(mob) else {
        return;
    };
    for item in held.into_iter().flatten() {
        if world.get::<crate::security::Baton>(item).is_none()
            || crate::inventory::no_drop(world, item)
        {
            continue;
        }
        let item_name = name(world, item);
        tell_mob(
            world,
            mob,
            Span::Warning,
            format!("Вам противен {item_name}!"),
        );
        crate::inventory::put_down(world, item, at);
    }
}

/// The discombobulate button: ready the slap, or stand down.
pub(super) fn trigger(world: &mut World, mob: Entity) {
    if super::incapacitated(world, mob) || super::effects(world, mob).hands_blocked() {
        return;
    }
    let ready = streak(world, mob) == DISCOMBOBULATE;
    let mn = name(world, mob);
    let (others, own) = if ready {
        (
            format!("{mn} встает в нейтральную стойку."),
            "Вы не готовите новых приёмов.",
        )
    } else {
        (
            format!("{mn} встает в сбивающую стойку!"),
            "Ваш следующий приём - «сбить с толку».",
        )
    };
    message(
        world,
        mob,
        mob,
        NEAR,
        (Span::Danger, others),
        own.to_owned(),
        None,
    );
    with_art(world, mob, |art| {
        art.streak = if ready {
            String::new()
        } else {
            DISCOMBOBULATE.to_owned()
        };
    });
}

/// `check_with_no_reset`: the streak ends with the combo.
fn ends_with(streak: &str, combo: &str) -> bool {
    streak.ends_with(combo)
}

/// The finisher the streak makes, if any (`check_streak`'s order).
pub fn finisher(streak: &str) -> Option<&'static str> {
    if streak.contains(GOLDENBLAST) {
        return Some(GOLDENBLAST);
    }
    [WHEELTHROW, ARMBAR, EYEPOKE, JUDOTHROW]
        .into_iter()
        .find(|c| ends_with(streak, c))
}

fn check_streak(world: &mut World, a: Entity, d: Entity) -> bool {
    match finisher(&streak(world, a)) {
        Some(GOLDENBLAST) => {
            reset_streak(world, a);
            goldenblast(world, a, d)
        }
        Some(WHEELTHROW) => {
            reset_streak(world, a);
            wheelthrow(world, a, d)
        }
        Some(ARMBAR) => armbar(world, a, d),
        Some(EYEPOKE) => eyepoke(world, a, d),
        Some(JUDOTHROW) => judothrow(world, a, d),
        _ => false,
    }
}

fn check_one_click(world: &mut World, a: Entity, d: Entity) -> bool {
    if streak(world, a) != DISCOMBOBULATE {
        return false;
    }
    reset_streak(world, a);
    discombobulate(world, a, d)
}

pub(super) fn harm(world: &mut World, a: Entity, d: Entity) -> Outcome {
    if check_one_click(world, a, d) {
        return Outcome::Success;
    }
    add_to_streak(world, a, "H", d);
    if check_streak(world, a, d) {
        return Outcome::Success;
    }
    hurt(world, d, None, DamageType::Brute, 6.0, Some(0.0));
    play(world, d, "sound/effects/hit_punch.ogg", 50.0);
    let verb = ["рубит", "режет", "бьёт"][roll(world, 0, 2) as usize];
    let (an, dn) = (name(world, a), name(world, d));
    message(
        world,
        a,
        d,
        NEAR,
        (Span::Danger, format!("{an} {verb} {dn}!")),
        format!("{an} {verb} вас!"),
        None,
    );
    Outcome::Success
}

pub(super) fn disarm(world: &mut World, a: Entity, d: Entity) -> Outcome {
    if check_one_click(world, a, d) {
        return Outcome::Success;
    }
    let text = name(world, a);
    if check_block(world, d, Some(a), AttackKind::Unarmed, 0.0, &text) {
        return Outcome::Fail;
    }
    add_to_streak(world, a, "D", d);
    if check_streak(world, a, d) {
        return Outcome::Success;
    }
    if a == d {
        tell_mob(world, a, Span::Notice, "Вы добавили толчок в комбо.");
        return Outcome::Fail;
    }
    Outcome::Invalid
}

pub(super) fn grab(world: &mut World, a: Entity, d: Entity) -> Outcome {
    if check_block(world, d, Some(a), AttackKind::Unarmed, 0.0, "захват") {
        return Outcome::Fail;
    }
    add_to_streak(world, a, "G", d);
    Outcome::from_bool(check_streak(world, a, d))
}

pub(super) fn help(world: &mut World, a: Entity, d: Entity) -> Outcome {
    if check_one_click(world, a, d) {
        return Outcome::Success;
    }
    add_to_streak(world, a, "E", d);
    let now = world.resource::<Time>().elapsed_secs_f64();
    world
        .entity_mut(a)
        .insert(NextMove(now + CLICK_CD_MELEE / 2.0));
    Outcome::from_bool(check_streak(world, a, d))
}

fn armbar(world: &mut World, a: Entity, d: Entity) -> bool {
    if !lying(world, d) {
        return false;
    }
    let (an, dn) = (name(world, a), name(world, d));
    message(
        world,
        a,
        d,
        NEAR,
        (Span::Warning, format!("{an} берёт {dn} в захват!")),
        format!("{an} берёт вас в захват!"),
        None,
    );
    play(world, a, "sound/items/weapons/slashmiss.ogg", 40.0);
    if !lying(world, a) {
        super::drop_held(world, d);
    }
    hurt(world, d, None, DamageType::Stamina, 45.0, None);
    let now = world.resource::<Time>().elapsed_secs_f64();
    world.entity_mut(d).insert(JudoArmbar(now + ARMBAR_SECS));
    incap(world, d, Incap::Knockdown, 5.0);
    true
}

fn eyepoke(world: &mut World, a: Entity, d: Entity) -> bool {
    let (an, dn) = (name(world, a), name(world, d));
    message(
        world,
        a,
        d,
        NEAR,
        (Span::Warning, format!("{an} тыкает в глаза {dn}!")),
        format!("{an} тыкает вам в глаза!"),
        None,
    );
    play(world, a, "sound/items/weapons/whip.ogg", 40.0);
    hurt(world, d, None, DamageType::Brute, 10.0, Some(0.0));
    crate::vision::set_eye_blur_if_lower(world, d, 30.0);
    crate::vision::adjust_temp_blindness(world, d, 2.0);
    true
}

fn goldenblast(world: &mut World, a: Entity, d: Entity) -> bool {
    let (an, dn) = (name(world, a), name(world, d));
    message(
        world,
        a,
        d,
        NEAR,
        (
            Span::Warning,
            format!("{an} бьёт {dn} энергией, прижимая к земле!"),
        ),
        format!(
            "{an} делает странные жесты руками, дико кричит и тычет вам прямо в грудь! Вы чувствуете, как гнев ЗОЛОТОГО РАЗРЯДА пронзает ваше тело! Вы совершенно обробастены!"
        ),
        None,
    );
    play(world, d, "sound/items/weapons/taser.ogg", 55.0);
    hurt(world, d, None, DamageType::Stamina, 120.0, None);
    incap(world, d, Incap::Knockdown, 30.0);
    set_confusion(world, d, 30.0);
    true
}

fn judothrow(world: &mut World, a: Entity, d: Entity) -> bool {
    if lying(world, a) || lying(world, d) {
        return false;
    }
    let (an, dn) = (name(world, a), name(world, d));
    message(
        world,
        a,
        d,
        NEAR,
        (Span::Warning, format!("{an} бросает {dn} на землю!")),
        format!("{an} бросает вас на землю!"),
        None,
    );
    play(world, a, "sound/items/weapons/slam.ogg", 40.0);
    hurt(world, d, None, DamageType::Stamina, 25.0, None);
    incap(world, d, Incap::Knockdown, 7.0);
    true
}

fn wheelthrow(world: &mut World, a: Entity, d: Entity) -> bool {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let armbarred = world.get::<JudoArmbar>(d).is_some_and(|h| now < h.0);
    if !lying(world, d) || !armbarred {
        return false;
    }
    let (an, dn) = (name(world, a), name(world, d));
    if lying(world, a) {
        message(
            world,
            a,
            d,
            NEAR,
            (Span::Warning, format!("{an} прижимает {dn} к земле.")),
            format!("{an} прижимает вас к земле!"),
            None,
        );
        play(world, a, "sound/items/weapons/slam.ogg", 40.0);
    } else {
        message(
            world,
            a,
            d,
            NEAR,
            (
                Span::Warning,
                format!("{an} бросает {dn} через плечо, и стукает об землю!"),
            ),
            format!("{an} бросает вас через плечо, стукая об землю!"),
            None,
        );
        play(world, a, "sound/effects/magic/tail_swing.ogg", 40.0);
    }
    hurt(world, d, None, DamageType::Stamina, 120.0, None);
    incap(world, d, Incap::Knockdown, 15.0);
    set_confusion(world, d, 10.0);
    true
}

fn discombobulate(world: &mut World, a: Entity, d: Entity) -> bool {
    let (an, dn) = (name(world, a), name(world, d));
    // BandaStation's line names the attacker twice; the target is meant.
    message(
        world,
        a,
        d,
        NEAR,
        (Span::Warning, format!("{an} бьёт {dn} ладонью по голове!")),
        format!("{an} ударяет вас ладонью!"),
        None,
    );
    play(world, a, "sound/items/weapons/slap.ogg", 40.0);
    hurt(world, d, None, DamageType::Stamina, 10.0, None);
    if let Some(mut e) = effects_mut(world, d) {
        e.confusion += 5.0;
    }
    true
}

/// tg `set_confusion`.
fn set_confusion(world: &mut World, mob: Entity, secs: f32) {
    if let Some(mut e) = effects_mut(world, mob) {
        e.confusion = secs;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finishers_end_the_streak_in_tg_order() {
        assert_eq!(finisher("HGD"), Some(JUDOTHROW));
        assert_eq!(finisher("GDH"), Some(WHEELTHROW), "before the eye poke");
        assert_eq!(finisher("HDH"), Some(EYEPOKE));
        assert_eq!(finisher("HDDG"), Some(ARMBAR));
        assert_eq!(finisher("GDHE"), None, "only at the end");
        assert_eq!(finisher("EDEGDDGEDDGE"), Some(GOLDENBLAST));
    }
}
