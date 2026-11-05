//! Wrestling (tg `code/datums/martial/wrestling.dm`, from Goonstation; the champion's belt):
//! action buttons ready a move (slam, throw, kick, strike, leg drop), the next punch, shove or
//! grab on someone performs it; a grab without a readied move cinches (pulls and stuns).
// shortcut: the moves land at once: no spinning, lifting or climbing animations and sleeps
// (the throw and slam keep their checks of the cinch and their effects), no screams or flips,
// no headbutt step onto the target's tile, no leg drop from a climbed object; add them with
// timed move sequences and emotes.

use bevy::prelude::*;
use ss13_shared::chat::Span;

use super::{
    AttackKind, COMBAT, NEAR, Outcome, capitalize, check_block, dir_to, fling, hurt, incap,
    knock_out, message, name, play, roll, streak, tell_mob, unconscious_or_crit, with_art,
};
use crate::explosions::Severity;
use crate::health::DamageType;
use crate::pull::{GrabState, PullSlowdown, Pulling};
use crate::status_effects::Incap;

pub(super) const HELP: &[&str] = &[
    "SNAP INTO A THIN TIM!",
    "Приёмы выбираются кнопками вверху экрана; следующий удар, толчок или захват выполняет выбранный приём.",
    "Бросок тела: впечатайте в пол противника, которого держите в захвате.",
    "Бросок: раскрутите и швырните противника, которого держите в захвате.",
    "Пинок: мощный удар с разворота, отбрасывающий противника.",
    "Удар: быстрый удар головой.",
    "Падение: обрушьтесь на противника всем телом.",
];

/// The move buttons: (id, name, `button_icon_state`); the streak is the id after the prefix.
pub(super) const MOVES: [(&str, &str, &str); 5] = [
    (
        "martial:slam",
        "Бросок тела (захват) - впечатать схваченного противника в пол.",
        "wrassle_slam",
    ),
    (
        "martial:throw",
        "Бросок (захват) - раскрутить и швырнуть схваченного противника.",
        "wrassle_throw",
    ),
    (
        "martial:kick",
        "Пинок - мощный удар, отбрасывающий противника.",
        "wrassle_kick",
    ),
    (
        "martial:strike",
        "Удар - быстрый удар по стоящему рядом противнику.",
        "wrassle_strike",
    ),
    (
        "martial:drop",
        "Падение - обрушиться на противника.",
        "wrassle_drop",
    ),
];

const SWING_HIT: &str = ss13_mapload::sound::files::SFX_SWING_HIT;

/// A move button (`/datum/action/<move>/Trigger`): its `check_flags`, then the move readied.
pub(super) fn trigger(world: &mut World, mob: Entity, id: &str) {
    if !MOVES.iter().any(|(m, ..)| *m == id) {
        return;
    }
    let kind = id.trim_start_matches(super::BUTTON);
    // `AB_CHECK_CONSCIOUS|AB_CHECK_INCAPACITATED` (+ `HANDS_BLOCKED` but for the kick and the
    // drop's conscious check).
    let hands = kind != "kick" && kind != "drop";
    let effects = super::effects(world, mob);
    if super::incapacitated(world, mob) || (hands && effects.hands_blocked()) {
        return;
    }
    let (others, own) = match kind {
        "slam" => (
            "готовится к БРОСКУ ТЕЛА!",
            "Ваша следующая атака - БРОСОК ТЕЛА.",
        ),
        "throw" => ("готовится к БРОСКУ!", "Ваша следующая атака - БРОСОК."),
        "kick" => ("готовится к ПИНКУ!", "Ваша следующая атака - ПИНОК."),
        "strike" => ("готовится к УДАРУ!", "Ваша следующая атака - УДАР."),
        _ => ("готовится к ПАДЕНИЮ!", "Ваша следующая атака - ПАДЕНИЕ."),
    };
    let mn = name(world, mob);
    message(
        world,
        mob,
        mob,
        NEAR,
        (Span::Danger, format!("{} {others}", capitalize(&mn))),
        own.to_owned(),
        None,
    );
    with_art(world, mob, |art| {
        art.streak = kind.to_owned();
    });
}

/// `harm_act`/`disarm_act`: the readied move.
pub(super) fn act(world: &mut World, a: Entity, d: Entity) -> Outcome {
    Outcome::from_bool(check_streak(world, a, d))
}

pub(super) fn grab(world: &mut World, a: Entity, d: Entity) -> Outcome {
    if check_streak(world, a, d) {
        return Outcome::Success;
    }
    let text = format!("захват {}", name(world, a));
    if check_block(world, d, Some(a), AttackKind::Unarmed, 0.0, &text) {
        return Outcome::Fail;
    }
    if world.get::<Pulling>(a).map(|p| p.0) == Some(d) {
        return Outcome::Fail;
    }
    world
        .entity_mut(a)
        .insert((Pulling(d), GrabState::Passive, PullSlowdown(0.0)));
    let (an, dn) = (name(world, a), name(world, d));
    message(
        world,
        a,
        d,
        COMBAT,
        (Span::Danger, format!("{an} берёт {dn} в замок!")),
        format!("{} берёт вас в замок!", capitalize(&an)),
        Some((Span::Danger, format!("Вы берёте {dn} в замок!"))),
    );
    let secs = roll(world, 60, 100) as f32 / 10.0;
    incap(world, d, Incap::Stun, secs);
    Outcome::Success
}

