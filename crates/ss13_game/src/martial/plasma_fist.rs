//! Plasma Fist (tg `code/datums/martial/plasma_fist.dm`), the wizard's frayed scroll: the
//! tornado sweep (repulse around the user), the throwback (the target flies away), the plasma
//! fist (gibs the target; a kill with a client powers up the apotheosis) and the apotheosis (the
//! plasma fist on yourself: a plasmaman body, everything dropped, the soul knocked out and an
//! explosion of `power, 2×power, 4×power`; the body dies 6 s later). The novice scroll's style
//! has no apotheosis.
// shortcut: no dance rotation, plasma soul beam or purple flash; the user's bomb immunity is the
// explosion's protected epicenter (items dropped on the tile survive too). Add with animations
// and `TRAIT_BOMBIMMUNE`.

use bevy::prelude::*;
use ss13_shared::chat::Span;

use super::{
    AttackKind, NEAR, Outcome, add_to_streak, check_block, message, name, play, reset_streak,
    streak, tell_mob, with_art,
};
use crate::spells::Invocation;

const TORNADO: &str = "HHD";
const THROWBACK: &str = "DHD";
const PLASMA: &str = "HDDDH";

/// `plasma_increment`, `plasma_cap`.
const PLASMA_INCREMENT: u32 = 1;
const PLASMA_CAP: u32 = 12;
const PUNCH: &str = "sound/items/weapons/punch1.ogg";

pub(super) const HELP: &[&str] = &[
    "Вы сжимаете кулаки, и вас озаряет вспышка знаний...",
    "Торнадо: Удар Удар Толчок. Отбрасывает противника и всех вокруг.",
    "Отброс: Толчок Удар Толчок. Отшвыривает противника.",
    "Плазменный кулак: Удар Толчок Толчок Толчок Удар. Мгновенно разрывает противника. Каждое убийство им усиливает взрыв Апофеоза.",
    "Апофеоз: Плазменный кулак на себе. Вы уходите в славном взрыве.",
];

pub(super) const HELP_NOVICE: &[&str] = &[
    "Вы сжимаете кулаки, и вас озаряет вспышка знаний...",
    "Торнадо: Удар Удар Толчок. Отбрасывает противника и всех вокруг.",
    "Отброс: Толчок Удар Толчок. Отшвыривает противника.",
    "Плазменный кулак: Удар Толчок Толчок Толчок Удар. Мгновенно разрывает противника.",
];

/// The frayed scroll's `remarks`.
pub(super) const REMARKS: &[&str] = &[
    "Равновесие...",
    "Сила...",
    "Контроль...",
    "Мастерство...",
    "Бдительность...",
    "Умение...",
];

/// The frayed scroll's `greet`.
pub(super) const GREET: &str = "Вы изучили древнее боевое искусство Плазменного кулака. Ваши комбо крайне сложно провести, но среди них есть одни из самых смертоносных приёмов, включая плазменный кулак, от которого противник буквально взрывается.";

/// The body of an apotheosis, dying at this time (`Apotheosis_end`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Apotheosis(pub f64);

/// The finisher in the streak (`findtext`, tg order).
pub fn finisher(streak: &str) -> Option<&'static str> {
    [TORNADO, THROWBACK, PLASMA]
        .into_iter()
        .find(|c| streak.contains(c))
}

fn shout(world: &mut World, mob: Entity, text: &str) {
    crate::spells::say_invocation(world, mob, Invocation::Shout, text);
}

fn check_streak(world: &mut World, a: Entity, d: Entity, nobomb: bool) -> bool {
    match finisher(&streak(world, a)) {
        Some(TORNADO | THROWBACK) if a == d => false,
        Some(TORNADO) => {
            reset_streak(world, a);
            tornado(world, a)
        }
        Some(THROWBACK) => {
            reset_streak(world, a);
            throwback(world, a, d)
        }
        Some(_) => {
            reset_streak(world, a);
            if a == d && !nobomb {
                apotheosis(world, a)
            } else {
                plasma(world, a, d, nobomb)
            }
        }
        None => false,
    }
}

