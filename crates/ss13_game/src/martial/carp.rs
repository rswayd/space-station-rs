//! The sleeping carp (tg `code/datums/martial/sleeping_carp.dm`): wrist wrench, launch kick,
//! drop kick and knee to the stomach; stamina-heavy grabs and shoves in combat mode; projectiles
//! deflected and (in martial or carp clothing, lightly armed) blows dodged in combat mode; no
//! guns (`TRAIT_NOGUNS`, `crate::guns`), punches never miss (`TRAIT_PERFECT_ATTACKER`,
//! `crate::combat`) and grabs harder to resist (`grab_state_modifier`, `crate::pull`).
// shortcut: no neck snap (no kill grab), no carp faction,
// no `TRAIT_HARDLY_WOUNDED`/`TRAIT_NODISMEMBER`/`TRAIT_HEAVY_SLEEPER`, no carp organ set bonus,
// no `TRAIT_WIELDED` or `block_chance` of held items, no hulk check, no touch spell dodge; add
// them with those statuses, factions and two-handed items.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmClothing, DmItem, DmWeight};
use ss13_shared::chat::Span;

use super::{
    AttackKind, COMBAT, NEAR, Outcome, add_to_streak, capitalize, chance, check_block, combat_mode,
    dir_to, drop_held, fling, hurt, incap, incapacitated, knock_out, lying, message, name, play,
    reset_streak, stamina_loss, streak, unconscious,
};
use crate::health::DamageType;
use crate::inventory::Inventory;
use crate::status_effects::Incap;
use crate::zone::BodyZone;

const WRIST_WRENCH: &str = "HG";
const LAUNCH_KICK: &str = "HD";
const DROP_KICK: &str = "DD";
const KNEE_STOMACH: &str = "GH";

/// `CARP_STYLE_FACTOR` of `clothing_flags`.
const CARP_STYLE_FACTOR: u32 = 1 << 13;
/// `WEIGHT_CLASS_SMALL`.
const WEIGHT_CLASS_SMALL: u8 = 2;

/// `exempt_objects`: held items that do not spoil the style.
const EXEMPT: [&str; 17] = [
    "/obj/item/melee/baton/nunchaku",
    "/obj/item/staff/bostaff",
    "/obj/item/bambostaff",
    "/obj/item/cane",
    "/obj/item/nullrod/bostaff",
    "/obj/item/knife",
    "/obj/item/restraints/legcuffs/bola",
    "/obj/item/storage/toolbox",
    "/obj/item/spear",
    "/obj/item/claymore/shortsword",
    "/obj/item/nullrod/nullblade",
    "/obj/item/extendohand",
    "/obj/item/mop",
    "/obj/item/pushbroom",
    "/obj/item/melee/baseball_bat",
    "/obj/item/crowbar/hammer",
    "/obj/item/toy/plush/carpplushie",
];

pub(super) const HELP: &[&str] = &[
    "Вы уходите в себя и вспоминаете учения Спящего Карпа...",
    "Скрежещущие зубы: Удар, Захват. Яростно выкручивайте руку противника, выворачивая или даже ломая кость, заставляя противника выронить удерживаемый предмет.",
    "Сокрушительный удар волны: Удар, Толчок. Оттолкните вашего противника с невероятной силой!",
    "Под килем: Толчок, Толчок. Нелетально пнуть противника на пол, сбивая его с ног, приводящий его в замешательство и наносящий существенный урон выносливости. Если противник уже лежит ничком, обезоруживает его.",
    "Крушение Кракена: Захват, Удар. Нанесите удар коленом по противнику, нанося высокий урон выносливости, ненадолго оглушая его, сбивая дыхание и затрудняя речь.",
    "Захваты и толчки: В боевом режиме обычные захваты и толчки наносят значительный урон выносливости, и от них труднее отбиться. Если вы захватываете кого-то, с низким уровнем выносливости, вы вырубаете его!",
    "Находясь в боевом режиме (и не будучи оглушенным, халком, и не находясь в мехе), вы можете отражать все снаряды, летящие в вашу сторону, посылая их обратно в тех, кто запустил их в вас!",
    "Однако ваша способность уклоняться от снарядов ухудшается, когда вы обременены броней или когда вы несёте в руках предметы обычного, или более крупного, размера.",
    "Помимо этого, ваше обучение привило вам отвращение к оружию, и вы более не способны им пользоваться.",
];