/// `check_streak`: the readied move, after the defender's block.
fn check_streak(world: &mut World, a: Entity, d: Entity) -> bool {
    let streak = streak(world, a);
    if !matches!(
        streak.as_str(),
        "drop" | "strike" | "kick" | "throw" | "slam"
    ) {
        return false;
    }
    let text = format!("{} {}", streak, name(world, a));
    if check_block(world, d, Some(a), AttackKind::Unarmed, 10.0, &text) {
        return false;
    }
    with_art(world, a, |art| art.streak.clear());
    match streak.as_str() {
        "drop" => leg_drop(world, a, d),
        "strike" => strike(world, a, d),
        "kick" => kick(world, a, d),
        "throw" => throw(world, a, d),
        _ => slam(world, a, d),
    }
    true
}

fn cinched(world: &mut World, a: Entity, d: Entity) -> bool {
    if world.get::<Pulling>(a).map(|p| p.0) == Some(d) {
        return true;
    }
    let dn = name(world, d);
    tell_mob(
        world,
        a,
        Span::Warning,
        format!("Вам нужно держать {dn} в захвате!"),
    );
    false
}

/// `throw_wrassle`: spun around (stunned 8 s), then thrown ten tiles the way the wrestler
/// faces and paralyzed 2 s on landing.
fn throw(world: &mut World, a: Entity, d: Entity) {
    if !cinched(world, a, d) {
        return;
    }
    incap(world, d, Incap::Stun, 8.0);
    let (an, dn) = (name(world, a), name(world, d));
    message(
        world,
        a,
        d,
        NEAR,
        (Span::Danger, format!("{an} швыряет {dn}!")),
        format!("Вас швыряет {an}!"),
        Some((Span::Danger, format!("Вы швыряете {dn}!"))),
    );
    play(world, a, SWING_HIT, 50.0);
    // `throw_at` stops the pull.
    world.entity_mut(a).remove::<Pulling>();
    let dir = world
        .get::<ss13_shared::Facing>(a)
        .map_or(ss13_shared::Dir::South, |f| f.0);
    fling(world, d, dir, 10, 4.0);
    incap(world, d, Incap::Paralyze, 2.0);
}

/// `slam`: a body-slam of the cinched mob: paralyzed 4 s and 10-30 brute or a light explosion
/// (always the explosion on one already down).
fn slam(world: &mut World, a: Entity, d: Entity) {
    if !cinched(world, a, d) {
        return;
    }
    let fluff = if roll(world, 2, 3) == 2 {
        "турбо-бросок тела"
    } else {
        "атомный бросок тела"
    };
    let (an, dn) = (name(world, a), name(world, d));
    message(
        world,
        a,
        d,
        COMBAT,
        (Span::Danger, format!("{an} проводит {fluff} {dn}!")),
        format!("{} проводит вам {fluff}!", capitalize(&an)),
        Some((Span::Danger, format!("Вы проводите {fluff} {dn}!"))),
    );
    play(world, a, SWING_HIT, 50.0);
    if unconscious_or_crit(world, d) {
        crate::explosions::ex_act::ex_act_mob(world, d, Severity::Light);
        return;
    }
    incap(world, d, Incap::Paralyze, 4.0);
    match roll(world, 1, 3) {
        2 => {
            let damage = roll(world, 20, 30) as f32;
            hurt(world, d, None, DamageType::Brute, damage, None);
        }
        3 => crate::explosions::ex_act::ex_act_mob(world, d, Severity::Light),
        _ => {
            let damage = roll(world, 10, 20) as f32;
            hurt(world, d, None, DamageType::Brute, damage, None);
        }
    }
}

/// `strike`: a headbutt, 10-20 brute and knocked out 2 s.
fn strike(world: &mut World, a: Entity, d: Entity) {
    let (an, dn) = (name(world, a), name(world, d));
    message(
        world,
        a,
        d,
        COMBAT,
        (Span::Danger, format!("{an} бьёт {dn} головой!")),
        format!("{} бьёт вас головой!", capitalize(&an)),
        Some((Span::Danger, format!("Вы бьёте {dn} головой!"))),
    );
    let damage = roll(world, 10, 20) as f32;
    hurt(world, d, None, DamageType::Brute, damage, None);
    play(world, a, SWING_HIT, 50.0);
    knock_out(world, d, 2.0);
}

/// `kick`: a roundhouse kick, 10-20 brute, paralyzed 2 s and thrown three tiles away.
fn kick(world: &mut World, a: Entity, d: Entity) {
    let (an, dn) = (name(world, a), name(world, d));
    message(
        world,
        a,
        d,
        COMBAT,
        (Span::Danger, format!("{an} бьёт {dn} ногой с разворота!")),
        format!("{} бьёт вас ногой с разворота!", capitalize(&an)),
        Some((Span::Danger, format!("Вы бьёте {dn} ногой с разворота!"))),
    );
    play(world, a, SWING_HIT, 50.0);
    let damage = roll(world, 10, 20) as f32;
    hurt(world, d, None, DamageType::Brute, damage, None);
    incap(world, d, Incap::Paralyze, 2.0);
    if let Some(dir) = dir_to(world, a, d)
        && a != d
    {
        fling(world, d, dir, 3, 2.0);
    }
}

/// `drop`: a leg drop, 20-30 brute and paralyzed 4 s.
fn leg_drop(world: &mut World, a: Entity, d: Entity) {
    let (an, dn) = (name(world, a), name(world, d));
    message(
        world,
        a,
        d,
        NEAR,
        (Span::Danger, format!("{an} обрушивается на {dn}!")),
        format!("{} обрушивается на вас!", capitalize(&an)),
        Some((Span::Danger, format!("Вы обрушиваетесь на {dn}!"))),
    );
    play(world, a, SWING_HIT, 50.0);
    let damage = roll(world, 20, 30) as f32;
    hurt(world, d, None, DamageType::Brute, damage, None);
    incap(world, d, Incap::Paralyze, 4.0);
}
