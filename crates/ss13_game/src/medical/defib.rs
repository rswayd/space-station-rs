//! Defibrillators (server), after tg `code/game/objects/items/tools/medical/defib.dm`
//! (`/obj/item/defibrillator`, `/obj/item/shockpaddles` `attack`, `do_help`, `do_harm`,
//! `do_disarm`), `carbon.dm` (`can_defib`, `revive`) and the organ decay of a corpse
//! (`_organ.dm` `on_death`, `STANDARD_ORGAN_DECAY`).
//!
//! - A defibrillator worn in its slot (back, or belt for the compact ones) clicked with an empty
//!   hand puts its paddles into that hand (tg `toggle_paddles`); the paddles snap back when they
//!   leave the hand or the unit leaves the wearer. A unit `/loaded` with a cell has 10 shocks
//!   (`revivecost` = a tenth of a standard cell); the combat one recharges forever.
//! - Paddles wielded (the other hand empty) on someone's chest: `do_help` (3 s placing, 2 s
//!   charging). A dead patient comes back if the brute and burn are each under 180 and the heart
//!   and brain have not decayed (a corpse's heart fails after about 6 minutes at body
//!   temperature, `decay_factor` 2.5 × `STANDARD_ORGAN_DECAY`; the brain after 30): the damage
//!   is brought to `HALFWAYCRITDEATH` (-50, oxygen first), the mob revives in crit and its ghost
//!   returns. The unit then recharges for `cooldown_duration` (5 s).
//! - In combat mode, with a combat defibrillator (or one with its safety off): `do_harm`
//!   (1.5 s + 1.5 s) burns the chest for 50 and paralyzes for 10 s; right click knocks down.
// shortcut: no heart attacks (`set_heartattack`, cardiac arrest), so a living patient is "not in
// a valid state" and a harm shock does not stop the heart; no jitter, convulsions, mood, sounds,
// shocking whoever pulls the patient, suicide/DNR/husk checks, emag, EMP, cell swapping, wall
// mounts; paddles count as wielded while the other hand is empty (no two-handed component).

use bevy::prelude::*;
use ss13_mapload::{DmClothing, DmDefib, DmItem};
use ss13_shared::chat::Span;

use super::{MedJob, name_of, visible};
use crate::combat::CombatMode;
use crate::health::{
    DamageType, HEALTH_THRESHOLD_CRIT, HEALTH_THRESHOLD_DEAD, Health, Stat, ZoneSelected,
};
use crate::inventory::{CarriedBy, Inventory};
use crate::obj_damage::is_under;
use crate::tools::{Actor, DoAfter, do_after, tell};
use crate::zone::{BodyZone, TargetZone};

/// `HALFWAYCRITDEATH`.
pub const HALFWAYCRITDEATH: f32 = (HEALTH_THRESHOLD_CRIT + HEALTH_THRESHOLD_DEAD) * 0.5;
/// `MAX_REVIVE_BRUTE_DAMAGE`, `MAX_REVIVE_FIRE_DAMAGE`.
pub const MAX_REVIVE_DAMAGE: f32 = 180.0;
/// `STANDARD_CELL_CHARGE` (joules) and the paddles' `revivecost`.
pub const STANDARD_CELL_CHARGE: f32 = 10_000.0;
pub const REVIVE_COST: f32 = STANDARD_CELL_CHARGE * 0.1;
/// `STANDARD_ORGAN_DECAY`, `STANDARD_ORGAN_HEALING`, per second of the organ's max health.
pub const STANDARD_ORGAN_DECAY: f32 = 111.0 / 100_000.0;
pub const STANDARD_ORGAN_HEALING: f32 = 50.0 / 100_000.0;
/// Heart (`STANDARD_ORGAN_THRESHOLD`) and brain (`BRAIN_DAMAGE_DEATH`) max health.
pub const HEART_MAX_HEALTH: f32 = 100.0;
pub const BRAIN_MAX_HEALTH: f32 = 200.0;
const T0C: f64 = 273.15;
const THICKMATERIAL: u32 = 1 << 4;
const CHEST: u32 = 1 << 0;
pub const DEFIB_ROOT: &str = "/obj/item/defibrillator";
pub const PADDLES_ROOT: &str = "/obj/item/shockpaddles";