pub(super) const REMARKS: &[&str] = &[
    "Подождите, диета с высоким содержанием белка - это действительно всё, что нужно, чтобы стать устойчивым к колото-ножевым ранениям...?",
    "Непреодолимая сила, неподвижный объект...",
    "Сфокусируйтесь... И вы сможете вывести из строя любого противника за считанные секунды...",
    "Я должен пробить броню, чтобы нанести максимальный урон...",
    "Я не думаю, что это будет сочетаться с другими боевыми искусствами...",
    "Стать единым с карпом...",
    "Бульк...",
];

pub(super) const GREET: &str = "Вы овладели древним боевым искусством Спящего Карпа! Ваш рукопашный бой стал намного эффективнее, и теперь, находясь в боевом режиме, вы способны отражать любые снаряды, направленные на вас. Однако вы также теряете способность использовать любые виды дальнобойного оружия. Вы можете узнать больше о своем новообретенном искусстве с помощью кнопки «Вспомнить учения».";

const KICK_SOUND: &str = "sound/effects/hit_kick.ogg";
const PUNCH: &str = "sound/items/weapons/punch1.ogg";

/// The read scroll (`update_appearance` with no uses left).
pub(super) fn blank_scroll(world: &mut World, item: Entity) {
    super::rename(
        world,
        item,
        "empty scroll",
        "Свиток абсолютно пуст.",
        ("icons/obj/scrolls.dmi", "blankscroll"),
    );
}

/// `can_deflect`: combat mode, able to act, on a turf.
pub(super) fn can_deflect(world: &World, mob: Entity) -> bool {
    combat_mode(world, mob)
        && !incapacitated(world, mob)
        && !super::effects(world, mob).hands_blocked()
        && world.get::<crate::closets::InCloset>(mob).is_none()
        && world.get::<crate::mecha::Pilot>(mob).is_none()
}

fn check_streak(world: &mut World, a: Entity, d: Entity) -> bool {
    let streak = streak(world, a);
    let combos: [(&str, fn(&mut World, Entity, Entity) -> bool); 4] = [
        (WRIST_WRENCH, wrist_wrench),
        (LAUNCH_KICK, launch_kick),
        (DROP_KICK, drop_kick),
        (KNEE_STOMACH, knee_stomach),
    ];
    for (combo, finisher) in combos {
        if streak.contains(combo) {
            reset_streak(world, a);
            return finisher(world, a, d);
        }
    }
    false
}

/// Gnashing Teeth.
fn wrist_wrench(world: &mut World, a: Entity, d: Entity) -> bool {
    let Some(arm) = super::active_arm(world, d) else {
        return false;
    };
    let (an, dn) = (name(world, a), name(world, d));
    let part = arm.ru().1;
    message(
        world,
        a,
        d,
        NEAR,
        (
            Span::Danger,
            format!("{} яростно выкручивает {part} {dn}!", capitalize(&an)),
        ),
        format!("{} яростно выкручивает вам {part}!", capitalize(&an)),
        Some((
            Span::Danger,
            format!("Вы яростно выкручиваете {part} {dn}!"),
        )),
    );
    play(world, d, PUNCH, 25.0);
    hurt(world, d, Some(arm), DamageType::Brute, 20.0, Some(30.0));
    drop_held(world, d);
    true
}

/// Crashing Wave Kick: seven tiles back.
fn launch_kick(world: &mut World, a: Entity, d: Entity) -> bool {
    let (an, dn) = (name(world, a), name(world, d));
    message(
        world,
        a,
        d,
        COMBAT,
        (
            Span::Warning,
            format!(
                "{} пинает {dn} прямо в грудь, отправляя его в полёт!",
                capitalize(&an)
            ),
        ),
        format!("{} запустил вас в полёт ударом в грудь!", capitalize(&an)),
        None,
    );
    play(world, a, KICK_SOUND, 50.0);
    let dir = world
        .get::<ss13_shared::Facing>(a)
        .map(|f| f.0)
        .or_else(|| dir_to(world, a, d));
    if let Some(dir) = dir
        && a != d
    {
        fling(world, d, dir, 7, 4.0);
    }
    hurt(
        world,
        d,
        Some(BodyZone::Chest),
        DamageType::Brute,
        15.0,
        None,
    );
    true
}

