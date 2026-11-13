//! Toy mechs and mecha battles (server), after tg `/obj/item/toy/mecha`
//! (`code/game/objects/items/toy_mechs.dm`).
//!
//! - Used in hand a toy mech is played with (a step sound, 1.5 s apart).
//! - One toy mech used on another (in the other hand or within reach) starts a brawl between
//!   them ([`mecha_brawl`]); used on another human (not in combat mode) it offers them a battle
//!   for 6 s, or, if one of the mechs they hold offers one, accepts it.
//! - A brawl ([`Brawl`] on the defending toy) runs a round a second for at most 50 rounds while
//!   the toys stay next to each other and their controllers next to their toys and capable:
//!   rolls of 1-8 for who hits (a 5% crippling extra point), clashes and deflections; a toy at a
//!   third of its health charges its special attack (damage, healing, both, or its own: the
//!   H.O.N.K.'s cooldown, the Death-Ripley's execution, the Reticence's). Then the winner gloats,
//!   wins and losses are counted, health resets and both rest for 30 s.
// shortcut: no spin animations, no `toy_talk` speech through the toy, no telekinetic control,
// no suicide brawl; a brawl's first round comes 1.5 s after the collision and each round is one
// tick of 1 s (tg sleeps 0.5 s before and after the round's checks).
// Cost: one pass over the toy mechs (a handful per round) per tick.

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use crate::combat::CombatRng;
use crate::inventory::Inventory;
use crate::security::name_of;
use crate::sound::{PlaySound, playsound};
use crate::tools::{now, tell_mob};

pub const TOY_MECH: &str = "/obj/item/toy/mecha";
/// `cooldown`, `cooldown_multiplier`.
const COOLDOWN: f64 = 1.5;
const COOLDOWN_MULTIPLIER: f64 = 20.0;
/// `MAX_BATTLE_LENGTH`.
const MAX_BATTLE_LENGTH: u32 = 50;
/// `addtimer(CALLBACK(src, PROC_REF(withdraw_offer), user), 6 SECONDS)`.
const OFFER_SECS: f64 = 6.0;
/// `COMBAT_MESSAGE_RANGE`.
const COMBAT_MESSAGE_RANGE: u32 = 3;
const STEP_SOUND: &str = "sound/vehicles/mecha/mechstep.ogg";
const WIN_LINES: [&str; 6] = [
    "YOU'RE NOTHING BUT SCRAP!",
    "I'LL YIELD TO NONE!",
    "GLORY IS MINE!",
    "AN EASY FIGHT.",
    "YOU SHOULD HAVE NEVER FACED ME.",
    "ROCKED AND SOCKED.",
];

/// `special_attack_type`, with the `SPECIAL_ATTACK_OTHER` moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Special {
    /// +2 damage.
    Damage,
    /// +2 healing.
    Heal,
    /// +1 heal, +1 damage.
    Utility,
    /// The H.O.N.K.: the opponent's special on cooldown, a heal.
    Honk,
    /// The Death-Ripley: kills a weaker opponent, else 1 damage.
    Execute,
    /// The Reticence: a shorter own cooldown, a longer opponent's, 1 damage.
    Reticence,
}

/// One toy mech type: `max_combat_health`, `special_attack_type`, `special_attack_cry`,
/// `quiet`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MechKind {
    pub max_health: i32,
    pub special: Special,
    pub cry: &'static str,
    pub quiet: bool,
}

const fn kind(max_health: i32, special: Special, cry: &'static str) -> MechKind {
    MechKind {
        max_health,
        special,
        cry,
        quiet: false,
    }
}