/// The heart and brain damage a corpse accrues (tg organ `damage`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct OrganDecay {
    pub heart: f32,
    pub brain: f32,
}

impl OrganDecay {
    /// tg `handle_organs`: organs of the dead decay (by body temperature), of the living heal.
    pub fn tick(&mut self, dead: bool, bodytemperature: f64, secs: f32) {
        if dead {
            if bodytemperature > T0C {
                let factor = ((bodytemperature - T0C) / 20.0).min(1.0) as f32;
                self.heart = (self.heart
                    + 2.5 * STANDARD_ORGAN_DECAY * HEART_MAX_HEALTH * secs * factor)
                    .min(HEART_MAX_HEALTH);
                self.brain = (self.brain
                    + 0.5 * STANDARD_ORGAN_DECAY * BRAIN_MAX_HEALTH * secs * factor)
                    .min(BRAIN_MAX_HEALTH);
            }
        } else if !self.heart_failing() {
            self.heart = (self.heart - STANDARD_ORGAN_HEALING * HEART_MAX_HEALTH * secs).max(0.0);
            self.brain = (self.brain - STANDARD_ORGAN_HEALING * BRAIN_MAX_HEALTH * secs).max(0.0);
        }
    }

    /// `ORGAN_FAILING`: damage at the organ's max health.
    pub fn heart_failing(&self) -> bool {
        self.heart >= HEART_MAX_HEALTH
    }

    pub fn brain_failing(&self) -> bool {
        self.brain >= BRAIN_MAX_HEALTH
    }
}

/// Why `can_defib` refuses (tg `DEFIB_FAIL_*`), with the unit's buzz.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefibFail {
    TissueDamage,
    FailingHeart,
    FailingBrain,
}

impl DefibFail {
    pub fn reason(self) -> &'static str {
        match self {
            Self::TissueDamage => "Tissue damage too severe, repair and try again.",
            Self::FailingHeart => "Patient's heart too damaged, replace or repair and try again.",
            Self::FailingBrain => "Patient's brain is too damaged, repair and try again.",
        }
    }
}

/// tg `can_defib` of a human.
pub fn can_defib(health: &Health, organs: OrganDecay) -> Result<(), DefibFail> {
    if health.brute >= MAX_REVIVE_DAMAGE || health.burn >= MAX_REVIVE_DAMAGE {
        return Err(DefibFail::TissueDamage);
    }
    if organs.heart_failing() {
        return Err(DefibFail::FailingHeart);
    }
    if organs.brain_failing() {
        return Err(DefibFail::FailingBrain);
    }
    Ok(())
}

/// The damage changes of a successful `do_help` on a corpse: up to crit again with oxygen
/// damage, or every damage type healed in proportion down to `HALFWAYCRITDEATH`.
pub fn defib_heal(health: &mut Health) {
    let current = health.health();
    if current > HALFWAYCRITDEATH {
        health.adjust(DamageType::Oxy, current - HALFWAYCRITDEATH);
        return;
    }
    let (brute, burn, tox, oxy) = (health.brute, health.burn, health.tox, health.oxy);
    let overall = brute + burn + tox + oxy;
    if overall <= 0.0 {
        return;
    }
    let change = current - HALFWAYCRITDEATH;
    health.adjust(DamageType::Oxy, change * (oxy / overall));
    health.adjust(DamageType::Tox, change * (tox / overall));
    health.adjust(DamageType::Burn, change * (burn / overall));
    health.adjust(DamageType::Brute, change * (brute / overall));
}

/// A defibrillator unit's live state.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Defib {
    /// `(charge, maxcharge)` of its cell; `None`: no cell.
    pub cell: Option<(f32, f32)>,
    pub infinite: bool,
    pub safety: bool,
    pub combat: bool,
    /// The paddles out in a hand.
    pub paddles: Option<Entity>,
    /// `Time::elapsed_secs_f64` the paddles' `cooldown` ends.
    pub cooldown_until: f64,
    pub cooldown_secs: f64,
    pub paddle_type: String,
}

