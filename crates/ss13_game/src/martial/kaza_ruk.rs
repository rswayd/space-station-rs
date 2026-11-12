//! Kaza Ruk (tg `code/datums/martial/kaza_ruk.dm`): the Tiziran style of the Kaza Ruk gloves
//! and implant. Three buttons ready a one-shot move for the next punch or shove (neck chop,
//! low sweep, lung punch); a shove takes the target's held item 60% of the time; half of the
//! landed punches get a follow-up strike of a third of their damage 0.25 s later.
// shortcut: the lizard's tail variants follow the species (no tail organ is ported), no spin or
// flip emote, no attack animation; `grab_damage_modifier`/`grab_escape_chance_modifier` are not
// ported (no grab damage). Add them with tail organs and grab damage.

use bevy::prelude::*;
use ss13_shared::chat::Span;

use super::{
    AttackKind, COMBAT, NEAR, Outcome, check_block, hurt, incap, message, name, play, streak,
    take_active_item, tell_mob, with_art,
};
use crate::health::DamageType;
use crate::status_effects::{Incap, Timed};
use crate::zone::BodyZone;

const NECK_CHOP: &str = "neck_chop";
const LOW_SWEEP: &str = "low_sweep";
/// tg's internal name of the lung punch.
const QUICK_CHOKE: &str = "quick_choke";

/// The move buttons: (id, name, `actions_items.dmi` state, streak).
pub(super) const MOVES: [(&str, &str, &str, &str); 3] = [
    (
        "martial:neck_chop",
        "Удар в шею — повреждает горло, жертва на время теряет голос.",
        "neckchop",
        NECK_CHOP,
    ),
    (
        "martial:low_sweep",
        "Подсечка — сбивает жертву с ног на короткое время.",
        "legsweep",
        LOW_SWEEP,
    ),
    (
        "martial:lung_punch",
        "Удар в лёгкие — сильный удар над животом, жертва ненадолго не может дышать.",
        "lungpunch",
        QUICK_CHOKE,
    ),
];

/// Gloves that teach Kaza Ruk (`/obj/item/clothing/gloves/kaza_ruk` and subtypes).
pub(super) const GLOVES: &str = "/obj/item/clothing/gloves/kaza_ruk";

/// A follow-up strike waiting for its 0.25 s (`blow_followup`'s timer).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct KazaFollowups(pub Vec<Followup>);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Followup {
    pub at: f64,
    pub target: Entity,
    pub zone: BodyZone,
    /// The punch's damage before armor.
    pub damage: f32,
    pub armor: f32,
    pub kick: bool,
}

/// `COMSIG_HUMAN_PUNCHED` of a Kaza Ruk practitioner: half the time a follow-up is queued.
pub fn on_punched(
    world: &mut World,
    attacker: Entity,
    target: Entity,
    zone: BodyZone,
    damage: f32,
    armor: f32,
    kick: bool,
) {
    if super::active_style(world, attacker) != Some(super::Style::KazaRuk)
        || !super::chance(world, 50.0)
    {
        return;
    }
    let at = world.resource::<Time>().elapsed_secs_f64() + 0.25;
    let mut queue = world
        .get::<KazaFollowups>(attacker)
        .cloned()
        .unwrap_or_default();
    queue.0.push(Followup {
        at,
        target,
        zone,
        damage,
        armor,
        kick,
    });
    world.entity_mut(attacker).insert(queue);
}

/// Runs the follow-ups that are due.
// Cost: per tick, the few mobs with a queued follow-up.
pub(super) fn run_followups(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &KazaFollowups)>();
    let due: Vec<(Entity, Vec<Followup>)> = q
        .iter(world)
        .map(|(e, f)| (e, f.0.iter().copied().filter(|f| f.at <= now).collect()))
        .filter(|(_, d): &(Entity, Vec<Followup>)| !d.is_empty())
        .collect();
    for (source, list) in due {
        let emptied = world
            .get_mut::<KazaFollowups>(source)
            .is_some_and(|mut f| {
                f.0.retain(|f| f.at > now);
                f.0.is_empty()
            });
        if emptied {
            world.entity_mut(source).remove::<KazaFollowups>();
        }
        for f in list {
            execute_followup(world, source, f);
        }
    }
}