/// tg's toy mechs by subtype.
const KINDS: [(&str, MechKind); 18] = [
    ("ripley", kind(4, Special::Damage, "CLAMP SMASH")),
    ("ripleymkii", kind(5, Special::Damage, "GIGA DRILL BREAK")),
    ("hauler", kind(3, Special::Utility, "HAUL AWAY")),
    ("clarke", kind(4, Special::Utility, "ROLL OUT")),
    ("odysseus", kind(4, Special::Heal, "MECHA BEAM")),
    ("gygax", kind(5, Special::Utility, "SUPER SERVOS")),
    ("durand", kind(6, Special::Heal, "SHIELD OF PROTECTION")),
    (
        "savannahivanov",
        kind(7, Special::Utility, "SKYFALL!! IVANOV STRIKE"),
    ),
    ("phazon", kind(6, Special::Utility, "NO-CLIP")),
    ("honk", kind(4, Special::Honk, "MEGA HORN")),
    ("darkgygax", kind(6, Special::Utility, "ULTRA SERVOS")),
    ("mauler", kind(7, Special::Damage, "BULLET STORM")),
    ("darkhonk", kind(5, Special::Damage, "BOMBANANA SPREE")),
    ("deathripley", kind(5, Special::Execute, "KILLER CLAMP")),
    (
        "reticence",
        MechKind {
            max_health: 4,
            special: Special::Reticence,
            cry: "*wave",
            quiet: true,
        },
    ),
    ("marauder", kind(7, Special::Damage, "BEAM BLAST")),
    ("seraph", kind(8, Special::Damage, "ROCKET BARRAGE")),
    ("firefighter", kind(5, Special::Heal, "FIRE SHIELD")),
];

impl MechKind {
    pub fn of_path(path: &str) -> Option<Self> {
        if !is_under(path, TOY_MECH) {
            return None;
        }
        let found = KINDS
            .iter()
            .find(|(sub, _)| is_under(path, &format!("{TOY_MECH}/{sub}")));
        // Unknown subtypes keep the base type's zero health and `*flip`.
        Some(found.map_or(kind(0, Special::Damage, "*flip"), |&(_, k)| k))
    }

    /// `special_attack_type_message`.
    fn special_text(self) -> &'static str {
        match self.special {
            Special::Damage => "агрессивный приём, наносящий дополнительный урон.",
            Special::Heal => "защитный приём, дающий дополнительное лечение.",
            Special::Utility => "универсальный приём, лечащий себя и ранящий противника.",
            Special::Honk => {
                "особый приём, который откладывает особую атаку противника и лечит этого меха."
            }
            Special::Execute => {
                "особый приём, мгновенно уничтожающий противника, если у того меньше здоровья."
            }
            Special::Reticence => {
                "особый приём с меньшей перезарядкой, увеличивающий перезарядку противника и \
                 наносящий урон."
            }
        }
    }
}

/// A toy mech's battle state (`/obj/item/toy/mecha` vars).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct ToyMech {
    pub kind: MechKind,
    /// `timer`: when it can play or fight again.
    pub ready_at: f64,
    /// `wants_to_battle` until then, offered by that user.
    pub offer: Option<(f64, Entity)>,
    pub in_combat: bool,
    pub health: i32,
    pub special_charged: bool,
    pub special_cooldown: i32,
    pub wins: u32,
    pub losses: u32,
}

impl ToyMech {
    pub fn new(kind: MechKind) -> Self {
        Self {
            kind,
            ready_at: 0.0,
            offer: None,
            in_combat: false,
            health: kind.max_health,
            special_charged: false,
            special_cooldown: 0,
            wins: 0,
            losses: 0,
        }
    }
}

/// A running `mecha_brawl` on the defending toy.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Brawl {
    pub attacker: Entity,
    pub attacker_controller: Entity,
    /// The defender's controller in a PvP battle.
    pub opponent: Option<Entity>,
    pub length: u32,
    pub next_at: f64,
}

pub struct ToyMechsPlugin;

impl Plugin for ToyMechsPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::AttackSelf, 3337, |w, c| {
            crate::toy_mechs::attack_self(w, c.mob, c.target)
        });
        // A toy mech on the toy mech in the other hand.
        app.add_click_handler(ClickPhase::OtherHand, 427, |w, c| {
            c.held
                .is_some_and(|h| crate::toy_mechs::item_interaction(w, c.mob, h, c.target))
        });
        // A toy mech on a toy mech within reach, or on a human.
        app.add_click_handler(ClickPhase::HeldOnTarget, 3657, |w, c| {
            c.held.is_some_and(|h| {
                crate::toy_mechs::item_interaction(w, c.mob, h, c.target)
                    || crate::toy_mechs::attack(w, c.mob, h, c.target)
            })
        });
        app.add_observer(add_mech).add_systems(Update, tick);
    }
}

