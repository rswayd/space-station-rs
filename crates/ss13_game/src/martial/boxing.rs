//! Boxing (tg `code/datums/martial/boxing.dm`): every punch and shove is a tussle that deals
//! stamina damage; left/right click streaks with the active hand make straight punches, jabs,
//! hooks and uppercuts; a knockout chance (the arm's `unarmed_effectiveness`) staggers, or
//! knocks out a staggered target; blocking punches in throw mode. Honorable boxers only box
//! other boxers (no combos or knockouts against anyone else), never hit a downed target and
//! never grab. Evil boxing hits brute, sucker punches from behind and grabs; hunter boxing hits
//! brute and boxes beasts (basic mobs) with extra damage and a crushing crit that heals.
// shortcut: no athletics skill (its damage and knockout bonus is 0) or experience, no
// `TRAIT_STRENGTH` other than evil boxing's, no cyberspine, no `TRAIT_DETECTIVES_TASTE`
// discombobulate, no crit visual, no rebuke status or battle cry of hunter boxing, no
// `TRAIT_STIMMED`; add them with skills and those traits.

use bevy::prelude::*;
use ss13_shared::chat::Span;

use super::{
    AttackKind, COMBAT, Outcome, Style, active_arm, active_style, add_to_streak, adjacent,
    capitalize, chance, check_block, dir_to, effects, fling, hurt, incapacitated, lying,
    melee_armor, message, name, play, reset_streak, roll, stamina_loss, streak, throw_mode,
    unconscious, zone_selected,
};
use crate::combat::{ARM, CLICK_CD_MELEE, NextMove};
use crate::health::{DamageType, Health, Stat};
use crate::pull::{GrabState, Pulling};
use crate::status_effects::{Incap, effects_mut};
use crate::zone::BodyZone;

/// `STAGGERED_SLOWDOWN_LENGTH`, the stagger's max.
const STAGGER: f32 = 3.0;
const STAGGER_MAX: f32 = 10.0;

pub(super) const HELP: &[&str] = &[
    "Вы сосредотачиваетесь на стойке, представляя, как лучше нанести удар.",
    "Приёмы зависят от того, какой кнопкой мыши вы бьёте и совпадает ли последняя кнопка с выбранной рукой.",
    "Прямой удар: Левая Левая/Правая Правая той же рукой. Обычный урон.",
    "Джеб: Левая Левая/Правая Правая другой рукой. Обычный урон. Если вы слепы, это будет слепой джеб.",
    "Левый/Правый хук: Левая Правая/Правая Левая той же рукой. Больше урона, но следующий удар медленнее.",
    "Апперкот: Левая Правая/Правая Левая другой рукой. Больше шанс нокаута, но следующий удар медленнее.",
    "В режиме броска вы блокируете удары и отвечаете нападающему; блок стоит вам выносливости.",
    "Ваши боксёрские приёмы работают только против других боксёров.",
];

/// A tussle's combo (`check_streak`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Combo {
    None,
    Straight,
    LightJab,
    BlindJab,
    LeftHook,
    RightHook,
    Uppercut,
    Craven,
}

impl Combo {
    /// `combo_multiplier`.
    fn multiplier(self) -> f32 {
        match self {
            Self::None => 0.0,
            Self::BlindJab => 0.5,
            Self::Straight | Self::LightJab | Self::Uppercut => 1.0,
            Self::LeftHook | Self::RightHook => 1.5,
            Self::Craven => 2.0,
        }
    }

    /// The next punch waits `CLICK_CD_MELEE * 1.5`.
    fn slow(self) -> bool {
        matches!(
            self,
            Self::LeftHook | Self::RightHook | Self::Uppercut | Self::BlindJab
        )
    }

    /// tg `current_atk_verb`.
    fn verb(self) -> &'static str {
        match self {
            Self::None => "удар",
            Self::Straight => "прямой удар",
            Self::LightJab => "лёгкий джеб",
            Self::BlindJab => "слепой джеб",
            Self::LeftHook => "левый хук",
            Self::RightHook => "правый хук",
            Self::Uppercut => "апперкот",
            Self::Craven => "подлый удар",
        }
    }
}