/// Keelhaul.
fn drop_kick(world: &mut World, a: Entity, d: Entity) -> bool {
    play(world, a, KICK_SOUND, 50.0);
    let (an, dn) = (name(world, a), name(world, d));
    if lying(world, d) {
        drop_held(world, d);
        message(
            world,
            a,
            d,
            COMBAT,
            (
                Span::Warning,
                format!("{} пинает {dn} в голову!", capitalize(&an)),
            ),
            format!("{} пинает вас по голове!", capitalize(&an)),
            None,
        );
    } else {
        incap(world, d, Incap::Knockdown, 4.0);
        message(
            world,
            a,
            d,
            COMBAT,
            (
                Span::Warning,
                format!(
                    "{} пинает {dn} в голову, швыряя его лицом в пол!",
                    capitalize(&an)
                ),
            ),
            format!(
                "{} бьёт вас по голове, швыряя вас лицом в пол!",
                capitalize(&an)
            ),
            None,
        );
    }
    hurt(world, d, None, DamageType::Stamina, 40.0, None);
    crate::status_effects::adjust_timed(
        world,
        d,
        crate::status_effects::Timed::Dizziness,
        10.0,
        Some(10.0),
    );
    // `adjust_temp_blindness_up_to(2 SECONDS, 10 SECONDS)`.
    let blind = world
        .get::<crate::vision::Vision>(d)
        .map_or(0.0, |v| v.temp_blind);
    if blind < 10.0 {
        crate::vision::adjust_temp_blindness(world, d, 2.0_f32.min(10.0 - blind));
    }
    true
}

/// Kraken Wrack.
fn knee_stomach(world: &mut World, a: Entity, d: Entity) -> bool {
    play(world, a, KICK_SOUND, 50.0);
    let (an, dn) = (name(world, a), name(world, d));
    message(
        world,
        a,
        d,
        COMBAT,
        (
            Span::Warning,
            format!("{} яростно бьёт коленом {dn}!", capitalize(&an)),
        ),
        format!("{} бьёт вас коленом!", capitalize(&an)),
        None,
    );
    hurt(world, d, None, DamageType::Stamina, 20.0, None);
    incap(world, d, Incap::Paralyze, 1.0);
    crate::status_effects::adjust_timed(
        world,
        d,
        crate::status_effects::Timed::Silence,
        5.0,
        Some(5.0),
    );
    if let Some(mut breath) = world.get_mut::<crate::breath::Respiration>(d)
        && breath.losebreath <= 10.0
    {
        breath.losebreath = (breath.losebreath + 5.0).clamp(0.0, 10.0);
    }
    true
}

pub(super) fn grab(world: &mut World, a: Entity, d: Entity) -> Outcome {
    if !can_deflect(world, a) {
        return Outcome::Invalid;
    }
    let text = format!("захват {}", name(world, a));
    if check_block(world, d, Some(a), AttackKind::Unarmed, 0.0, &text) {
        return Outcome::Fail;
    }
    add_to_streak(world, a, "G", d);
    if check_streak(world, a, d) {
        return Outcome::Success;
    }
    play(world, d, PUNCH, 25.0);
    if !unconscious(world, d) && stamina_loss(world, d) >= 80.0 {
        let (an, dn) = (name(world, a), name(world, d));
        message(
            world,
            a,
            d,
            NEAR,
            (
                Span::Danger,
                format!(
                    "{} аккуратно бьёт по нерву на шее {dn}, вырубая его!",
                    capitalize(&an)
                ),
            ),
            format!(
                "{} бьёт в какое-то место на вашей шее, выводя вас из сознания!",
                capitalize(&an)
            ),
            None,
        );
        knock_out(world, d, 10.0);
    }
    hurt(world, d, None, DamageType::Stamina, 20.0, None);
    Outcome::Invalid
}

pub(super) fn harm(world: &mut World, a: Entity, d: Entity) -> Outcome {
    let text = name(world, a);
    if check_block(world, d, Some(a), AttackKind::Unarmed, 10.0, &text) {
        return Outcome::Fail;
    }
    add_to_streak(world, a, "H", d);
    Outcome::from_bool(check_streak(world, a, d))
}

pub(super) fn disarm(world: &mut World, a: Entity, d: Entity) -> Outcome {
    if !can_deflect(world, a) {
        return Outcome::Invalid;
    }
    let text = name(world, a);
    if check_block(world, d, Some(a), AttackKind::Unarmed, 0.0, &text) {
        return Outcome::Fail;
    }
    add_to_streak(world, a, "D", d);
    if check_streak(world, a, d) {
        return Outcome::Success;
    }
    play(world, d, PUNCH, 25.0);
    hurt(world, d, None, DamageType::Stamina, 20.0, None);
    Outcome::Invalid
}