fn add_mech(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    if let Some(kind) = items
        .get(add.entity)
        .ok()
        .and_then(|i| MechKind::of_path(&i.path))
    {
        commands
            .entity(add.entity)
            .try_insert_if_new(ToyMech::new(kind));
    }
}

fn pos_of(world: &World, e: Entity) -> Option<TilePos> {
    crate::explosions::turf_of(world, e)
}

/// `in_range(a, b)`: within a tile, on one level.
fn in_range(world: &World, a: Entity, b: Entity) -> bool {
    match (pos_of(world, a), pos_of(world, b)) {
        (Some(a), Some(b)) => a.chebyshev(b).is_some_and(|d| d <= 1),
        _ => false,
    }
}

fn sound(world: &mut World, at: Entity, path: &str, vol: f32) {
    if let Some(pos) = pos_of(world, at) {
        playsound(world, PlaySound::new(pos, path, vol, true));
    }
}

/// `say(text)`, unless `quiet` (`can_speak`).
fn say(world: &mut World, mech: Entity, text: &str) {
    if world.get::<ToyMech>(mech).is_some_and(|m| m.kind.quiet) {
        return;
    }
    crate::atmos_machinery::machine_say(world, mech, text);
}

/// `visible_message` within `COMBAT_MESSAGE_RANGE` (or the default range).
fn show(world: &mut World, actor: Entity, own: &str, others: &str, span: Span, combat: bool) {
    if combat {
        crate::medical::visible_range(world, actor, own, others, span, COMBAT_MESSAGE_RANGE);
    } else {
        crate::medical::visible(world, actor, own, others, span);
    }
}

/// `attack_self`: playing with it, 1.5 s apart.
pub fn attack_self(world: &mut World, mob: Entity, item: Entity) -> bool {
    let Some(mech) = world.get::<ToyMech>(item).cloned() else {
        return false;
    };
    let t = now(world);
    if mech.ready_at < t {
        let name = name_of(world, item);
        tell_mob(world, mob, Span::Notice, format!("Вы играете с {name}."));
        if let Some(mut m) = world.get_mut::<ToyMech>(item) {
            m.ready_at = t + COOLDOWN;
        }
        if !mech.kind.quiet {
            sound(world, mob, STEP_SOUND, 20.0);
        }
    }
    true
}

/// `check_battle_start(user, attacker, target)`: neither toy fights or rests.
fn check_battle_start(
    world: &mut World,
    user: Entity,
    mech: Entity,
    attacker: Option<Entity>,
    target: Option<Entity>,
) -> bool {
    let t = now(world);
    let state = |world: &World, e: Entity| world.get::<ToyMech>(e).cloned();
    let Some(ours) = state(world, mech) else {
        return false;
    };
    let theirs = attacker.and_then(|a| state(world, a));
    let refuse = |world: &mut World, to_user: String, to_target: String| {
        tell_mob(world, user, Span::Notice, to_user);
        if let Some(target) = target {
            tell_mob(world, target, Span::Notice, to_target);
        }
        false
    };
    let name = name_of(world, mech);
    if let (Some(a), Some(their)) = (attacker, &theirs) {
        let their_name = name_of(world, a);
        if their.in_combat {
            return refuse(
                world,
                format!("{their_name} сейчас в бою."),
                format!("Ваш {their_name} сейчас в бою."),
            );
        }
        if ours.in_combat {
            return refuse(
                world,
                format!("Ваш {name} сейчас в бою."),
                format!("{name} противника сейчас в бою."),
            );
        }
        if their.ready_at > t {
            return refuse(
                world,
                format!("{their_name} ещё не готов к бою."),
                format!("Ваш {their_name} ещё не готов к бою."),
            );
        }
    } else if ours.in_combat {
        return refuse(
            world,
            format!("Ваш {name} сейчас в бою."),
            format!("{name} противника сейчас в бою."),
        );
    }
    if ours.ready_at > t {
        return refuse(
            world,
            format!("Ваш {name} ещё не готов к бою."),
            format!("{name} противника ещё не готов к бою."),
        );
    }
    true
}