pub(super) fn harm(world: &mut World, a: Entity, d: Entity, nobomb: bool) -> Outcome {
    let text = name(world, a);
    if check_block(world, d, Some(a), AttackKind::Unarmed, 10.0, &text) {
        return Outcome::Fail;
    }
    add_to_streak(world, a, "H", d);
    Outcome::from_bool(check_streak(world, a, d, nobomb))
}

pub(super) fn disarm(world: &mut World, a: Entity, d: Entity, nobomb: bool) -> Outcome {
    let text = name(world, a);
    if check_block(world, d, Some(a), AttackKind::Unarmed, 0.0, &text) {
        return Outcome::Fail;
    }
    add_to_streak(world, a, "D", d);
    if check_streak(world, a, d, nobomb) {
        return Outcome::Success;
    }
    if a == d {
        tell_mob(world, a, Span::Notice, "Вы добавили толчок в комбо.");
        return Outcome::Fail;
    }
    Outcome::Invalid
}

pub(super) fn grab(world: &mut World, a: Entity, d: Entity, nobomb: bool) -> Outcome {
    let text = format!("захват {}", name(world, a));
    if check_block(world, d, Some(a), AttackKind::Unarmed, 0.0, &text) {
        return Outcome::Fail;
    }
    add_to_streak(world, a, "G", d);
    Outcome::from_bool(check_streak(world, a, d, nobomb))
}

/// `Tornado`: the scroll's repulse (radius 7) cast from the user.
fn tornado(world: &mut World, a: Entity) -> bool {
    shout(world, a, "TORNADO SWEEP!");
    play(world, a, PUNCH, 15.0);
    if let Some(&pos) = world.get::<ss13_shared::TilePos>(a) {
        crate::spells::effects::repulse(world, a, pos, 7);
    }
    true
}

fn throwback(world: &mut World, a: Entity, d: Entity) -> bool {
    let (an, dn) = (name(world, a), name(world, d));
    message(
        world,
        a,
        d,
        NEAR,
        (Span::Danger, format!("{an} бьёт {dn} плазменным ударом!")),
        format!("{an} бьёт вас плазменным ударом!"),
        Some((Span::Danger, format!("Вы бьёте {dn} плазменным ударом!"))),
    );
    play(world, d, PUNCH, 50.0);
    if let Some(dir) = super::dir_to(world, a, d) {
        super::fling(world, d, dir, 200, 4.0);
    }
    shout(world, a, "HYAH!");
    true
}

fn plasma(world: &mut World, a: Entity, d: Entity, nobomb: bool) -> bool {
    let has_client = world.get::<crate::server::PlayerMob>(d).is_some();
    play(world, d, PUNCH, 50.0);
    shout(world, a, "PLASMA FIST!");
    let (an, dn) = (name(world, a), name(world, d));
    message(
        world,
        a,
        d,
        NEAR,
        (
            Span::Danger,
            format!("{an} бьёт {dn} ТЕХНИКОЙ ПЛАЗМЕННОГО КУЛАКА!"),
        ),
        format!("{an} внезапно бьёт вас ТЕХНИКОЙ ПЛАЗМЕННОГО КУЛАКА!"),
        Some((
            Span::Danger,
            format!("Вы бьёте {dn} ТЕХНИКОЙ ПЛАЗМЕННОГО КУЛАКА!"),
        )),
    );
    info!("{a} gibbed {d} with the plasma fist");
    crate::health::gib(world, d);
    if nobomb {
        return true;
    }
    let power = with_art(world, a, |art| art.plasma_power).unwrap_or(1);
    if !has_client {
        tell_mob(
            world,
            a,
            Span::Warning,
            "Забрать эту плазменную энергию для Апофеоза было бы бесчестьем для клана!",
        );
    } else if power >= PLASMA_CAP {
        tell_mob(
            world,
            a,
            Span::Warning,
            "Вы больше не можете усилить свой Апофеоз!",
        );
    } else {
        let power = power + PLASMA_INCREMENT;
        with_art(world, a, |art| art.plasma_power = power);
        tell_mob(
            world,
            a,
            Span::Notice,
            format!("Сила растёт! Ваш Апофеоз теперь на уровне силы {power}!"),
        );
    }
    true
}

