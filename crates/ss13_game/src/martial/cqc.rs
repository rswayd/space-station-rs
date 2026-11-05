//! CQC (tg `code/datums/martial/cqc.dm`, BandaStation's translation): slam, kick, restrain,
//! pressure and consecutive combos, a sweep from the floor, an instant aggressive grab, a
//! disarming jab, the chokehold of a restrained mob, and blocking in throw mode.
// shortcut: no neck snap (needs a kill grab; `crate::pull` stops at aggressive), no chokehold neck
// grab, no jitter, no touch spell redirection (`on_attackby`), and no chef's "close quarters
// cooking" (`under_siege`, its skillchip and kitchen areas); add them with neck grabs, jitter and
// skillchips.

use bevy::prelude::*;
use ss13_shared::chat::Span;

use super::{
    AttackKind, COMBAT, NEAR, Outcome, add_to_streak, adjacent, capitalize, chance, check_block,
    dir_to, drop_held, fling, hurt, incap, incapacitated, knock_out, lying, melee_armor, message,
    name, play, reset_streak, stamina_loss, streak, take_active_item, throw_mode, unconscious,
    unconscious_or_crit, with_art,
};
use crate::health::{DamageType, Stat};
use crate::pull::{GrabState, PullSlowdown, Pulling};
use crate::status_effects::Incap;
use crate::zone::BodyZone;

const SLAM: &str = "GH";
const KICK: &str = "HH";
const RESTRAIN: &str = "GG";
const PRESSURE: &str = "DG";
const CONSECUTIVE: &str = "DDH";

/// `block_chance`.
const BLOCK_CHANCE: f32 = 75.0;

pub(super) const HELP: &[&str] = &[
    "Вы пытаетесь вспомнить некоторые основы CQC.",
    "Бросок: Захват, Удар. Впечатайте оппонента в землю, опрокидывая его.",
    "CQC пинок: Удар, Удар. Отбросьте оппонента от себя. Отбрасывание оглушённого противника наносит урон выносливости.",
    "Сдерживание: Захват, Захват. Удерживает в захвате и обезоруживает оппонента, чтобы вырубить его удушающим приёмом.",
    "Давление: Толчок, Захват. Значительный урон по выносливости.",
    "Последовательный CQC: Толчок, Толчок, Удар. Основной атакующий приём, наносящий огромный урон и значительный урон выносливости.",
    "В дополнение, включив режим броска при нападении, вы переходите в режим активной защиты, где у вас есть шанс заблокировать удары противника, а иногда даже провести контратаку.",
];

pub(super) const REMARKS: &[&str] = &[
    "Пинок... Бросок...",
    "Захват... Пинок...",
    "Наносите удары по животу, шее и спине, чтобы нанести критический урон...",
    "Бросок... Захват...",
    "Я, наверное, мог бы совместить это с какими-нибудь другими боевыми искусствами! ...Подождите, это же незаконно.",
    "Слова которые убивают...",
    "Последний, завершающий миг ваш...",
];

const HIT1: &str = "sound/items/weapons/cqchit1.ogg";
const HIT2: &str = "sound/items/weapons/cqchit2.ogg";

/// The mob the attacker restrains (`restraining_mob`), while its 5 s last.
fn restraining(world: &World, a: Entity) -> Option<Entity> {
    let now = world.resource::<Time>().elapsed_secs_f64();
    world
        .get::<super::MartialArts>(a)
        .and_then(|m| m.0.first())
        .and_then(|art| art.restraining)
        .filter(|&(_, until)| now < until)
        .map(|(m, _)| m)
}

/// `check_streak`.
fn check_streak(world: &mut World, a: Entity, d: Entity) -> bool {
    let streak = streak(world, a);
    let combos: [(&str, fn(&mut World, Entity, Entity) -> bool); 5] = [
        (SLAM, slam),
        (KICK, kick),
        (RESTRAIN, restrain),
        (PRESSURE, pressure),
        (CONSECUTIVE, consecutive),
    ];
    for (combo, finisher) in combos {
        if streak.contains(combo) {
            reset_streak(world, a);
            return finisher(world, a, d);
        }
    }
    false
}