/// `item_interaction`: a toy mech used on another starts a brawl (`target` defends).
pub fn item_interaction(world: &mut World, mob: Entity, held: Entity, target: Entity) -> bool {
    if held == target
        || world.get::<ToyMech>(held).is_none()
        || world.get::<ToyMech>(target).is_none()
    {
        return false;
    }
    if check_battle_start(world, mob, target, Some(held), None) {
        mecha_brawl(world, target, held, mob, None);
    }
    true
}

/// `attack` of a toy mech on a human out of combat mode: the battle offer, or its acceptance.
pub fn attack(world: &mut World, mob: Entity, held: Entity, target: Entity) -> bool {
    let Some(mech) = world.get::<ToyMech>(held).cloned() else {
        return false;
    };
    let Some(theirs) = world.get::<Inventory>(target).map(|i| i.hands) else {
        return false;
    };
    if target == mob {
        tell_mob(
            world,
            mob,
            Span::Notice,
            "Направьте мех на другой игрушечный мех, если хотите сразиться сами с собой.",
        );
        return true;
    }
    if world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0)
    {
        return false;
    }
    let t = now(world);
    if mech.offer.is_some_and(|(until, _)| until > t) {
        tell_mob(world, mob, Span::Notice, "Вы уже предлагаете кому-то бой!");
        return true;
    }
    if !check_battle_start(world, mob, held, None, None) {
        return true;
    }
    for gundam in theirs.into_iter().flatten() {
        let Some(their) = world.get::<ToyMech>(gundam).cloned() else {
            continue;
        };
        if !check_battle_start(world, target, gundam, None, Some(mob)) {
            break;
        }
        // Slap them with the metaphorical white glove.
        if their.offer.is_some_and(|(until, _)| until > t) {
            mecha_brawl(world, held, gundam, target, Some(mob));
            if let Some(mut g) = world.get_mut::<ToyMech>(gundam) {
                g.offer = None;
            }
            return true;
        }
    }
    let (user_name, target_name, name) = (
        name_of(world, mob),
        name_of(world, target),
        name_of(world, held),
    );
    tell_mob(
        world,
        mob,
        Span::Notice,
        format!("Вы предлагаете бой {target_name}!"),
    );
    tell_mob(
        world,
        target,
        Span::Notice,
        format!(
            "{user_name} хочет сразиться с вами своим {name}! Ударьте его игрушечным мехом, \
             чтобы начать бой."
        ),
    );
    if let Some(mut m) = world.get_mut::<ToyMech>(held) {
        m.offer = Some((t + OFFER_SECS, mob));
    }
    true
}

/// `mecha_brawl(attacker, attacker_controller, opponent)` on `defender`: the collision; the
/// rounds follow in [`tick`].
pub fn mecha_brawl(
    world: &mut World,
    defender: Entity,
    attacker: Entity,
    attacker_controller: Entity,
    opponent: Option<Entity>,
) {
    let (user, a, d) = (
        name_of(world, attacker_controller),
        name_of(world, attacker),
        name_of(world, defender),
    );
    show(
        world,
        attacker_controller,
        &format!("Вы сталкиваете {a} с {d}, начиная яростную битву!"),
        &format!("{user} сталкивает {a} с {d}! Похоже, они готовятся к драке!"),
        Span::Danger,
        true,
    );
    let t = now(world);
    for e in [defender, attacker] {
        if let Some(mut m) = world.get_mut::<ToyMech>(e) {
            m.in_combat = true;
            m.ready_at = t + COOLDOWN * COOLDOWN_MULTIPLIER;
        }
    }
    world.entity_mut(defender).insert(Brawl {
        attacker,
        attacker_controller,
        opponent,
        length: 0,
        next_at: t + 1.5,
    });
}

fn incapacitated(world: &World, mob: Entity) -> bool {
    world.get::<crate::health::Stat>(mob) != Some(&crate::health::Stat::Conscious)
        || crate::status_effects::incapacitation(world, mob).incapacitated
}

