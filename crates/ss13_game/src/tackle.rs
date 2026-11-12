//! Tackling (server): tg `/datum/component/tackler` (`code/datums/components/tackle.dm`) of the
//! tackle gloves (`code/modules/clothing/gloves/tacklers.dm`: gripper, dolphin, combat, rocket,
//! off-brand and football gloves).
//!
//! In throw mode, a right click with an empty active hand while wearing tackle gloves leaps at
//! the clicked tile (`checkTackle`): not pulling, buckled, incapacitated, lying, staggered or
//! with blocked hands, and not before the last leap's knockdown has passed. The leap costs
//! stamina and knocks the tackler down for the gloves' `base_knockdown`; it flies up to the
//! gloves' `range` tiles (at least `min_distance`). Hitting a carbon rolls the tackle
//! (`rollTackle`: −3..3, the defender's and attacker's modifiers, the gloves' `skill_mod`) into
//! a bad, neutral or good outcome (stamina damage, knockdowns, staggering, a passive or
//! aggressive grab); a block of the defender forces the neutral one. Hitting a window or another
//! dense obstacle splats the tackler (`splat`): the faster the gloves, the worse.
// shortcut: the leap is instant (steps this tick, like `crate::martial::fling`); the roll reads
// arms, clumsiness, health, nakedness and `TRAIT_NOGUNS` only (no drunkenness, leg wounds,
// height, wings, tails, spine implants, sanity or riot armor); no table messes, vendor tilting
// or window shards; the splat's paralysis and skull-crack traumas are the mild concussion and
// the paraplegic paralysis only. Add with the throw engine for mobs and the missing traits.

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_shared::chat::Span;
use ss13_shared::protocol::InventorySlot;
use ss13_shared::{Dir, TilePos};

use crate::health::{DamageType, Health};
use crate::inventory::Inventory;
use crate::martial::{COMBAT, hurt, incap, message, name, play};
use crate::status_effects::Incap;
use crate::zone::BodyZone;

/// One tackle glove type's `tackler` arguments.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gloves {
    pub root: &'static str,
    pub stamina_cost: f32,
    /// `base_knockdown`, seconds.
    pub knockdown: f32,
    pub range: u32,
    pub speed: f32,
    pub skill_mod: i32,
    pub min_distance: u32,
}

const fn gloves(
    root: &'static str,
    stamina_cost: f32,
    knockdown: f32,
    range: u32,
    speed: f32,
    skill_mod: i32,
    min_distance: u32,
) -> Gloves {
    Gloves {
        root,
        stamina_cost,
        knockdown,
        range,
        speed,
        skill_mod,
        min_distance,
    }
}

/// Most specific first.
pub static GLOVES: [Gloves; 5] = [
    gloves(
        "/obj/item/clothing/gloves/tackler/dolphin",
        15.0,
        0.5,
        5,
        2.0,
        -2,
        2,
    ),
    gloves(
        "/obj/item/clothing/gloves/tackler/combat",
        30.0,
        1.25,
        5,
        1.0,
        2,
        0,
    ),
    gloves(
        "/obj/item/clothing/gloves/tackler/rocket",
        50.0,
        2.0,
        10,
        6.0,
        7,
        7,
    ),
    gloves(
        "/obj/item/clothing/gloves/tackler/offbrand",
        30.0,
        1.75,
        4,
        1.0,
        -1,
        2,
    ),
    gloves("/obj/item/clothing/gloves/tackler", 25.0, 1.0, 4, 1.0, 1, 0),
];

/// The tackle gloves `mob` wears in its gloves slot.
pub fn worn_gloves(world: &World, mob: Entity) -> Option<&'static Gloves> {
    let gloves = world.get::<Inventory>(mob)?.slot(InventorySlot::Gloves)?;
    let path = world.get::<DmItem>(gloves)?.path.as_str();
    GLOVES.iter().find(|g| is_under(path, g.root))
}

/// tg `tackling`: when the tackler may leap again (`resetTackle` after `base_knockdown`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct TackleReady(pub f64);