impl Defib {
    /// tg `powered`.
    pub fn powered(&self) -> bool {
        self.infinite || self.cell.is_some_and(|(charge, _)| charge >= REVIVE_COST)
    }

    /// `DEFIB_CAN_HURT`.
    pub fn can_hurt(&self) -> bool {
        self.combat || !self.safety
    }
}

/// On paddles: the unit they belong to.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Paddles(pub Entity);

/// The unit's state from its type (tg `Initialize` of `/loaded` types gives a cell; the combat
/// one an infinite cell).
fn add_defib_state(
    add: On<Add<DmDefib>>,
    defibs: Query<(&DmDefib, &DmItem)>,
    mut commands: Commands,
) {
    let Ok((dm, item)) = defibs.get(add.entity) else {
        return;
    };
    let loaded = item.path.contains("/loaded");
    let infinite = is_under(&item.path, "/obj/item/defibrillator/compact/combat/loaded");
    commands.entity(add.entity).insert(Defib {
        cell: loaded.then_some((STANDARD_CELL_CHARGE, STANDARD_CELL_CHARGE)),
        infinite,
        safety: true,
        combat: dm.combat,
        paddles: None,
        cooldown_until: 0.0,
        cooldown_secs: f64::from(dm.cooldown_duration) / 10.0,
        paddle_type: if dm.paddle_type.is_empty() {
            PADDLES_ROOT.to_owned()
        } else {
            dm.paddle_type.clone()
        },
    });
}

/// tg `defibrillator/attack_hand` from its slot: the paddles come out into the empty active hand
/// or go back. True when `worn` is a defibrillator.
pub fn attack_hand_worn(world: &mut World, client: Entity, mob: Entity, worn: Entity) -> bool {
    let Some(defib) = world.get::<Defib>(worn).cloned() else {
        return false;
    };
    toggle_paddles(world, client, mob, worn, defib);
    true
}

fn toggle_paddles(world: &mut World, client: Entity, mob: Entity, unit: Entity, mut defib: Defib) {
    if let Some(paddles) = defib.paddles.take() {
        world.try_despawn(paddles).ok();
        world.entity_mut(unit).insert(defib);
        return;
    }
    let Some(inventory) = world.get::<Inventory>(mob).cloned() else {
        return;
    };
    if inventory.active_item().is_some() {
        tell(
            world,
            client,
            Span::Warning,
            "You need a free hand to hold the paddles!",
        );
        return;
    }
    let Some(&pos) = world.get::<ss13_shared::TilePos>(mob) else {
        return;
    };
    let Some(paddles) = crate::construction::spawn_obj(world, &defib.paddle_type, pos) else {
        warn!("no prototype for {}", defib.paddle_type);
        return;
    };
    world.entity_mut(paddles).insert(Paddles(unit));
    crate::inventory::pick_up(world, mob, paddles, inventory.active);
    defib.paddles = Some(paddles);
    world.entity_mut(unit).insert(defib);
}

/// tg `shockpaddles/dropped` and `check_range`: paddles out of the hand of the unit's wearer go
/// back into the unit. Cost: per tick, the paddles that are out (a few).
fn snap_back(
    paddles: Query<(Entity, &Paddles, Option<&CarriedBy>)>,
    inventories: Query<&Inventory>,
    mut defibs: Query<&mut Defib>,
    mut commands: Commands,
) {
    for (entity, &Paddles(unit), carried) in &paddles {
        let held = carried.is_some_and(|c| {
            inventories
                .get(c.0)
                .is_ok_and(|inv| inv.hands.contains(&Some(entity)) && inv.holds(unit))
        });
        if held {
            continue;
        }
        commands.entity(entity).despawn();
        if let Ok(mut defib) = defibs.get_mut(unit) {
            defib.paddles = None;
        }
    }
}