/// `combat_sleep`'s checks: the toys next to each other, each controller capable and next to
/// its toy. False (with tg's message) ends the battle.
fn combat_check(world: &mut World, defender: Entity, brawl: &Brawl) -> bool {
    let (attacker, controller) = (brawl.attacker, brawl.attacker_controller);
    let (a, d) = (name_of(world, attacker), name_of(world, defender));
    if !in_range(world, defender, attacker) {
        let text = format!("{a} и {d} расходятся, бой окончен.");
        show(world, controller, &text, &text, Span::Notice, false);
        return false;
    }
    if incapacitated(world, controller) {
        return false;
    }
    let user = name_of(world, controller);
    if !in_range(world, attacker, controller) {
        show(
            world,
            controller,
            &format!("Вы отходите от {a}, бой окончен."),
            &format!("{user} отходит от {a}, бой окончен."),
            Span::Notice,
            false,
        );
        return false;
    }
    match brawl.opponent {
        Some(opponent) => {
            if incapacitated(world, opponent) {
                return false;
            }
            if !in_range(world, defender, opponent) {
                let them = name_of(world, opponent);
                show(
                    world,
                    opponent,
                    &format!("Вы отходите от {d}, бой окончен."),
                    &format!("{them} отходит от {d}, бой окончен."),
                    Span::Notice,
                    false,
                );
                return false;
            }
        }
        None if !in_range(world, defender, controller) => {
            show(
                world,
                controller,
                &format!("Вы разводите {a} и {d}, бой окончен."),
                &format!("{user} отходит от {d} и {a}, бой окончен."),
                Span::Notice,
                false,
            );
            return false;
        }
        None => {}
    }
    true
}

fn health(world: &World, mech: Entity) -> i32 {
    world.get::<ToyMech>(mech).map_or(0, |m| m.health)
}

fn hurt(world: &mut World, mech: Entity, amount: i32) {
    if let Some(mut m) = world.get_mut::<ToyMech>(mech) {
        m.health -= amount;
    }
}

/// Every tick: the brawls whose round is due.
fn tick(world: &mut World) {
    let t = now(world);
    let mut q = world.query::<(Entity, &ToyMech, Option<&Brawl>)>();
    let mut due = Vec::new();
    let mut expired = Vec::new();
    for (e, mech, brawl) in q.iter(world) {
        if let Some(b) = brawl
            && b.next_at <= t
        {
            due.push((e, *b));
        }
        if let Some((until, user)) = mech.offer
            && until <= t
        {
            expired.push((e, user));
        }
    }
    for (mech, user) in expired {
        if let Some(mut m) = world.get_mut::<ToyMech>(mech) {
            m.offer = None;
        }
        // `withdraw_offer`.
        tell_mob(
            world,
            user,
            Span::Notice,
            "Похоже, с вами не хотят сражаться.",
        );
    }
    for (defender, mut brawl) in due {
        let live = world.get_entity(brawl.attacker).is_ok() && world.get_entity(defender).is_ok();
        let over = !live
            || health(world, defender) <= 0
            || health(world, brawl.attacker) <= 0
            || brawl.length >= MAX_BATTLE_LENGTH
            || !combat_check(world, defender, &brawl);
        if over {
            finish(world, defender, &brawl);
            continue;
        }
        round(world, defender, &brawl);
        brawl.length += 1;
        brawl.next_at = t + 1.0;
        world.entity_mut(defender).insert(brawl);
    }
}