/// `check_streak` with the punching arm (`left`). The streak's last two clicks: `H` left
/// click, `D` right click.
// tg returns the uppercut only for an arm that is neither left nor right, so it never happens
// (and a hook's opposite hand does nothing); the help text says the opposite hand uppercuts.
pub fn combo(streak: &str, left: bool, behind: bool, honorable: bool, blind: bool) -> Combo {
    if behind && !honorable {
        return Combo::Craven;
    }
    let (ll, rr) = (streak.contains("HH"), streak.contains("DD"));
    if (ll && !left) || (rr && left) {
        return if blind {
            Combo::BlindJab
        } else {
            Combo::LightJab
        };
    }
    if (ll && left) || (rr && !left) {
        return Combo::Straight;
    }
    let (lr, rl) = (streak.contains("DH"), streak.contains("HD"));
    if lr || rl {
        return match (left, rl, lr) {
            (true, true, _) => Combo::LeftHook,
            (false, _, true) => Combo::RightHook,
            _ => Combo::Uppercut,
        };
    }
    Combo::None
}

/// tg `check_behind`: the same tile, lying, or the target's back turned to the attacker.
fn behind(world: &World, a: Entity, d: Entity) -> bool {
    let (Some(from), Some(to)) = (
        world.get::<ss13_shared::TilePos>(a),
        world.get::<ss13_shared::TilePos>(d),
    ) else {
        return false;
    };
    if from == to || lying(world, d) {
        return true;
    }
    let Some(dir) = crate::pull::dir_between(*to, *from) else {
        return false;
    };
    let facing = world
        .get::<ss13_shared::Facing>(d)
        .map_or(ss13_shared::Dir::South, |f| f.0);
    ((facing as u8) & (dir.reverse() as u8)) != 0
}

/// `honorable_boxer`.
fn honorable(style: Style) -> bool {
    style != Style::EvilBoxing
}

/// `default_damage_type`.
fn default_damage(style: Style) -> DamageType {
    if style == Style::HunterBoxing {
        DamageType::Brute
    } else {
        DamageType::Stamina
    }
}

/// A beast for hunter boxing (`MOB_HUMANOID` is not set): a basic mob.
fn beast(world: &World, mob: Entity) -> bool {
    world.get::<Health>(mob).is_some_and(|h| h.basic.is_some())
}

/// `honor_check`: whether the target can be boxed in earnest.
fn honor_check(world: &World, style: Style, target: Entity) -> bool {
    if !honorable(style) {
        return true;
    }
    if active_style(world, target).is_some_and(Style::boxing) {
        return true;
    }
    style == Style::HunterBoxing && beast(world, target)
}

pub(super) fn harm(world: &mut World, style: Style, a: Entity, d: Entity) -> Outcome {
    if honor_check(world, style, d) {
        add_to_streak(world, a, "H", d);
    }
    tussle(world, style, a, d);
    Outcome::Success
}

pub(super) fn disarm(world: &mut World, style: Style, a: Entity, d: Entity) -> Outcome {
    if honor_check(world, style, d) {
        add_to_streak(world, a, "D", d);
    }
    tussle(world, style, a, d);
    Outcome::Success
}

pub(super) fn grab(world: &mut World, style: Style, a: Entity) -> Outcome {
    if honorable(style) {
        super::tell_mob(world, a, Span::Warning, "Никаких захватов в боксе!");
        return Outcome::Fail;
    }
    Outcome::Invalid
}

/// `perform_extra_effect` of hunter boxing: 15-20 more against a beast.
fn extra_effect(world: &mut World, style: Style, d: Entity) {
    if style == Style::HunterBoxing && beast(world, d) {
        let extra = roll(world, 15, 20) as f32;
        hurt(
            world,
            d,
            Some(BodyZone::Chest),
            DamageType::Brute,
            extra,
            Some(0.0),
        );
    }
}