/// tg `shockpaddles/attack` on a living mob. True when the click was the paddles'.
pub fn paddles_attack(
    world: &mut World,
    actor: Actor,
    paddles: Entity,
    target: Entity,
    right: bool,
) -> bool {
    let Some(&Paddles(unit)) = world.get::<Paddles>(paddles) else {
        return false;
    };
    let Some(defib) = world.get::<Defib>(unit).cloned() else {
        return true;
    };
    if world.get::<DoAfter>(actor.mob).is_some() {
        return true;
    }
    let unit_name = name_of(world, unit);
    if !defib.powered() {
        let text = format!("{unit_name} beeps: Not enough charge!");
        visible(world, actor.mob, &text, &text, Span::Warning);
        return true;
    }
    if !is_wielded(world, actor.mob) {
        tell(
            world,
            actor.client,
            Span::Warning,
            "You need to wield the paddles in both hands before you can use them on someone!",
        );
        return true;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now < defib.cooldown_until {
        tell(
            world,
            actor.client,
            Span::Warning,
            format!("{unit_name} is recharging!"),
        );
        return true;
    }
    if right {
        if defib.can_hurt() {
            do_disarm(world, actor, unit, target);
        }
        return true;
    }
    let zone = world
        .get::<ZoneSelected>(actor.mob)
        .copied()
        .unwrap_or_default()
        .0;
    if zone != TargetZone::Chest {
        let item = name_of(world, paddles);
        tell(
            world,
            actor.client,
            Span::Warning,
            format!("You need to target your patient's chest with {item}!"),
        );
        return true;
    }
    let (user, victim, item) = (
        name_of(world, actor.mob),
        name_of(world, target),
        name_of(world, paddles),
    );
    if world.get::<CombatMode>(actor.mob).is_some_and(|c| c.0) {
        if !defib.can_hurt() {
            return true;
        }
        visible(
            world,
            actor.mob,
            &format!("You overcharge the paddles and begin to place them onto {victim}'s chest..."),
            &format!("{user} begins to place {item} on {victim}'s chest."),
            Span::Warning,
        );
        do_after(
            world,
            actor,
            target,
            15.0,
            0,
            crate::construction::Job::Medical(MedJob::Defib {
                unit,
                stage: Stage::HarmPlace,
            }),
        );
        return true;
    }
    // tg grabs the ghost back into a body that can be revived before the shock.
    if world.get::<Stat>(target) == Some(&Stat::Dead) && revivable(world, target).is_ok() {
        crate::ghost::return_to_body(world, target);
    }
    visible(
        world,
        actor.mob,
        &format!("You begin to place {item} on {victim}'s chest..."),
        &format!("{user} begins to place {item} on {victim}'s chest."),
        Span::Warning,
    );
    do_after(
        world,
        actor,
        target,
        30.0,
        0,
        crate::construction::Job::Medical(MedJob::Defib {
            unit,
            stage: Stage::HelpPlace,
        }),
    );
    true
}

/// tg paddles `is_wielded` (the `extra_checks` of every shock stage): the other hand is free.
pub fn is_wielded(world: &World, mob: Entity) -> bool {
    world
        .get::<Inventory>(mob)
        .is_some_and(|inv| inv.hands.iter().filter(|h| h.is_some()).count() == 1)
}

/// The `do_after` stages of a shock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    HelpPlace,
    HelpCharge,
    HarmPlace,
    HarmCharge,
}

fn revivable(world: &World, mob: Entity) -> Result<(), DefibFail> {
    let health = world.get::<Health>(mob).cloned().unwrap_or_default();
    let organs = world.get::<OrganDecay>(mob).copied().unwrap_or_default();
    can_defib(&health, organs)
}