fn slam(world: &mut World, a: Entity, d: Entity) -> bool {
    if lying(world, d) {
        return false;
    }
    let (an, dn) = (name(world, a), name(world, d));
    message(
        world,
        a,
        d,
        NEAR,
        (Span::Danger, format!("{an} швыряет {dn} на землю!")),
        format!("{} швыряет вас на землю!", capitalize(&an)),
        Some((Span::Danger, format!("Вы швыряете {dn} на землю!"))),
    );
    play(world, a, "sound/items/weapons/slam.ogg", 50.0);
    hurt(world, d, None, DamageType::Brute, 10.0, Some(0.0));
    incap(world, d, Incap::Paralyze, 12.0);
    true
}

fn kick(world: &mut World, a: Entity, d: Entity) -> bool {
    if unconscious_or_crit(world, d) {
        return false;
    }
    let (an, dn) = (name(world, a), name(world, d));
    if lying(world, d) && !unconscious(world, d) && stamina_loss(world, d) >= 100.0 {
        message(
            world,
            a,
            d,
            NEAR,
            (
                Span::Danger,
                format!("{an} пинает голову {dn}, выводя его из сознания!"),
            ),
            format!("Вы потеряли сознание после пинка {an}!"),
            Some((
                Span::Danger,
                format!("Вы пинаете голову {dn}, выводя его из сознания!"),
            )),
        );
        play(world, a, "sound/items/weapons/genhit1.ogg", 50.0);
        let helmet = melee_armor(world, d, BodyZone::Head, 0.0);
        incap(world, d, Incap::Knockdown, super::armored(20.0, helmet));
        knock_out(world, d, super::armored(10.0, helmet));
        crate::medical::organs::adjust_organ_loss_up_to(
            world,
            d,
            crate::medical::organs::Slot::Brain,
            15.0,
            150.0,
        );
        return true;
    }
    message(
        world,
        a,
        d,
        COMBAT,
        (
            Span::Danger,
            format!("{} отбрасывает {dn} назад!", capitalize(&an)),
        ),
        format!("Вы были отброшены {an}!"),
        Some((Span::Danger, format!("Вы отбрасываете {dn} назад!"))),
    );
    play(world, a, HIT1, 50.0);
    let dir = world
        .get::<ss13_shared::Facing>(a)
        .map(|f| f.0)
        .or_else(|| dir_to(world, a, d));
    if let Some(dir) = dir
        && a != d
    {
        fling(world, d, dir, 1, 14.0);
    }
    hurt(world, d, None, DamageType::Brute, 10.0, Some(0.0));
    if lying(world, d) && !unconscious(world, d) {
        super::hurt(world, d, None, DamageType::Stamina, 45.0, None);
    }
    true
}

fn pressure(world: &mut World, a: Entity, d: Entity) -> bool {
    let (an, dn) = (name(world, a), name(world, d));
    message(
        world,
        a,
        d,
        COMBAT,
        (Span::Danger, format!("{an} бьёт {dn} в шею!")),
        format!("Вас бьёт в шею {an}!"),
        Some((Span::Danger, format!("Вы бьёте {dn} в шею!"))),
    );
    hurt(world, d, None, DamageType::Stamina, 60.0, None);
    play(world, a, HIT1, 50.0);
    true
}

fn restrain(world: &mut World, a: Entity, d: Entity) -> bool {
    if restraining(world, a).is_some() || unconscious_or_crit(world, d) {
        return false;
    }
    let (an, dn) = (name(world, a), name(world, d));
    message(
        world,
        a,
        d,
        NEAR,
        (Span::Warning, format!("{an} берёт {dn} в захват!")),
        format!("{} берёт вас в захват!", capitalize(&an)),
        Some((Span::Danger, format!("Вы берёте {dn} в захват!"))),
    );
    hurt(world, d, None, DamageType::Stamina, 20.0, None);
    incap(world, d, Incap::Stun, 10.0);
    let now = world.resource::<Time>().elapsed_secs_f64();
    with_art(world, a, |art| art.restraining = Some((d, now + 5.0)));
    true
}

fn consecutive(world: &mut World, a: Entity, d: Entity) -> bool {
    if unconscious_or_crit(world, d) {
        return false;
    }
    let (an, dn) = (name(world, a), name(world, d));
    message(
        world,
        a,
        d,
        COMBAT,
        (
            Span::Danger,
            format!("{an} последовательно наносит удары в живот, шею и спину {dn}."),
        ),
        format!("Вы чувствуете, как {an} наносит последовательные удары в ваш живот, шею и спину!"),
        Some((
            Span::Danger,
            format!("Вы наносите последовательные удары по животу, шее и спине {dn}!"),
        )),
    );
    play(world, d, HIT2, 50.0);
    take_active_item(world, d, a);
    hurt(world, d, None, DamageType::Stamina, 50.0, None);
    hurt(world, d, None, DamageType::Brute, 25.0, Some(0.0));
    true
}