/// `tussle`: the one boxing move.
fn tussle(world: &mut World, style: Style, a: Entity, d: Entity) {
    let honor = honorable(style);
    let (an, dn) = (name(world, a), name(world, d));
    if honor && ((lying(world, d) && stamina_loss(world, d) >= 100.0) || unconscious(world, d)) {
        super::tell_mob(world, a, Span::Warning, "Неспортивное поведение!");
        return;
    }
    let Some(arm) = active_arm(world, a) else {
        return;
    };
    let mut effectiveness = ARM.effectiveness;
    let damage_type = if honor {
        default_damage(style)
    } else {
        DamageType::Brute
    };
    // `possible_wound_bonus`: an honorable boxer's punch may wound, a dishonorable one cannot.
    let mut wound_bonus = if honor { Some(0.0) } else { None };
    let mut damage = roll(world, ARM.damage_low, ARM.damage_high) as f32;
    let text = format!("удар {an}");
    if check_block(world, d, Some(a), AttackKind::Unarmed, damage, &text) {
        return;
    }
    let base = zone_selected(world, a);
    let mut zone = {
        let mut rng = world.resource_mut::<crate::combat::CombatRng>();
        crate::medical::random_valid_zone(&mut rng, base, 80.0)
    };
    let mut combo = Combo::None;
    if honor_check(world, style, d) {
        let strength = if style == Style::EvilBoxing { 2.0 } else { 0.0 };
        let blind = crate::vision::is_blind(world, a);
        let behind = behind(world, a, d);
        combo = self::combo(
            &streak(world, a),
            arm == BodyZone::LArm,
            behind,
            honor,
            blind,
        );
        if combo != Combo::None {
            reset_streak(world, a);
        }
        if combo.slow() {
            let now = world.resource::<Time>().elapsed_secs_f64();
            world
                .entity_mut(a)
                .insert(NextMove(now + CLICK_CD_MELEE * 1.5));
        }
        match combo {
            Combo::Uppercut => effectiveness *= 1.5,
            Combo::Craven => {
                wound_bonus = Some(damage * 1.5);
                zone = {
                    let mut rng = world.resource_mut::<crate::combat::CombatRng>();
                    crate::medical::random_valid_zone(&mut rng, BodyZone::Head, 80.0)
                };
                stagger(world, d);
            }
            _ => {}
        }
        damage += (strength * combo.multiplier()).round();
        if combo.multiplier() >= 1.0 {
            extra_effect(world, style, d);
        }
    }
    let armor = melee_armor(world, d, zone, effectiveness);
    play(world, d, ss13_mapload::sound::files::PUNCH, 25.0);
    let verb = combo.verb();
    message(
        world,
        a,
        d,
        COMBAT,
        (
            Span::Danger,
            format!("{} наносит {verb} по {dn}!", capitalize(&an)),
        ),
        format!("Вы получаете {verb} от {an}!"),
        Some((Span::Danger, format!("Вы нанесли {verb} по {dn}!"))),
    );
    let dealt = crate::armor::blocked_damage(damage, armor);
    hurt(world, d, Some(zone), damage_type, dealt, wound_bonus);
    if super::stat(world, d) == Stat::Dead || !honor_check(world, style, d) {
        return;
    }
    // Knockout: the attacker's `unarmed_effectiveness` (no athletics on either side).
    let knockout = effectiveness.round().clamp(0.0, 100.0);
    if !chance(world, knockout) {
        return;
    }
    crit(world, style, a, d, armor, dealt);
    play(world, d, "sound/effects/coin2.ogg", 40.0);
}

fn stagger(world: &mut World, mob: Entity) {
    let dead = super::stat(world, mob) == Stat::Dead;
    if let Some(mut e) = effects_mut(world, mob) {
        e.adjust_staggered_up_to(STAGGER, STAGGER_MAX, dead);
    }
}