/// The end of a shock stage.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, unit: Entity, stage: Stage) {
    let actor = Actor {
        client: action.client,
        mob,
        item: action.item,
    };
    let target = action.target;
    let Some(paddles) = actor
        .item
        .filter(|&p| world.get::<Paddles>(p).is_some_and(|p| p.0 == unit))
    else {
        return;
    };
    let Some(defib) = world.get::<Defib>(unit).cloned() else {
        return;
    };
    let unit_name = name_of(world, unit);
    let (user, victim, item) = (
        name_of(world, mob),
        name_of(world, target),
        name_of(world, paddles),
    );
    match stage {
        Stage::HelpPlace => {
            visible(
                world,
                mob,
                &format!("You place {item} on {victim}'s chest."),
                &format!("{user} places {item} on {victim}'s chest."),
                Span::Notice,
            );
            do_after(
                world,
                actor,
                target,
                20.0,
                0,
                crate::construction::Job::Medical(MedJob::Defib {
                    unit,
                    stage: Stage::HelpCharge,
                }),
            );
        }
        Stage::HarmPlace => {
            visible(
                world,
                mob,
                &format!("You place {item} on {victim}'s chest and begin to charge them."),
                &format!("{user} places {item} on {victim}'s chest."),
                Span::Warning,
            );
            do_after(
                world,
                actor,
                target,
                15.0,
                0,
                crate::construction::Job::Medical(MedJob::Defib {
                    unit,
                    stage: Stage::HarmCharge,
                }),
            );
        }
        Stage::HelpCharge => do_help(world, actor, unit, &defib, target, &unit_name),
        Stage::HarmCharge => {
            if world.get::<Stat>(target) == Some(&Stat::Dead) {
                tell(
                    world,
                    actor.client,
                    Span::Warning,
                    format!("{victim} is dead."),
                );
                return;
            }
            visible(
                world,
                mob,
                &format!("You shock {victim} with {item}!"),
                &format!("{user} shocks {victim} with {item}!"),
                Span::Danger,
            );
            hurt(world, target, BodyZone::Chest, DamageType::Burn, 50.0);
            crate::status_effects::apply(
                world,
                target,
                crate::status_effects::Incap::Paralyze,
                10.0,
            );
            success(world, unit);
        }
    }
}

fn do_help(
    world: &mut World,
    actor: Actor,
    unit: Entity,
    defib: &Defib,
    target: Entity,
    unit_name: &str,
) {
    if !defib.combat {
        let obscured = world.get::<Inventory>(target).is_some_and(|inv| {
            inv.equipped().any(|item| {
                world.get::<DmClothing>(item).is_some_and(|c| {
                    c.body_parts_covered & CHEST != 0 && c.clothing_flags & THICKMATERIAL != 0
                })
            })
        });
        if obscured {
            let text =
                format!("{unit_name} buzzes: Patient's chest is obscured. Operation aborted.");
            visible(world, actor.mob, &text, &text, Span::Warning);
            return;
        }
    }
    if world.get::<Stat>(target) != Some(&Stat::Dead) {
        let text =
            format!("{unit_name} buzzes: Patient is not in a valid state. Operation aborted.");
        visible(world, actor.mob, &text, &text, Span::Warning);
        return;
    }
    let victim = name_of(world, target);
    let text = format!("{victim}'s body convulses a bit.");
    visible(world, actor.mob, &text, &text, Span::Warning);
    if let Err(fail) = revivable(world, target) {
        let text = format!(
            "{unit_name} buzzes: Resuscitation failed - {}",
            fail.reason()
        );
        visible(world, actor.mob, &text, &text, Span::Warning);
        success(world, unit);
        return;
    }
    let Some(mut health) = world.get::<Health>(target).cloned() else {
        return;
    };
    defib_heal(&mut health);
    world.entity_mut(target).insert(health.clone());
    let text = format!("{unit_name} pings: Resuscitation successful.");
    visible(world, actor.mob, &text, &text, Span::Notice);
    crate::ghost::return_to_body(world, target);
    revive(world, target);
    let text = format!("{victim} задыхается!");
    visible(world, target, &text, &text, Span::Emote);
    success(world, unit);
}