pub(super) fn grab(world: &mut World, a: Entity, d: Entity) -> Outcome {
    if a == d {
        return Outcome::Invalid;
    }
    let text = name(world, a);
    if check_block(world, d, Some(a), AttackKind::Unarmed, 0.0, &text) {
        return Outcome::Fail;
    }
    add_to_streak(world, a, "G", d);
    if check_streak(world, a, d) {
        return Outcome::Success;
    }
    if lying(world, a) {
        return Outcome::Invalid;
    }
    // `grabbedby(attacker, TRUE)` then, from a passive grab (or none), aggressive at once.
    let pulling = world.get::<Pulling>(a).map(|p| p.0) == Some(d);
    let old = if pulling {
        world.get::<GrabState>(a).copied().unwrap_or_default()
    } else {
        GrabState::Passive
    };
    if !pulling {
        world
            .entity_mut(a)
            .insert((Pulling(d), GrabState::Passive, PullSlowdown(0.0)));
    }
    if old == GrabState::Passive {
        drop_held(world, d);
        world.entity_mut(a).insert(GrabState::Aggressive);
        // `grippedby`: the victim lets go of what it pulls.
        world.entity_mut(d).remove::<Pulling>();
        let (an, dn) = (name(world, a), name(world, d));
        message(
            world,
            a,
            d,
            COMBAT,
            (Span::Warning, format!("{an} яростно хватает {dn}!")),
            format!("Вас яростно хватает {an}!"),
            Some((Span::Danger, format!("Вы яростно хватаете {dn}!"))),
        );
    }
    Outcome::Success
}

pub(super) fn harm(world: &mut World, a: Entity, d: Entity) -> Outcome {
    let text = name(world, a);
    if check_block(world, d, Some(a), AttackKind::Unarmed, 10.0, &text) {
        return Outcome::Fail;
    }
    let (an, dn) = (name(world, a), name(world, d));
    let resting = crate::status_effects::incapacitation(world, a).resting;
    if resting && super::stat(world, d) != Stat::Dead && !lying(world, d) {
        message(
            world,
            a,
            d,
            NEAR,
            (Span::Danger, format!("{an} делает подсечку {dn}!")),
            format!("{} делает вам подсечку!", capitalize(&an)),
            Some((Span::Danger, format!("Вы делаете подсечку {dn}!"))),
        );
        play(world, a, "sound/effects/hit_kick.ogg", 50.0);
        hurt(world, d, None, DamageType::Brute, 10.0, Some(0.0));
        incap(world, d, Incap::Knockdown, 5.0);
        reset_streak(world, a);
        return Outcome::Success;
    }
    add_to_streak(world, a, "H", d);
    if check_streak(world, a, d) {
        return Outcome::Success;
    }
    let lying_target = lying(world, d);
    let damage = if lying_target { 18.0 } else { 13.0 };
    hurt(world, d, None, DamageType::Brute, damage, Some(0.0));
    let kicked = chance(world, 50.0);
    let (others, to_defender, to_attacker, sound) = match (lying_target, kicked) {
        (true, true) => (
            format!("{an} пинает {dn}!"),
            format!("{} пнул вас!", capitalize(&an)),
            format!("Вы пинаете {dn}!"),
            HIT2,
        ),
        (true, false) => (
            format!("{an} топчет {dn}!"),
            format!("{} растоптал вас!", capitalize(&an)),
            format!("Вы топчете {dn}!"),
            HIT2,
        ),
        // BandaStation's translation lost the line of a standing hit; tg prints one.
        (false, _) => {
            let hit = if kicked {
                "CQC"
            } else {
                "Большой Босс"
            };
            (
                format!("{an} бьёт {dn} приёмом «{hit}»!"),
                format!("{} бьёт вас приёмом «{hit}»!", capitalize(&an)),
                format!("Вы бьёте {dn} приёмом «{hit}»!"),
                HIT1,
            )
        }
    };
    play(world, d, sound, 50.0);
    message(
        world,
        a,
        d,
        COMBAT,
        (Span::Danger, others),
        to_defender,
        Some((Span::Danger, to_attacker)),
    );
    Outcome::Success
}