/// `carp_style_check`: how "carp" the mob is. Martial or carp clothing on the head, chest and
/// feet add 20 each; the melee armor of the head and chest, held items bigger than small (not
/// exempt) and lying down take away.
pub(super) fn style_points(world: &World, mob: Entity) -> f32 {
    let Some(inventory) = world.get::<Inventory>(mob) else {
        return 0.0;
    };
    let mut points = 0.0;
    let mut malus = 0.0;
    let worn: Vec<(Entity, &DmClothing)> = inventory
        .equipped()
        .filter_map(|i| Some((i, world.get::<DmClothing>(i)?)))
        .collect();
    for zone in [BodyZone::Head, BodyZone::Chest] {
        if !world
            .get::<crate::health::Health>(mob)
            .is_some_and(|h| h.has_part(zone))
        {
            continue;
        }
        let bit = crate::chem::body_part_bit(zone);
        points += 20.0
            * worn
                .iter()
                .filter(|(_, c)| {
                    c.body_parts_covered & bit != 0 && c.clothing_flags & CARP_STYLE_FACTOR != 0
                })
                .count() as f32;
        malus += super::melee_armor(world, mob, zone, 0.0);
    }
    if inventory
        .shoes
        .and_then(|s| world.get::<DmClothing>(s))
        .is_some_and(|c| c.clothing_flags & CARP_STYLE_FACTOR != 0)
    {
        points += 20.0;
    }
    for item in inventory.hands.into_iter().flatten() {
        let Some(path) = world.get::<DmItem>(item).map(|i| i.path.as_str()) else {
            continue;
        };
        if EXEMPT.iter().any(|root| is_under(path, root)) {
            continue;
        }
        let w_class = world
            .get::<DmWeight>(item)
            .copied()
            .unwrap_or_default()
            .w_class;
        if w_class <= WEIGHT_CLASS_SMALL {
            continue;
        }
        malus += f32::from(w_class) * 10.0;
    }
    if lying(world, mob) {
        points -= 30.0;
    }
    points - malus
}

/// `check_dodge`: in combat mode, a melee or unarmed attack is dodged with the style points
/// (at most 75%).
pub(super) fn dodge(world: &mut World, me: Entity, kind: AttackKind, attack_text: &str) -> bool {
    if !can_deflect(world, me) || kind == AttackKind::Projectile {
        return false;
    }
    let avoidance = style_points(world, me).round().clamp(0.0, 75.0);
    if !chance(world, avoidance) {
        return false;
    }
    let mn = name(world, me);
    message(
        world,
        me,
        me,
        NEAR,
        (
            Span::Danger,
            format!(
                "{} чисто уворачивается от {attack_text} с невероятной скоростью!",
                capitalize(&mn)
            ),
        ),
        format!("Вы уворачиваетесь от {attack_text}"),
        None,
    );
    play(world, me, "sound/items/weapons/punchmiss.ogg", 25.0);
    true
}

/// `hit_by_projectile`: in combat mode, 100% plus the style points (a true-strike .38 75%
/// less, never a harpoon) to send the projectile off a random way as the carp's own.
pub(super) fn deflect(
    world: &mut World,
    me: Entity,
    entity: Entity,
    p: &mut crate::projectile::Projectile,
) -> bool {
    if is_under(&p.data.path, "/obj/projectile/bullet/harpoon") || !can_deflect(world, me) {
        return false;
    }
    let mut adjust = 0.0;
    if is_under(&p.data.path, "/obj/projectile/bullet/c38/match/true") {
        adjust -= 75.0;
    }
    let avoidance = (100.0 + style_points(world, me) + adjust).clamp(0.0, 100.0);
    if !chance(world, avoidance) {
        return false;
    }
    let (mn, pn) = (name(world, me), name(world, entity));
    message(
        world,
        me,
        me,
        NEAR,
        (
            Span::Danger,
            format!(
                "{} без каких-либо усилий отбивает {pn}! Он может отражать пули голыми руками!",
                capitalize(&mn)
            ),
        ),
        format!("Вы отражаете {pn}!"),
        None,
    );
    play(world, me, "bullet_miss", 75.0);
    p.firer = me;
    p.original = None;
    p.angle = super::roll(world, 0, 359) as f32;
    let (sin, cos) = p.angle.to_radians().sin_cos();
    p.dx = sin;
    p.dy = cos;
    true
}