/// `crit_effect`: a staggered target is knocked out, another staggered; one held in an
/// aggressive grab flies two tiles. Hunter boxing crushes a beast instead.
fn crit(world: &mut World, style: Style, a: Entity, d: Entity, armor: f32, damage: f32) {
    let (an, dn) = (name(world, a), name(world, d));
    if style == Style::HunterBoxing && beast(world, d) {
        message(
            world,
            a,
            d,
            COMBAT,
            (
                Span::Danger,
                format!(
                    "{} полностью выбивает всю дурь из {dn} ужасающим ударом!!!",
                    capitalize(&an)
                ),
            ),
            format!("Из вас полностью выбивают всю дурь ударом от {an}!!!"),
            Some((
                Span::Danger,
                format!("Вы полностью выбиваете всю дурь из {dn} своим ужасающим ударом!!!"),
            )),
        );
        let doubled = crate::armor::blocked_damage(damage * 2.0, armor);
        hurt(
            world,
            d,
            Some(BodyZone::Chest),
            DamageType::Brute,
            doubled,
            Some(0.0),
        );
        // `heal_overall_damage(healing, healing, healing)`.
        // shortcut: healed on the chest only, no stamina; spread with `heal_overall_damage`.
        let healing = (damage / 3.0).round();
        if let Some(mut health) = world.get_mut::<Health>(a) {
            health.heal_zone(BodyZone::Chest, healing, healing);
        }
        return;
    }
    if effects(world, d).staggered > 0.0 {
        message(
            world,
            a,
            d,
            COMBAT,
            (
                Span::Danger,
                format!("{} вырубает {dn} хеймейкером!", capitalize(&an)),
            ),
            format!("Вас вырубает {an}!"),
            Some((Span::Danger, format!("Вы вырубаете {dn} хеймейкером!"))),
        );
        incap(world, d, Incap::Knockdown, super::armored(20.0, armor));
        super::put_to_sleep(world, d, 10.0);
    } else {
        message(
            world,
            a,
            d,
            COMBAT,
            (
                Span::Danger,
                format!("{} ошеломляет {dn} хеймейкером!", capitalize(&an)),
            ),
            format!("Вас почти сбивает с ног удар {an}!"),
            Some((Span::Danger, format!("Вы ошеломляете {dn} хеймейкером!"))),
        );
        stagger(world, d);
    }
    let grabbed = world.get::<Pulling>(a).map(|p| p.0) == Some(d)
        && world
            .get::<GrabState>(a)
            .is_some_and(|g| *g >= GrabState::Aggressive);
    if grabbed && let Some(dir) = dir_to(world, a, d) {
        fling(world, d, dir, 2, 2.0);
    }
}

fn incap(world: &mut World, mob: Entity, kind: Incap, secs: f32) {
    super::incap(world, mob, kind, secs);
}

/// `check_block`: in throw mode, a punch by a boxer is blocked with the active arm's
/// `unarmed_effectiveness` percent; an adjacent attacker takes 10 back, the boxer 5 stamina.
pub(super) fn block(
    world: &mut World,
    style: Style,
    me: Entity,
    attacker: Option<Entity>,
    kind: AttackKind,
    _damage: f32,
    attack_text: &str,
) -> bool {
    if !throw_mode(world, me) || incapacitated(world, me) || kind != AttackKind::Unarmed {
        return false;
    }
    let Some(attacker) = attacker else {
        return false;
    };
    if active_arm(world, me).is_none() || !honor_check(world, style, attacker) {
        return false;
    }
    if !chance(world, ARM.effectiveness) {
        return false;
    }
    if adjacent(world, me, attacker) {
        hurt(
            world,
            attacker,
            None,
            default_damage(style),
            10.0,
            Some(0.0),
        );
        hurt(world, me, None, DamageType::Stamina, 5.0, None);
        extra_effect(world, style, attacker);
    }
    let dodged = chance(world, 50.0);
    let verb = if dodged {
        "избегает"
    } else {
        "блокирует"
    };
    let mn = name(world, me);
    message(
        world,
        me,
        me,
        super::NEAR,
        (
            Span::Danger,
            format!("{} {verb} {attack_text}!", capitalize(&mn)),
        ),
        format!("Вы {verb}е {attack_text}!"),
        None,
    );
    if dodged {
        play(world, me, ss13_mapload::sound::files::PUNCH_MISS, 25.0);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combos_follow_the_clicks_and_the_hand() {
        assert_eq!(combo("HH", true, false, true, false), Combo::Straight);
        assert_eq!(combo("HH", false, false, true, false), Combo::LightJab);
        assert_eq!(combo("DD", true, false, true, true), Combo::BlindJab);
        assert_eq!(combo("HD", true, false, true, false), Combo::LeftHook);
        assert_eq!(combo("DH", false, false, true, false), Combo::RightHook);
        assert_eq!(combo("DH", true, false, true, false), Combo::Uppercut);
        assert_eq!(combo("H", true, false, true, false), Combo::None);
        assert_eq!(combo("H", true, true, true, false), Combo::None, "honor");
        assert_eq!(combo("H", true, true, false, false), Combo::Craven);
    }
}