/// tg `/mob/living/revive` without healing: a dead mob whose health is above
/// `HEALTH_THRESHOLD_DEAD` gets up to the stat its health gives (tg sets `HARD_CRIT`, then
/// `updatehealth`).
pub fn revive(world: &mut World, mob: Entity) -> bool {
    let Some(health) = world.get::<Health>(mob).cloned() else {
        return false;
    };
    if world.get::<Stat>(mob) != Some(&Stat::Dead) || health.health() <= HEALTH_THRESHOLD_DEAD {
        return false;
    }
    let stat = Stat::for_health(health.health());
    world.entity_mut(mob).insert(stat);
    if let Some(mut alerts) = world.get_mut::<crate::breath::Alerts>(mob) {
        alerts.0.clear();
    }
    true
}

fn do_disarm(world: &mut World, actor: Actor, unit: Entity, target: Entity) {
    let (user, victim, item) = (
        name_of(world, actor.mob),
        name_of(world, target),
        actor.item.map(|i| name_of(world, i)).unwrap_or_default(),
    );
    visible(
        world,
        actor.mob,
        &format!("{user} touches {victim} with {item}!"),
        &format!("{user} touches {victim} with {item}!"),
        Span::Danger,
    );
    crate::status_effects::adjust_stamina(world, target, 60.0);
    crate::status_effects::knockdown(world, target, 7.5, 0.0);
    success(world, unit);
}

fn hurt(world: &mut World, mob: Entity, zone: BodyZone, kind: DamageType, amount: f32) {
    let (Some(mut health), Some(&stat)) =
        (world.get::<Health>(mob).cloned(), world.get::<Stat>(mob))
    else {
        return;
    };
    let mut new = stat;
    crate::health::apply_damage_zone(&mut health, &mut new, zone, kind, amount);
    world.entity_mut(mob).insert(health);
    if new != stat {
        world.entity_mut(mob).insert(new);
    }
}

/// tg `on_defib_success`: the charge and the recharge.
fn success(world: &mut World, unit: Entity) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(mut defib) = world.get_mut::<Defib>(unit) {
        if !defib.infinite
            && let Some((charge, _)) = &mut defib.cell
        {
            *charge = (*charge - REVIVE_COST).max(0.0);
        }
        defib.cooldown_until = now + defib.cooldown_secs;
    }
}

pub(super) fn build(app: &mut App) {
    app.add_observer(add_defib_state)
        .add_systems(Update, snap_back.after(ss13_world::WorldSystems::Movement));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn organ_decay_sets_the_defib_window() {
        let mut organs = OrganDecay::default();
        // 2 s ticks at body temperature: the heart fails after ~360 s.
        let mut secs = 0;
        while !organs.heart_failing() {
            organs.tick(true, 310.15, 2.0);
            secs += 2;
        }
        assert!((358..=362).contains(&secs), "{secs}");
        assert!(!organs.brain_failing());
        // A cold corpse does not decay.
        let mut cold = OrganDecay::default();
        cold.tick(true, 270.0, 1000.0);
        assert_eq!(cold, OrganDecay::default());
    }

    #[test]
    fn defib_heal_puts_the_body_halfway_to_death() {
        // Died of suffocation then patched up: oxygen brings it back to -50.
        let mut health = Health::default();
        health.adjust(DamageType::Brute, 20.0);
        defib_heal(&mut health);
        assert!(
            (health.health() - HALFWAYCRITDEATH).abs() < 0.11,
            "{}",
            health.health()
        );
        // Beaten to -120: every type healed in proportion to -50.
        let mut health = Health::default();
        health.adjust(DamageType::Brute, 160.0);
        health.adjust(DamageType::Oxy, 60.0);
        defib_heal(&mut health);
        assert!(
            (health.health() - HALFWAYCRITDEATH).abs() < 0.2,
            "{}",
            health.health()
        );
        assert!(health.brute < 160.0 && health.oxy < 60.0);
        assert_eq!(can_defib(&health, OrganDecay::default()), Ok(()));
        let mut torn = Health::default();
        torn.adjust(DamageType::Burn, 190.0);
        assert_eq!(
            can_defib(&torn, OrganDecay::default()),
            Err(DefibFail::TissueDamage)
        );
        let decayed = OrganDecay {
            heart: 100.0,
            brain: 0.0,
        };
        assert_eq!(
            can_defib(&Health::default(), decayed),
            Err(DefibFail::FailingHeart)
        );
    }
}