pub(super) fn disarm(world: &mut World, a: Entity, d: Entity) -> Outcome {
    let text = name(world, a);
    if check_block(world, d, Some(a), AttackKind::Unarmed, 0.0, &text) {
        return Outcome::Fail;
    }
    add_to_streak(world, a, "D", d);
    if check_streak(world, a, d) {
        return Outcome::Success;
    }
    let (an, dn) = (name(world, a), name(world, d));
    let restrained = restraining(world, a);
    if restrained.is_some() && world.get::<Pulling>(a).map(|p| p.0) == restrained {
        message(
            world,
            a,
            d,
            NEAR,
            (
                Span::Danger,
                format!("{an} берёт {dn} на удушающий захват!"),
            ),
            format!("{} берёт вас на удушающий захват!", capitalize(&an)),
            Some((Span::Danger, format!("Вы берёте {dn} в удушающий захват!"))),
        );
        // `SetSleeping(40 SECONDS)`.
        knock_out(world, d, 40.0);
        with_art(world, a, |art| art.restraining = None);
        return Outcome::Success;
    }
    // tg's condition `!crit || !paralyzed || !restraining` is true unless all three hold.
    let paralyzed = super::effects(world, d).paralyze > 0.0;
    if chance(world, 65.0) && (!unconscious_or_crit(world, d) || !paralyzed || restrained.is_none())
    {
        let item = world
            .get::<crate::inventory::Inventory>(d)
            .and_then(crate::inventory::Inventory::active_item)
            .filter(|&i| !crate::inventory::no_drop(world, i));
        let item_name = item.map(|i| name(world, i));
        if item.is_some() {
            take_active_item(world, d, a);
        }
        let (others, to_defender, to_attacker) = match &item_name {
            Some(i) => (
                format!("{an} бьёт {dn} в челюсть рукой, выбивая из его рук {i}!"),
                format!("{an} бьёт вам в челюсть, выбивая из ваших рук {i} и дезориентируя вас!"),
                format!(
                    "Вы бьёте в челюсть {dn}, выбивая из его рук {i} и оставляя его дезориентированным!"
                ),
            ),
            None => (
                format!("{an} бьёт {dn} в челюсть рукой!"),
                format!("{an} бьёт вам в челюсть, дезориентируя вас!"),
                format!("Вы бьёте в челюсть {dn}, оставляя его дезориентированным!"),
            ),
        };
        message(
            world,
            a,
            d,
            COMBAT,
            (Span::Danger, others),
            to_defender,
            Some((Span::Danger, to_attacker)),
        );
        play(world, d, HIT1, 50.0);
        hurt(world, d, None, DamageType::Brute, 5.0, Some(0.0));
        return Outcome::Success;
    }
    message(
        world,
        a,
        d,
        COMBAT,
        (Span::Danger, format!("У {an} не выходит разоружить {dn}!")),
        format!("{} почти удалось вас разоружить!", capitalize(&an)),
        Some((Span::Warning, format!("У вас не выходит разоружить {dn}!"))),
    );
    play(world, d, "sound/items/weapons/punchmiss.ogg", 25.0);
    Outcome::Fail
}

/// `check_block`: in throw mode, 75% to block a melee or unarmed attack; an adjacent attacker
/// gets its arm twisted (stunned 4 s).
pub(super) fn block(
    world: &mut World,
    me: Entity,
    attacker: Option<Entity>,
    kind: AttackKind,
    attack_text: &str,
) -> bool {
    if !throw_mode(world, me) || incapacitated(world, me) || kind == AttackKind::Projectile {
        return false;
    }
    if !chance(world, BLOCK_CHANCE) {
        return false;
    }
    let mn = name(world, me);
    match attacker {
        Some(attacker) if adjacent(world, me, attacker) => {
            let atn = name(world, attacker);
            message(
                world,
                me,
                me,
                NEAR,
                (
                    Span::Danger,
                    format!(
                        "{} блокирует {atn} и выкручивает руку {atn} за его спиной!",
                        capitalize(&mn)
                    ),
                ),
                format!("Вы блокируете {attack_text}!"),
                None,
            );
            incap(world, attacker, Incap::Stun, 4.0);
        }
        _ => {
            let others = format!("{} блокирует {attack_text}!", capitalize(&mn));
            message(
                world,
                me,
                me,
                NEAR,
                (Span::Danger, others),
                format!("Вы блокируете {attack_text}!"),
                None,
            );
        }
    }
    true
}