/// `execute_followup`: checked again after the delay (both still here, still adjacent).
fn execute_followup(world: &mut World, source: Entity, f: Followup) {
    if world.get_entity(f.target).is_err() || !super::adjacent(world, source, f.target) {
        return;
    }
    let tail = has_tail(world, source);
    let strike = match (f.kick, tail) {
        (true, true) => "рассекающий удар хвостом",
        (true, false) => "рубящий удар ногой",
        (false, true) => "хлёсткий разворот с хвостом",
        (false, false) => "удар локтем",
    };
    let (sn, tn) = (name(world, source), name(world, f.target));
    let seen = format!("{sn} добавляет {strike} по {tn}!");
    message(
        world,
        source,
        f.target,
        NEAR,
        (Span::Warning, seen.clone()),
        seen,
        Some((Span::Notice, format!("Вы наносите {strike} по {tn}!"))),
    );
    play(world, source, "sound/effects/hit_punch.ogg", 50.0);
    // `round(damage/3, 1)` through the punch's armor block, `wound_bonus = damage`.
    let amount = crate::armor::blocked_damage((f.damage / 3.0).round(), f.armor);
    hurt(
        world,
        f.target,
        Some(f.zone),
        DamageType::Brute,
        amount,
        Some(f.damage),
    );
}

/// A lizard's tail sweeps and strikes instead of the leg.
fn has_tail(world: &World, mob: Entity) -> bool {
    use crate::species::SpeciesId;
    world.get::<crate::inventory::Inventory>(mob).is_some()
        && matches!(
            crate::species::species_of(world, mob),
            SpeciesId::Lizard | SpeciesId::Ashwalker | SpeciesId::Silverscale
        )
}

/// A move button: ready the move, or clear it.
pub(super) fn trigger(world: &mut World, mob: Entity, id: &str) {
    let Some(&(_, _, _, combo)) = MOVES.iter().find(|(m, ..)| *m == id) else {
        return;
    };
    // `AB_CHECK_INCAPACITATED|AB_CHECK_HANDS_BLOCKED|AB_CHECK_CONSCIOUS`.
    if super::incapacitated(world, mob)
        || super::effects(world, mob).hands_blocked()
        || super::stat(world, mob) != crate::health::Stat::Conscious
    {
        return;
    }
    let mn = name(world, mob);
    let ready = streak(world, mob) == combo;
    let (others, own, alert) = if ready {
        (
            format!("{mn} встаёт в нейтральную стойку."),
            "Ваша следующая атака отменена.".to_owned(),
            "отменено",
        )
    } else {
        let (stance, alert) = match combo {
            NECK_CHOP => ("удара в шею", "удар в шею"),
            LOW_SWEEP => ("подсечки", "подсечка"),
            _ => ("удара в лёгкие", "удар в лёгкие"),
        };
        (
            format!("{mn} встаёт в стойку {stance}!"),
            format!("Ваша следующая атака — {alert}."),
            alert,
        )
    };
    message(world, mob, mob, NEAR, (Span::Danger, others), own, None);
    crate::balloon::balloon_alert(world, mob, mob, alert);
    with_art(world, mob, |art| {
        art.streak = if ready {
            String::new()
        } else {
            combo.to_owned()
        };
    });
}

/// `check_streak`: the readied move, cleared first; true for any readied move (tg ignores the
/// move's own result).
fn check_streak(world: &mut World, a: Entity, d: Entity) -> bool {
    let combo = streak(world, a);
    if ![NECK_CHOP, LOW_SWEEP, QUICK_CHOKE].contains(&combo.as_str()) {
        return false;
    }
    with_art(world, a, |art| art.streak.clear());
    match combo.as_str() {
        NECK_CHOP => neck_chop(world, a, d),
        LOW_SWEEP => low_sweep(world, a, d),
        _ => lung_punch(world, a, d),
    };
    true
}

fn pacifist(world: &World, mob: Entity) -> bool {
    world
        .get::<crate::medical::traumas::Pacifist>(mob)
        .is_some()
}

fn low_sweep(world: &mut World, a: Entity, d: Entity) -> Outcome {
    if super::unconscious_or_crit(world, d) || super::effects(world, d).paralyze > 0.0 {
        return Outcome::Invalid;
    }
    if pacifist(world, a) {
        return Outcome::Invalid;
    }
    let what = if has_tail(world, a) {
        "хвостом"
    } else {
        "ногой"
    };
    let (an, dn) = (name(world, a), name(world, d));
    message(
        world,
        a,
        d,
        NEAR,
        (Span::Warning, format!("{an} делает подсечку {what} {dn}!")),
        format!("{an} подсекает вам ноги!"),
        Some((Span::Danger, format!("Вы делаете подсечку {what} {dn}!"))),
    );
    play(world, a, "sound/effects/hit_kick.ogg", 50.0);
    hurt(
        world,
        d,
        Some(BodyZone::Chest),
        DamageType::Brute,
        5.0,
        Some(0.0),
    );
    incap(world, d, Incap::Knockdown, 6.0);
    Outcome::Success
}