fn apotheosis(world: &mut World, user: Entity) -> bool {
    shout(world, user, "АПОФЕОЗ!!");
    let Some(&pos) = world.get::<ss13_shared::TilePos>(user) else {
        return true;
    };
    if world.get::<crate::inventory::Inventory>(user).is_some() {
        crate::species::set_species(world, user, crate::species::SpeciesId::Plasmaman);
        // `unequip_everything`.
        for slot in crate::inventory::ALL_SLOTS {
            if let Some(item) = world
                .get::<crate::inventory::Inventory>(user)
                .and_then(|i| i.slot(slot))
            {
                crate::inventory::unequip(world, user, pos, slot);
                crate::inventory::put_down(world, item, pos);
            }
        }
    }
    let power = with_art(world, user, |art| art.plasma_power).unwrap_or(1);
    warn!("{user} triggered the final plasma explosion of power {power}");
    tell_mob(
        world,
        user,
        Span::BoldWarning,
        "Взрыв выбивает душу из вашего тела!",
    );
    if let Some(client) = world
        .get::<crate::server::PlayerMob>(user)
        .map(|p| p.client)
    {
        crate::ghost::ghostize_client(world, client);
    }
    let brute = super::roll(world, 50, 70) as f32;
    super::hurt(
        world,
        user,
        None,
        crate::health::DamageType::Brute,
        brute,
        None,
    );
    let now = world.resource::<Time>().elapsed_secs_f64();
    world.entity_mut(user).insert(Apotheosis(now + 6.0));
    play(world, user, PUNCH, 50.0);
    let p = power as i32;
    let mut ex = crate::explosions::Explosion::new(p, p * 2, p * 4);
    ex.ignorecap = true;
    ex.protect_epicenter = true;
    ex.cause = "plasma fist".to_owned();
    crate::explosions::explosion(world, pos, ex);
    with_art(world, user, |art| art.plasma_power = 1);
    true
}

/// `Apotheosis_end`: the body dies when its time comes.
// Cost: per tick, the bodies of an apotheosis (almost always none).
pub(super) fn end_apotheosis(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &Apotheosis)>();
    let due: Vec<Entity> = q
        .iter(world)
        .filter(|(_, a)| a.0 <= now)
        .map(|(e, _)| e)
        .collect();
    for mob in due {
        world.entity_mut(mob).remove::<Apotheosis>();
        let (Some(mut health), Some(mut stat)) = (
            world.get::<crate::health::Health>(mob).cloned(),
            world.get::<crate::health::Stat>(mob).copied(),
        ) else {
            continue;
        };
        if stat == crate::health::Stat::Dead {
            continue;
        }
        let amount = health.health() - crate::health::HEALTH_THRESHOLD_DEAD;
        crate::health::apply_damage(
            &mut health,
            &mut stat,
            crate::health::DamageType::Oxy,
            amount,
        );
        stat = crate::health::Stat::Dead;
        world.entity_mut(mob).insert((health, stat));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finishers_are_found_anywhere_in_tg_order() {
        assert_eq!(finisher("HHD"), Some(TORNADO));
        assert_eq!(finisher("DHHD"), Some(TORNADO), "before the throwback");
        assert_eq!(finisher("GDHDG"), Some(THROWBACK));
        assert_eq!(finisher("HDDDH"), Some(PLASMA));
        assert_eq!(finisher("HDDH"), None);
    }
}