/// `checkTackle` for a throw-mode click: true when it was a tackle (the click ends here).
pub fn try_tackle(
    world: &mut World,
    client: Entity,
    mob: Entity,
    target: Entity,
    target_pos: TilePos,
    right: bool,
) -> bool {
    if !right {
        return false;
    }
    let Some(gloves) = worn_gloves(world, mob).copied() else {
        return false;
    };
    let tell = |world: &mut World, text: &str| {
        crate::tools::tell(world, client, Span::Warning, text);
    };
    let held = world.get::<Inventory>(mob).and_then(Inventory::active_item);
    let state = crate::status_effects::incapacitation(world, mob);
    if held.is_some()
        || world.get::<crate::pull::Pulling>(mob).is_some()
        || world.get::<crate::medical::surgery::Buckled>(mob).is_some()
        || state.incapacitated
    {
        return false;
    }
    if state.hands_blocked {
        tell(world, "Для захвата нужны свободные руки!");
        return true;
    }
    if crate::martial::lying(world, mob) {
        tell(world, "Для захвата нужно стоять!");
        return true;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    if world.get::<TackleReady>(mob).is_some_and(|r| now < r.0) {
        tell(world, "Вы не готовы к захвату!");
        return true;
    }
    if crate::martial::effects(world, mob).staggered > 0.0 {
        tell(world, "Вы слишком потеряли равновесие для захвата!");
        return true;
    }
    let Some(&from) = world.get::<TilePos>(mob) else {
        return true;
    };
    world.entity_mut(mob).insert(crate::throw::ThrowMode(false));
    if let Some(dir) = crate::pull::dir_between(from, target_pos) {
        world.entity_mut(mob).insert(ss13_shared::Facing(dir));
    }
    play(world, mob, "sound/items/weapons/thudswoosh.ogg", 40.0);
    let (mn, tn) = (name(world, mob), name(world, target));
    message(
        world,
        mob,
        mob,
        COMBAT,
        (Span::Warning, format!("{mn} прыгает на {tn}!")),
        format!("Вы прыгаете на {tn}!"),
        None,
    );
    // `min_distance`: a closer target is a point that far along the same line.
    let dest = extend(from, target_pos, gloves.min_distance);
    world
        .entity_mut(mob)
        .insert(TackleReady(now + f64::from(gloves.knockdown)));
    crate::status_effects::adjust_stamina(world, mob, gloves.stamina_cost);
    let hit = leap(world, mob, dest, gloves.range);
    // `Knockdown(base_knockdown, ignore_canstun = TRUE)` of the leap; an outcome may set it.
    incap(world, mob, Incap::Knockdown, gloves.knockdown);
    if let Some(hit) = hit {
        if world.get::<Inventory>(hit).is_some() && world.get::<Health>(hit).is_some() {
            sack(world, mob, hit, &gloves);
        } else if world.get::<Health>(hit).is_none() {
            splat(world, mob, hit, &gloves);
        }
    }
    true
}

/// The tile `min` tiles from `from` toward `to` when `to` is closer than that.
pub fn extend(from: TilePos, to: TilePos, min: u32) -> TilePos {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let dist = dx.abs().max(dy.abs());
    if dist == 0 || dist >= min as i32 {
        return to;
    }
    let k = min as f32 / dist as f32;
    TilePos::new(
        from.x + (dx as f32 * k).round() as i32,
        from.y + (dy as f32 * k).round() as i32,
        from.z,
    )
}

/// The next cardinal step from `at` toward `dest` (the longer axis first).
pub fn step_dir(at: TilePos, dest: TilePos) -> Option<Dir> {
    let (dx, dy) = (dest.x - at.x, dest.y - at.y);
    if dx == 0 && dy == 0 {
        return None;
    }
    Some(if dx.abs() >= dy.abs() {
        if dx > 0 { Dir::East } else { Dir::West }
    } else if dy > 0 {
        Dir::North
    } else {
        Dir::South
    })
}

/// The flight: steps toward `dest` up to `range`; what stopped it, if anything.
fn leap(world: &mut World, mob: Entity, dest: TilePos, range: u32) -> Option<Entity> {
    for _ in 0..range {
        let at = *world.get::<TilePos>(mob)?;
        let dir = step_dir(at, dest)?;
        if let Err(blockers) = crate::pull::step_once(world, mob, dir, None) {
            // A mob first, then whatever else is dense.
            return blockers
                .iter()
                .copied()
                .find(|&b| world.get::<Health>(b).is_some())
                .or_else(|| blockers.first().copied());
        }
    }
    None
}

/// `rollTackle`.
fn roll(world: &mut World, sacker: Entity, target: Entity, skill_mod: i32) -> i32 {
    let mut defense = 0;
    defense += if crate::martial::active_arm(world, target).is_some() {
        1
    } else {
        -2
    };
    if crate::genetics::is_clumsy(world, target) {
        defense -= 2;
    }
    if world
        .get::<Health>(target)
        .is_some_and(|h| h.health() < 50.0)
    {
        defense -= 1;
    }
    if world.get::<Inventory>(target).is_some_and(|i| {
        i.slot(InventorySlot::Suit).is_none() && i.slot(InventorySlot::Uniform).is_none()
    }) {
        defense += 2;
    }
    let mut attack = if crate::martial::active_arm(world, sacker).is_some() {
        1
    } else {
        -4
    };
    if crate::genetics::is_clumsy(world, sacker) {
        attack -= 2;
    }
    if crate::martial::no_guns(world, sacker) {
        attack += 2;
    }
    crate::martial::roll(world, 0, 6) as i32 - 3 - defense + attack + skill_mod
}

/// `STAGGERED_SLOWDOWN_LENGTH`.
const STAGGER: f32 = crate::security::STAGGERED_SLOWDOWN_LENGTH;

fn stagger(world: &mut World, mob: Entity, secs: f32) {
    let dead = crate::martial::stat(world, mob) == crate::health::Stat::Dead;
    if let Some(mut e) = crate::status_effects::effects_mut(world, mob) {
        e.adjust_staggered_up_to(secs, 10.0, dead);
    }
}

/// `SetKnockdown(0)` and `get_up`.
fn get_up(world: &mut World, mob: Entity) {
    if let Some(mut e) = crate::status_effects::effects_mut(world, mob) {
        e.knockdown = 0.0;
    }
}

fn grab(world: &mut World, a: Entity, d: Entity, state: crate::pull::GrabState) {
    use crate::pull::{GrabState, PullSlowdown, Pulling};
    if crate::martial::stat(world, a) != crate::health::Stat::Conscious {
        return;
    }
    world
        .entity_mut(a)
        .insert((Pulling(d), state, PullSlowdown(0.0)));
    if state != GrabState::Passive {
        world.entity_mut(d).remove::<Pulling>();
    }
}

fn move_onto(world: &mut World, mob: Entity, target: Entity) {
    if let Some(&at) = world.get::<TilePos>(target) {
        world.entity_mut(mob).insert(at);
    }
}

fn say(world: &mut World, a: Entity, d: Entity, others: String, own: String, victim: String) {
    message(
        world,
        a,
        d,
        COMBAT,
        (Span::Warning, others),
        victim,
        Some((Span::BoldWarning, own)),
    );
}

/// `sack`: the tackle lands on a carbon.
fn sack(world: &mut World, user: Entity, target: Entity, gloves: &Gloves) {
    let (un, tn) = (name(world, user), name(world, target));
    if crate::martial::check_block(
        world,
        target,
        Some(user),
        crate::martial::AttackKind::Unarmed,
        0.0,
        &un,
    ) {
        say(
            world,
            user,
            target,
            format!("{tn} блокирует захват {un}, смягчая эффект!"),
            format!("{tn} блокирует ваш захват, смягчая эффект!"),
            format!("{tn} блокирует попытку захвата {un}, смягчая эффект!"),
        );
        neutral(world, user, target);
        return;
    }
    let r = roll(world, user, target, gloves.skill_mod);
    match r {
        ..=-1 => negative(world, user, target, r),
        0 => neutral(world, user, target),
        _ => positive(world, user, target, r),
    }
}

/// The target's chest melee armor as a fraction left (`run_armor_check`); 0.9 for non-humans.
fn armor_left(world: &World, target: Entity) -> f32 {
    if world.get::<Inventory>(target).is_none() {
        return 0.9;
    }
    (100.0 - crate::martial::melee_armor(world, target, BodyZone::Chest, 0.0)) / 100.0
}

fn positive(world: &mut World, user: Entity, target: Entity, r: i32) {
    let outcome = (r * 10) as f32 * armor_left(world, target);
    let (un, tn) = (name(world, user), name(world, target));
    if outcome <= 0.0 {
        neutral(world, user, target);
    } else if outcome <= 20.0 {
        say(
            world,
            user,
            target,
            format!("{un} проводит крепкий захват {tn}, и оба тяжело падают!"),
            format!("Вы проводите крепкий захват {tn}, и вы оба тяжело падаете!"),
            format!("{un} проводит по вам крепкий захват, и вы оба тяжело падаете!"),
        );
        hurt(world, target, None, DamageType::Stamina, 30.0, None);
        incap(world, target, Incap::Paralyze, 0.5);
        incap(world, user, Incap::Knockdown, 1.0);
        incap(world, target, Incap::Knockdown, 2.0);
        stagger(world, target, STAGGER * 2.0);
    } else if outcome < 50.0 {
        say(
            world,
            user,
            target,
            format!("{un} мастерски сбивает {tn} с ног и приземляется на ноги, удерживая захват!"),
            format!("Вы мастерски сбиваете {tn} с ног и приземляетесь на ноги, удерживая захват!"),
            format!("{un} мастерски сбивает вас с ног и удерживает захват!"),
        );
        get_up(world, user);
        move_onto(world, user, target);
        hurt(world, target, None, DamageType::Stamina, 40.0, None);
        incap(world, target, Incap::Paralyze, 0.5);
        incap(world, target, Incap::Knockdown, 3.0);
        stagger(world, target, STAGGER * 2.0);
        grab(world, user, target, crate::pull::GrabState::Passive);
    } else if crate::status_effects::incapacitation(world, user).incapacitated {
        say(
            world,
            user,
            target,
            format!("{un} безрассудно врезается в {tn}, оглушая обоих!"),
            format!("Вы безрассудно врезаетесь в {tn}, оглушая вас обоих!"),
            format!("{un} безрассудно врезается в вас, оглушая вас обоих!"),
        );
        move_onto(world, user, target);
        hurt(world, target, None, DamageType::Stamina, 100.0, None);
        incap(world, target, Incap::Paralyze, 1.0);
        incap(world, target, Incap::Knockdown, 5.0);
        stagger(world, target, STAGGER * 3.0);
    } else {
        say(
            world,
            user,
            target,
            format!("{un} чудовищно сбивает {tn}, оглушая и жёстко прижимая к полу!"),
            format!("Вы чудовищно сбиваете {tn}, оглушая и жёстко прижимая к полу!"),
            format!("{un} чудовищно сбивает вас, оглушая и жёстко прижимая к полу!"),
        );
        get_up(world, user);
        move_onto(world, user, target);
        hurt(world, target, None, DamageType::Stamina, 60.0, None);
        incap(world, target, Incap::Paralyze, 0.5);
        incap(world, target, Incap::Knockdown, 3.0);
        stagger(world, target, STAGGER * 3.0);
        grab(world, user, target, crate::pull::GrabState::Aggressive);
    }
}

fn neutral(world: &mut World, user: Entity, target: Entity) {
    let (un, tn) = (name(world, user), name(world, target));
    say(
        world,
        user,
        target,
        format!("{un} сбивает {tn}, и оба ненадолго теряют равновесие!"),
        format!("Вы сбиваете {tn}, и оба ненадолго теряете равновесие!"),
        format!("{un} сбивает вас, и вы оба ненадолго теряете равновесие!"),
    );
    get_up(world, user);
    stagger(world, user, STAGGER);
    stagger(world, target, STAGGER * 2.0);
}

fn negative(world: &mut World, user: Entity, target: Entity, r: i32) {
    // tg reads the target's armor here too.
    let potential = ((-r * 10) as f32 * armor_left(world, target)).max(1.0) as u32;
    let actual = crate::martial::roll(world, 1, potential);
    let (un, tn) = (name(world, user), name(world, target));
    if actual <= 20 {
        say(
            world,
            user,
            target,
            format!("{un} слабо сбивает {tn}, ненадолго выводя из равновесия!"),
            format!("Вы слабо сбиваете {tn}, ненадолго выводя из равновесия!"),
            format!("{un} слабо сбивает вас, выводя из равновесия!"),
        );
        incap(world, user, Incap::Knockdown, 1.0);
        stagger(world, user, STAGGER * 2.0);
        hurt(world, target, None, DamageType::Stamina, 20.0, None);
        stagger(world, target, STAGGER * 2.0);
    } else if actual < 50 {
        say(
            world,
            user,
            target,
            format!("{un} неуклюже бросается на {tn} и падает на пол!"),
            format!("Вы неуклюже бросаетесь на {tn} и падаете на пол!"),
            format!("{un} неуклюже бросается на вас и падает на пол!"),
        );
        incap(world, user, Incap::Knockdown, 3.0);
        hurt(world, user, None, DamageType::Stamina, 40.0, None);
        stagger(world, user, STAGGER * 2.0);
    } else {
        say(
            world,
            user,
            target,
            format!("{un} проваливает захват и врезается головой в {tn}!"),
            format!("Вы проваливаете захват и врезаетесь головой в {tn}!"),
            format!("{un} проваливает захват и врезается головой в вас!"),
        );
        incap(world, user, Incap::Paralyze, 3.0);
        hurt(world, user, None, DamageType::Stamina, 80.0, None);
        hurt(
            world,
            user,
            Some(BodyZone::Head),
            DamageType::Brute,
            20.0,
            Some(0.0),
        );
        concussion(world, user);
        stagger(world, user, STAGGER * 3.0);
    }
}

fn concussion(world: &mut World, mob: Entity) {
    if let Some(brain) =
        crate::medical::organs::organ(world, mob, crate::medical::organs::Slot::Brain)
    {
        crate::medical::traumas::gain(
            world,
            brain,
            crate::medical::traumas::Kind::Concussion,
            None,
        );
    }
}

/// The `splat` severity: `rand(danger_zone, 100) + oopsie_mod`.
pub fn oopsie(roll_from_danger: u32, clumsy: bool) -> u32 {
    roll_from_danger + if clumsy { 6 } else { 0 }
}

/// `splat`: the tackler flew into something dense.
fn splat(world: &mut World, user: Entity, hit: Entity, gloves: &Gloves) {
    let (un, hn) = (name(world, user), name(world, hit));
    let tell = |world: &mut World, others: String, own: String| {
        message(world, user, user, COMBAT, (Span::Danger, others), own, None);
    };
    let window = world
        .get::<ss13_mapload::DmObj>(hit)
        .is_some_and(|o| is_under(&o.path, crate::obj_damage::WINDOW));
    if window {
        play(world, user, "sound/effects/glass/Glasshit.ogg", 140.0);
        tell(
            world,
            format!("{un} врезается в {hn}, как жук!"),
            format!("Вы врезаетесь в {hn}, как жук!"),
        );
        incap(world, user, Incap::Paralyze, 1.0);
        incap(world, user, Incap::Knockdown, 3.0);
        let _ = crate::obj_damage::take_damage(world, hit, 30.0 * gloves.speed, DamageType::Brute);
        crate::status_effects::adjust_stamina(world, user, 10.0 * gloves.speed);
        hurt(
            world,
            user,
            None,
            DamageType::Brute,
            5.0 * gloves.speed,
            None,
        );
        return;
    }
    let danger = (((gloves.speed - 1.0) * 13.0) as u32).clamp(1, 100);
    let rolled = crate::martial::roll(world, danger, 100);
    let clumsy = crate::genetics::is_clumsy(world, user);
    match oopsie(rolled, clumsy) {
        99.. => {
            tell(
                world,
                format!("{un} врезается лицом в {hn} под неудачным углом, ломая себе позвоночник!"),
                format!(
                    "Вы врезаетесь лицом в {hn} под неудачным углом и ломаете себе позвоночник!"
                ),
            );
            hurt(
                world,
                user,
                Some(BodyZone::Head),
                DamageType::Brute,
                40.0,
                Some(40.0),
            );
            hurt(world, user, None, DamageType::Stamina, 30.0, None);
            play(world, user, "sound/effects/wounds/crack2.ogg", 70.0);
            if let Some(brain) =
                crate::medical::organs::organ(world, user, crate::medical::organs::Slot::Brain)
            {
                crate::medical::traumas::gain(
                    world,
                    brain,
                    crate::medical::traumas::Kind::Paralysis(
                        crate::medical::traumas::Paralysis::Legs,
                    ),
                    None,
                );
            }
        }
        97..=98 => {
            tell(
                world,
                format!("{un} врезается черепом в {hn} с хрустом!"),
                format!("Вы врезаетесь черепом в {hn}, и череп трескается!"),
            );
            hurt(
                world,
                user,
                Some(BodyZone::Head),
                DamageType::Brute,
                30.0,
                Some(25.0),
            );
            hurt(world, user, None, DamageType::Stamina, 30.0, None);
            concussion(world, user);
            play(world, user, "sound/effects/splat.ogg", 70.0);
        }
        93..=96 => {
            tell(
                world,
                format!("{un} врезается лицом в {hn} и тут же обмякает!"),
                format!("Вы врезаетесь лицом в {hn} и теряете сознание!"),
            );
            hurt(world, user, None, DamageType::Brute, 30.0, None);
            hurt(world, user, None, DamageType::Stamina, 30.0, None);
            crate::martial::knock_out(world, user, 10.0);
            concussion(world, user);
        }
        86..=92 => {
            tell(
                world,
                format!("{un} врезается головой в {hn}, получая серьёзную травму!"),
                format!("Вы врезаетесь головой в {hn}, и мир взрывается вокруг вас!"),
            );
            hurt(world, user, None, DamageType::Brute, 30.0, None);
            hurt(world, user, None, DamageType::Stamina, 30.0, None);
            if let Some(mut e) = crate::status_effects::effects_mut(world, user) {
                e.confusion += 15.0;
            }
            if crate::martial::chance(world, 80.0) {
                concussion(world, user);
            }
            incap(world, user, Incap::Knockdown, 4.0);
        }
        68..=85 => {
            tell(
                world,
                format!("{un} сильно врезается в {hn} и теряет ориентацию!"),
                format!("Вы сильно врезаетесь в {hn} и теряете ориентацию!"),
            );
            hurt(world, user, None, DamageType::Brute, 10.0, None);
            hurt(world, user, None, DamageType::Stamina, 30.0, None);
            if let Some(mut e) = crate::status_effects::effects_mut(world, user) {
                e.confusion += 10.0;
            }
            incap(world, user, Incap::Knockdown, 3.0);
        }
        _ => {
            tell(
                world,
                format!("{un} врезается в {hn}!"),
                format!("Вы врезаетесь в {hn}!"),
            );
            hurt(world, user, None, DamageType::Brute, 10.0, None);
            hurt(world, user, None, DamageType::Stamina, 20.0, None);
            incap(world, user, Incap::Knockdown, 2.0);
        }
    }
    play(world, user, "sound/items/weapons/smash.ogg", 70.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn min_distance_extends_the_leap() {
        let from = TilePos::new(5, 5, 1);
        assert_eq!(
            extend(from, TilePos::new(6, 5, 1), 7),
            TilePos::new(12, 5, 1)
        );
        assert_eq!(
            extend(from, TilePos::new(9, 5, 1), 2),
            TilePos::new(9, 5, 1)
        );
        assert_eq!(step_dir(from, TilePos::new(5, 3, 1)), Some(Dir::South));
        assert_eq!(step_dir(from, from), None);
    }

    #[test]
    fn gloves_follow_tacklers_dm() {
        let rocket = GLOVES
            .iter()
            .find(|g| is_under("/obj/item/clothing/gloves/tackler/rocket", g.root))
            .unwrap();
        assert_eq!(
            (rocket.range, rocket.min_distance, rocket.skill_mod),
            (10, 7, 7)
        );
        let insulated = GLOVES
            .iter()
            .find(|g| is_under("/obj/item/clothing/gloves/tackler/combat/insulated", g.root))
            .unwrap();
        assert_eq!(insulated.skill_mod, 2);
        let football = GLOVES
            .iter()
            .find(|g| is_under("/obj/item/clothing/gloves/tackler/football", g.root))
            .unwrap();
        assert_eq!(football.stamina_cost, 25.0);
    }
}