fn lung_punch(world: &mut World, a: Entity, d: Entity) -> Outcome {
    let (an, dn) = (name(world, a), name(world, d));
    message(
        world,
        a,
        d,
        COMBAT,
        (Span::Warning, format!("{an} бьёт {dn} в грудь!")),
        format!("{an} бьёт вас в грудь! Вы не можете дышать!"),
        Some((Span::Danger, format!("Вы бьёте {dn} в грудь!"))),
    );
    play(world, a, "sound/effects/hit_punch.ogg", 50.0);
    if let Some(mut r) = world.get_mut::<crate::breath::Respiration>(d)
        && r.losebreath <= 10.0
    {
        r.losebreath = (r.losebreath + 5.0).clamp(0.0, 10.0);
    }
    hurt(world, d, None, DamageType::Oxy, 10.0, None);
    Outcome::Success
}

fn neck_chop(world: &mut World, a: Entity, d: Entity) -> Outcome {
    if pacifist(world, a) {
        return Outcome::Invalid;
    }
    let (an, dn) = (name(world, a), name(world, d));
    message(
        world,
        a,
        d,
        COMBAT,
        (
            Span::Warning,
            format!("{an} бьёт ребром ладони по шее {dn}!"),
        ),
        format!("{an} бьёт вас ребром ладони по шее, вы не можете говорить!"),
        Some((
            Span::Danger,
            format!("Вы бьёте ребром ладони по шее {dn}, лишая голоса!"),
        )),
    );
    play(world, a, "sound/effects/hit_punch.ogg", 50.0);
    hurt(
        world,
        d,
        Some(BodyZone::Head),
        DamageType::Brute,
        10.0,
        Some(0.0),
    );
    crate::status_effects::adjust_timed(world, d, Timed::Silence, 20.0, Some(20.0));
    Outcome::Success
}

pub(super) fn harm(world: &mut World, a: Entity, d: Entity) -> Outcome {
    let text = name(world, a);
    if check_block(world, d, Some(a), AttackKind::Unarmed, 10.0, &text) {
        return Outcome::Fail;
    }
    if check_streak(world, a, d) {
        return Outcome::Success;
    }
    Outcome::Invalid
}

pub(super) fn disarm(world: &mut World, a: Entity, d: Entity) -> Outcome {
    let text = name(world, a);
    if check_block(world, d, Some(a), AttackKind::Unarmed, 0.0, &text) {
        return Outcome::Fail;
    }
    if check_streak(world, a, d) {
        return Outcome::Success;
    }
    let holds = world
        .get::<crate::inventory::Inventory>(d)
        .and_then(crate::inventory::Inventory::active_item)
        .is_some();
    if holds && super::chance(world, 60.0) && take_active_item(world, d, a).is_some() {
        let (an, dn) = (name(world, a), name(world, d));
        message(
            world,
            a,
            d,
            COMBAT,
            (Span::Danger, format!("{an} обезоруживает {dn}!")),
            format!("{an} обезоруживает вас!"),
            Some((Span::Danger, format!("Вы обезоруживаете {dn}!"))),
        );
        play(world, d, "sound/items/weapons/thudswoosh.ogg", 50.0);
    }
    // The normal shove follows.
    Outcome::Invalid
}

/// `activate_style`/`deactivate_style` lines.
pub(super) fn greet(world: &mut World, mob: Entity, active: bool) {
    if active {
        tell_mob(
            world,
            mob,
            Span::BoldWarning,
            "Вы знаете искусство Каза Рук!",
        );
        tell_mob(
            world,
            mob,
            Span::Danger,
            "Наведите курсор на приём вверху экрана, чтобы узнать, что он делает.",
        );
    } else {
        tell_mob(
            world,
            mob,
            Span::BoldWarning,
            "Вы внезапно забываете искусство Каза Рук...",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_map_to_tg_streaks() {
        let streaks: Vec<&str> = MOVES.iter().map(|m| m.3).collect();
        assert_eq!(streaks, ["neck_chop", "low_sweep", "quick_choke"]);
        assert!(MOVES.iter().all(|m| m.0.starts_with(super::super::BUTTON)));
    }
}