/// One round of the `while` loop.
fn round(world: &mut World, defender: Entity, brawl: &Brawl) {
    let attacker = brawl.attacker;
    let controller = brawl.attacker_controller;
    let src_controller = brawl.opponent.unwrap_or(controller);
    let (Some(ours), Some(theirs)) = (
        world.get::<ToyMech>(defender).cloned(),
        world.get::<ToyMech>(attacker).cloned(),
    ) else {
        return;
    };
    let (a, d) = (name_of(world, attacker), name_of(world, defender));
    if ours.special_charged {
        show(
            world,
            src_controller,
            &format!("Вы запускаете особую атаку {d}!"),
            &format!("{d} запускает свою особую атаку!!"),
            Span::Danger,
            false,
        );
        special_attack_move(world, defender, attacker);
        return;
    }
    if theirs.special_charged {
        show(
            world,
            controller,
            &format!("Вы запускаете особую атаку {a}!"),
            &format!("{a} запускает свою особую атаку!!"),
            Span::Danger,
            false,
        );
        special_attack_move(world, attacker, defender);
        return;
    }
    for e in [defender, attacker] {
        if let Some(mut m) = world.get_mut::<ToyMech>(e)
            && m.special_cooldown > 0
        {
            m.special_cooldown -= 1;
        }
    }
    let roll = world.get_resource_or_init::<CombatRng>().range(1, 8);
    match roll {
        1..=3 => strike(world, attacker, defender, controller, &a, &d),
        4 => {
            hurt(world, defender, 1);
            hurt(world, attacker, 1);
            for e in [defender, attacker] {
                if let Some(pos) = pos_of(world, e) {
                    crate::effects::particles::do_sparks(world, 2, false, pos);
                }
            }
            let text = if world.get_resource_or_init::<CombatRng>().chance(50.0) {
                format!("{a} и {d} эффектно сталкиваются, высекая искры!")
            } else {
                format!("{d} и {a} эффектно сталкиваются, высекая искры!")
            };
            show(world, controller, &text, &text, Span::Danger, true);
        }
        5 => {
            sound(world, attacker, "sound/items/weapons/parry.ogg", 20.0);
            let text = if world.get_resource_or_init::<CombatRng>().chance(50.0) {
                format!("Атака {d} отражается от {a}.")
            } else {
                format!("Атака {a} отражается от {d}.")
            };
            show(world, controller, &text, &text, Span::Danger, true);
        }
        _ => strike(world, defender, attacker, src_controller, &d, &a),
    }
}

/// One side's hit (`1 to 3` for the attacker, `6 to 8` for the defender): at a third of its
/// health with its special ready it charges it instead.
fn strike(world: &mut World, hitter: Entity, victim: Entity, controller: Entity, h: &str, v: &str) {
    let Some(mech) = world.get::<ToyMech>(hitter).cloned() else {
        return;
    };
    if mech.special_cooldown == 0 && mech.health <= mech.kind.max_health / 3 {
        if let Some(mut m) = world.get_mut::<ToyMech>(hitter) {
            m.special_charged = true;
        }
        show(
            world,
            controller,
            &format!("Вы начинаете заряжать особую атаку {h}!"),
            &format!("{h} начинает заряжать свою особую атаку!!"),
            Span::Danger,
            false,
        );
        return;
    }
    sound(world, hitter, STEP_SOUND, 30.0);
    hurt(world, victim, 1);
    show(
        world,
        controller,
        &format!("Вы врезаете {h} в {v}!"),
        &format!("{h} сокрушает {v}!"),
        Span::Danger,
        true,
    );
    if world.get_resource_or_init::<CombatRng>().chance(5.0) {
        hurt(world, victim, 1);
        sound(world, victim, "sound/effects/meteorimpact.ogg", 20.0);
        show(
            world,
            controller,
            &format!("...и вы наносите {v} СОКРУШИТЕЛЬНЫЙ удар!"),
            "...и наносит СОКРУШИТЕЛЬНЫЙ УДАР!",
            Span::BoldWarning,
            true,
        );
    }
}

/// `special_attack_move(victim)`.
fn special_attack_move(world: &mut World, mech: Entity, victim: Entity) {
    let Some(kind) = world.get::<ToyMech>(mech).map(|m| m.kind) else {
        return;
    };
    say(world, mech, &format!("{}!!", kind.cry));
    if let Some(mut m) = world.get_mut::<ToyMech>(mech) {
        m.special_charged = false;
        m.special_cooldown = 3;
    }
    let heal = |world: &mut World, amount: i32| hurt(world, mech, -amount);
    match kind.special {
        Special::Damage => {
            hurt(world, victim, 2);
            sound(world, mech, "sound/items/weapons/marauder.ogg", 20.0);
        }
        Special::Heal => {
            heal(world, 2);
            sound(
                world,
                mech,
                "sound/vehicles/mecha/mech_shield_raise.ogg",
                20.0,
            );
        }
        Special::Utility => {
            hurt(world, victim, 1);
            heal(world, 1);
            sound(world, mech, "sound/vehicles/mecha/mechmove01.ogg", 30.0);
        }
        Special::Honk => {
            sound(
                world,
                mech,
                "sound/mobs/non-humanoids/honkbot/honkbot_evil_laugh.ogg",
                20.0,
            );
            if let Some(mut v) = world.get_mut::<ToyMech>(victim) {
                v.special_cooldown += 3;
            }
            heal(world, 1);
        }
        Special::Execute => {
            sound(
                world,
                mech,
                "sound/items/weapons/sonic_jackhammer.ogg",
                20.0,
            );
            if health(world, victim) < health(world, mech) {
                say(world, mech, "EXECUTE!!");
                if let Some(mut v) = world.get_mut::<ToyMech>(victim) {
                    v.health = 0;
                }
            } else {
                hurt(world, victim, 1);
            }
        }
        Special::Reticence => {
            if let Some(mut m) = world.get_mut::<ToyMech>(mech) {
                m.special_cooldown -= 1;
            }
            if let Some(mut v) = world.get_mut::<ToyMech>(victim) {
                v.special_cooldown += 1;
                v.health -= 1;
            }
        }
    }
}

/// The end of `mecha_brawl`: the result, the counts, health back to full.
fn finish(world: &mut World, defender: Entity, brawl: &Brawl) {
    world.entity_mut(defender).remove::<Brawl>();
    let attacker = brawl.attacker;
    let controller = brawl.attacker_controller;
    let src_controller = brawl.opponent.unwrap_or(controller);
    let (a, d) = (name_of(world, attacker), name_of(world, defender));
    let (ours, theirs) = (health(world, defender), health(world, attacker));
    let line = {
        let mut rng = world.get_resource_or_init::<CombatRng>();
        WIN_LINES[rng.range(0, WIN_LINES.len() as u32 - 1) as usize]
    };
    let add = |world: &mut World, mech: Entity, win: bool| {
        if let Some(mut m) = world.get_mut::<ToyMech>(mech) {
            if win {
                m.wins += 1;
            } else {
                m.losses += 1;
            }
        }
    };
    if theirs <= 0 && ours <= 0 {
        sound(world, defender, "sound/machines/warning-buzzer.ogg", 20.0);
        show(
            world,
            controller,
            &format!("И {d}, и {a} уничтожены!"),
            &format!("ВЗАИМНОЕ ГАРАНТИРОВАННОЕ УНИЧТОЖЕНИЕ!! {d} и {a} уничтожают друг друга!"),
            Span::BoldNotice,
            false,
        );
    } else if theirs <= 0 {
        add(world, defender, true);
        add(world, attacker, false);
        sound(world, attacker, "sound/effects/light_flicker.ogg", 20.0);
        let text = format!("{a} разваливается!");
        show(world, controller, &text, &text, Span::Notice, true);
        say(world, defender, line);
        show(
            world,
            src_controller,
            &format!("Вы победно поднимаете {d} над {a}!"),
            &format!("{d} уничтожает {a} и уходит победителем!"),
            Span::Notice,
            false,
        );
    } else if ours <= 0 {
        add(world, attacker, true);
        add(world, defender, false);
        sound(world, defender, "sound/effects/light_flicker.ogg", 20.0);
        let text = format!("{d} рушится!");
        show(world, src_controller, &text, &text, Span::Notice, true);
        say(world, attacker, line);
        show(
            world,
            controller,
            &format!("Вы гордо поднимаете {a} над {d}!"),
            &format!("{a} разносит {d} и уходит победителем!"),
            Span::Notice,
            false,
        );
    } else {
        say(world, defender, "NEXT TIME.");
        let quiet = world.get::<ToyMech>(defender).is_some_and(|m| m.kind.quiet);
        say(
            world,
            attacker,
            if quiet {
                "I WENT EASY ON YOU."
            } else {
                "OF COURSE."
            },
        );
    }
    for e in [defender, attacker] {
        if let Some(mut m) = world.get_mut::<ToyMech>(e) {
            m.in_combat = false;
            m.health = m.kind.max_health;
        }
    }
}

/// tg `examine` of a toy mech.
pub fn examine_lines(world: &World, item: Entity, lines: &mut Vec<(Span, String)>) {
    let Some(mech) = world.get::<ToyMech>(item) else {
        return;
    };
    lines.push((
        Span::Notice,
        format!(
            "Особая атака этой игрушки — {}, {}",
            mech.kind.cry,
            mech.kind.special_text()
        ),
    ));
    if mech.in_combat {
        lines.push((
            Span::Notice,
            format!(
                "Максимальное здоровье игрушки — {}. Сейчас — {}.",
                mech.kind.max_health, mech.health
            ),
        ));
        let light = if mech.special_cooldown != 0 {
            "мигает красным."
        } else {
            "горит зелёным, приём готов!"
        };
        lines.push((Span::Notice, format!("Индикатор особого приёма {light}")));
    } else {
        lines.push((
            Span::Notice,
            format!("Максимальное здоровье игрушки — {}.", mech.kind.max_health),
        ));
    }
    if mech.wins > 0 || mech.losses > 0 {
        lines.push((
            Span::Notice,
            format!("Побед: {}, поражений: {}.", mech.wins, mech.losses),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toy_mech_kinds() {
        let ripley = MechKind::of_path("/obj/item/toy/mecha/ripley").unwrap();
        assert_eq!((ripley.max_health, ripley.special), (4, Special::Damage));
        // `ripleymkii` is not under `ripley`.
        let mkii = MechKind::of_path("/obj/item/toy/mecha/ripleymkii").unwrap();
        assert_eq!(mkii.cry, "GIGA DRILL BREAK");
        let reticence = MechKind::of_path("/obj/item/toy/mecha/reticence").unwrap();
        assert!(reticence.quiet);
        assert_eq!(
            MechKind::of_path("/obj/item/toy/mecha/deathripley")
                .unwrap()
                .special,
            Special::Execute
        );
        assert!(MechKind::of_path("/obj/item/toy/plush").is_none());
    }

    fn mech(world: &mut World, path: &str, at: TilePos) -> Entity {
        let kind = MechKind::of_path(path).unwrap();
        world.spawn((ToyMech::new(kind), at)).id()
    }

    #[test]
    fn a_brawl_runs_to_a_result() {
        let mut world = World::new();
        world.init_resource::<Time>();
        world.init_resource::<bevy::ecs::message::Messages<PlaySound>>();
        world.insert_resource(CombatRng(7));
        let at = TilePos::new(5, 5, 0);
        let controller = world.spawn((at, crate::health::Stat::Conscious)).id();
        let defender = mech(&mut world, "/obj/item/toy/mecha/ripley", at);
        let attacker = mech(&mut world, "/obj/item/toy/mecha/hauler", at);
        assert!(check_battle_start(
            &mut world,
            controller,
            defender,
            Some(attacker),
            None
        ));
        mecha_brawl(&mut world, defender, attacker, controller, None);
        assert!(world.get::<ToyMech>(defender).unwrap().in_combat);
        // A busy toy cannot start another.
        assert!(!check_battle_start(
            &mut world, controller, defender, None, None
        ));
        let brawl = *world.get::<Brawl>(defender).unwrap();
        for _ in 0..MAX_BATTLE_LENGTH {
            round(&mut world, defender, &brawl);
        }
        finish(&mut world, defender, &brawl);
        let (d, a) = (
            world.get::<ToyMech>(defender).unwrap().clone(),
            world.get::<ToyMech>(attacker).unwrap().clone(),
        );
        assert!(!d.in_combat && !a.in_combat && world.get::<Brawl>(defender).is_none());
        assert_eq!((d.health, a.health), (4, 3), "health resets");
        assert_eq!(d.wins + d.losses, a.wins + a.losses);
    }
}
