//! Holding someone at gunpoint (server): tg `/datum/component/gunpoint`
//! (`code/datums/components/gunpoint.dm`), `/datum/status_effect/holdup` and
//! `/datum/status_effect/grouped/heldup` (`status_effects/neutral.dm`).
//!
//! A right click with a gun (combat mode off) on a living mob within
//! [`GUNPOINT_SHOOTER_STRAY_RANGE`] tiles aims at it after half a second (`do_after`). Both
//! flinch (`Immobilize(0.75 s / distance)`); the shooter gets the "holding up" alert (a click
//! lowers the gun), the target the "held up" and surrender alerts and the `gunpoint` moodlet.
//! The aim steadies over time: the charged shot deals ×1.25, after 2.5 s ×1.75, after another
//! 7.5 s ×2.25 (BandaStation's values), plus that many times 5 wound bonus. The target moving,
//! attacking a mob or firing a gun pulls the trigger; the shooter losing sight of it (past the
//! range), or the gun leaving the shooter's hands, lowers the gun.
// shortcut: no flinch shot when the shooter is hurt, no bump blocking, no examine lines, no
// cancel on being pulled or disarmed, no chime and no `gaspshock` emote; `can_hold_up = FALSE`
// guns (3 in tg) still hold up. Add them as the mechanics need.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, Span};
use ss13_tgui::server::UiActed;

use crate::breath::{AlertCategory, Alerts, clicked_alert};
use crate::click::{ClickResolved, RangedClick};
use crate::guns::{Aim, ChamberBoost, Gun};
use crate::health::{COMBAT_MESSAGE_RANGE, Health, Stat};
use crate::hud::AlertHudUi;
use crate::inventory::Inventory;
use crate::mood::MoodEvent;
use crate::security::{name_of, show};
use crate::status_effects::{self, Incap};
use crate::tools::tell;

/// `GUNPOINT_SHOOTER_STRAY_RANGE` (BandaStation: 4 instead of 2).
pub const GUNPOINT_SHOOTER_STRAY_RANGE: u32 = 4;
/// `GUNPOINT_DELAY_STAGE_2`, `GUNPOINT_DELAY_STAGE_3`, seconds.
const DELAY_STAGE_2: f64 = 2.5;
const DELAY_STAGE_3: f64 = 7.5;
/// `GUNPOINT_MULT_STAGE_1..3` (BandaStation: 1.75 and 2.25 for the later stages).
pub const STAGE_MULT: [f32; 3] = [1.25, 1.75, 2.25];
/// `GUNPOINT_BASE_WOUND_BONUS`.
const BASE_WOUND_BONUS: f32 = 5.0;
/// `interact_with_atom_secondary`'s `do_after(user, 0.5 SECONDS, interacting_with)`.
pub const AIM_DELAY_DS: f32 = 5.0;

/// `/datum/mood_event/gunpoint`.
const GUNPOINT_MOOD: MoodEvent = MoodEvent {
    id: "gunpoint",
    description: "Он сошёл с ума! Мне нужно быть осторожным...",
    mood_change: -10.0,
    timeout: 0.0,
    hidden: false,
};

/// `/datum/component/gunpoint` on the shooter.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Gunpoint {
    pub client: Entity,
    pub target: Entity,
    pub weapon: Entity,
    /// 1 to 3.
    pub stage: u8,
    /// When the next stage begins.
    pub next_stage_at: f64,
    /// Where the target stood when it was aimed at: leaving it pulls the trigger.
    pub target_pos: TilePos,
}

impl Gunpoint {
    /// `damage_mult` of the current stage.
    pub fn damage_mult(&self) -> f32 {
        STAGE_MULT[usize::from(self.stage.clamp(1, 3) - 1)]
    }
}

pub fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        holdup_alert_clicks.in_set(ss13_tgui::server::UiSystems::Handle),
    );
}

/// tg `/obj/item/gun/interact_with_atom_secondary` without combat mode: starts aiming. True
/// when the click is used up.
pub fn start(world: &mut World, client: Entity, mob: Entity, gun: Entity, target: Entity) -> bool {
    if world.get::<Health>(target).is_none() {
        return false;
    }
    if let Some(held) = world.get::<Gunpoint>(mob) {
        let whom = if held.target == target {
            "его"
        } else {
            "кого-то"
        };
        tell(
            world,
            client,
            Span::Warning,
            format!("вы уже держите {whom} на прицеле!"),
        );
        return true;
    }
    if mob == target {
        tell(
            world,
            client,
            Span::Warning,
            "вы не можете держать себя на прицеле!",
        );
        return true;
    }
    let actor = crate::tools::Actor {
        client,
        mob,
        item: Some(gun),
    };
    crate::tools::do_after(
        world,
        actor,
        target,
        AIM_DELAY_DS,
        0,
        crate::construction::Job::Gunpoint,
    );
    true
}

/// `AddComponent(/datum/component/gunpoint)` after the `do_after`.
pub fn complete_aim(world: &mut World, mob: Entity, action: &crate::tools::DoAfter) {
    let (target, Some(gun)) = (action.target, action.item) else {
        return;
    };
    if world.get::<Gunpoint>(mob).is_some() || world.get::<Health>(target).is_none() {
        return;
    }
    let (Some(&from), Some(&at)) = (world.get::<TilePos>(mob), world.get::<TilePos>(target)) else {
        return;
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    world.entity_mut(mob).insert(Gunpoint {
        client: action.client,
        target,
        weapon: gun,
        stage: 1,
        next_stage_at: now + DELAY_STAGE_2,
        target_pos: at,
    });
    let distance = from.chebyshev(at).unwrap_or(1).max(1);
    let blank = if distance <= 1 { "в упор " } else { "" };
    let (shooter, weapon, victim) = (
        name_of(world, mob),
        crate::obj_damage::strip(&name_of(world, gun)).to_owned(),
        name_of(world, target),
    );
    show(
        world,
        from,
        COMBAT_MESSAGE_RANGE,
        ChatChannel::Combat,
        (
            Span::Danger,
            format!("{shooter} целится из {weapon} {blank}в {victim}!"),
        ),
        &[
            (
                mob,
                Span::Danger,
                format!("Вы целитесь из {weapon} {blank}в {victim}!"),
            ),
            (
                target,
                Span::BoldWarning,
                format!("{shooter} целится из {weapon} {blank}в вас!"),
            ),
        ],
    );
    let flinch = 0.75 / distance as f32;
    status_effects::apply(world, mob, Incap::Immobilize, flinch);
    status_effects::apply(world, target, Incap::Immobilize, flinch);
    if let Some(mut alerts) = world.get_mut::<Alerts>(mob) {
        alerts.throw(AlertCategory::Holdup, "aimed", 0);
    }
    // `heldup` -> `grouped/surrender`.
    if let Some(mut alerts) = world.get_mut::<Alerts>(target) {
        alerts.throw(AlertCategory::Heldup, "aimed", 0);
        alerts.throw(AlertCategory::Surrender, "surrender", 0);
    }
    crate::mood::add_event(world, target, "gunpoint", &GUNPOINT_MOOD);
}

/// `Destroy`: the alerts and the moodlet go (the target's only when no one else aims at it).
fn end(world: &mut World, mob: Entity) {
    let Some(gunpoint) = world.get::<Gunpoint>(mob).copied() else {
        return;
    };
    world.entity_mut(mob).remove::<Gunpoint>();
    if let Some(mut alerts) = world.get_mut::<Alerts>(mob)
        && alerts.get(AlertCategory::Holdup).is_some()
    {
        alerts.clear(AlertCategory::Holdup);
    }
    let target = gunpoint.target;
    let mut others = world.query::<&Gunpoint>();
    if others.iter(world).any(|g| g.target == target) {
        return;
    }
    let timed = world.get::<crate::buffs::SurrenderPrompt>(target).is_some();
    if let Some(mut alerts) = world.get_mut::<Alerts>(target) {
        if alerts.get(AlertCategory::Heldup).is_some() {
            alerts.clear(AlertCategory::Heldup);
        }
        if !timed && alerts.get(AlertCategory::Surrender).is_some() {
            alerts.clear(AlertCategory::Surrender);
        }
    }
    if world.get_entity(target).is_ok() {
        crate::mood::clear_event(world, target, "gunpoint");
    }
}

/// tg `cancel`: the shooter lowers the gun.
pub fn cancel(world: &mut World, mob: Entity) {
    let Some(gunpoint) = world.get::<Gunpoint>(mob).copied() else {
        return;
    };
    if let Some(&at) = world.get::<TilePos>(mob) {
        let (shooter, weapon, victim) = (
            name_of(world, mob),
            crate::obj_damage::strip(&name_of(world, gunpoint.weapon)).to_owned(),
            name_of(world, gunpoint.target),
        );
        show(
            world,
            at,
            COMBAT_MESSAGE_RANGE,
            ChatChannel::Combat,
            (
                Span::Danger,
                format!("{shooter} перестаёт целиться в {victim}!"),
            ),
            &[
                (
                    mob,
                    Span::Danger,
                    format!("Вы больше не целитесь из {weapon} в {victim}."),
                ),
                (
                    gunpoint.target,
                    Span::BoldWarning,
                    format!("{shooter} перестаёт целиться в вас!"),
                ),
            ],
        );
    }
    end(world, mob);
}

/// tg `trigger_reaction`: the charged shot at the target.
pub fn trigger(world: &mut World, mob: Entity) {
    let Some(gunpoint) = world.get::<Gunpoint>(mob).copied() else {
        return;
    };
    end(world, mob);
    let Some(&target_pos) = world.get::<TilePos>(gunpoint.target) else {
        return;
    };
    let mult = gunpoint.damage_mult();
    world.entity_mut(gunpoint.weapon).insert(ChamberBoost {
        damage_mult: mult,
        wound_bonus: mult * BASE_WOUND_BONUS,
        exposed_wound_bonus: mult * BASE_WOUND_BONUS,
        head: false,
    });
    let aim = Aim {
        target: gunpoint.target,
        target_pos,
        pixel: [0, 0],
    };
    crate::guns::fire_gun(world, gunpoint.client, mob, gunpoint.weapon, aim);
    // A shot that did not go off keeps the round as it was.
    if let Ok(mut gun) = world.get_entity_mut(gunpoint.weapon) {
        gun.remove::<ChamberBoost>();
    }
}

/// Every tick: the stages, the target's moves and actions, the shooter's sight and hands.
pub fn upkeep(
    world: &mut World,
    mut clicks: Local<MessageCursor<ClickResolved>>,
    mut ranged: Local<MessageCursor<RangedClick>>,
) {
    // The target attacks a mob (`COMSIG_MOB_ATTACK_HAND`, `COMSIG_MOB_ITEM_ATTACK`) or fires
    // a gun (`COMSIG_MOB_FIRED_GUN`).
    let mut acted: Vec<Entity> = clicks
        .read(world.resource::<Messages<ClickResolved>>())
        .filter(|c| c.target != c.mob && world.get::<Health>(c.target).is_some())
        .map(|c| c.mob)
        .collect();
    acted.extend(
        ranged
            .read(world.resource::<Messages<RangedClick>>())
            .filter(|c| {
                world
                    .get::<Inventory>(c.mob)
                    .and_then(Inventory::active_item)
                    .is_some_and(|g| world.get::<Gun>(g).is_some())
            })
            .map(|c| c.mob),
    );
    let mut query = world.query::<(Entity, &Gunpoint)>();
    let aiming: Vec<(Entity, Gunpoint)> = query.iter(world).map(|(e, g)| (e, *g)).collect();
    if aiming.is_empty() {
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    for (mob, gunpoint) in aiming {
        let target = gunpoint.target;
        if world.get_entity(target).is_err() || world.get_entity(mob).is_err() {
            end(world, mob);
            continue;
        }
        // `COMSIG_ITEM_DROPPED`, `COMSIG_ITEM_EQUIPPED`: the gun left the hands.
        let holding = world
            .get::<Inventory>(mob)
            .is_some_and(|i| i.hands.contains(&Some(gunpoint.weapon)));
        let conscious = world.get::<Stat>(mob) == Some(&Stat::Conscious);
        // `check_deescalate`: `can_see(parent, target, GUNPOINT_SHOOTER_STRAY_RANGE)`.
        let sees = match (world.get::<TilePos>(mob), world.get::<TilePos>(target)) {
            (Some(&from), Some(&to)) => {
                crate::mobs::ai::behaviors::can_see(world, from, to, GUNPOINT_SHOOTER_STRAY_RANGE)
            }
            _ => false,
        };
        if !holding || !conscious || !sees {
            cancel(world, mob);
            continue;
        }
        let moved = world.get::<TilePos>(target) != Some(&gunpoint.target_pos);
        if moved || acted.contains(&target) {
            trigger(world, mob);
            continue;
        }
        if gunpoint.stage < 3 && now >= gunpoint.next_stage_at {
            update_stage(world, mob, gunpoint.stage + 1, now);
        }
    }
}

/// tg `update_stage`.
fn update_stage(world: &mut World, mob: Entity, stage: u8, now: f64) {
    let Some(mut gunpoint) = world.get::<Gunpoint>(mob).copied() else {
        return;
    };
    gunpoint.stage = stage;
    gunpoint.next_stage_at = now + DELAY_STAGE_3;
    world.entity_mut(mob).insert(gunpoint);
    let (shooter, weapon, victim) = (
        name_of(world, mob),
        crate::obj_damage::strip(&name_of(world, gunpoint.weapon)).to_owned(),
        name_of(world, gunpoint.target),
    );
    let (own, theirs) = if stage == 2 {
        (
            format!("Вы твёрже наводите {weapon} на {victim}."),
            format!("{shooter} твёрже наводит {weapon} на вас!"),
        )
    } else {
        (
            format!("Вы полностью навели {weapon} на {victim}."),
            format!("{shooter} полностью наводит {weapon} на вас!"),
        )
    };
    if let Some(c) = world.get::<crate::server::PlayerMob>(mob).map(|p| p.client) {
        tell(world, c, Span::Danger, own);
    }
    if let Some(c) = world
        .get::<crate::server::PlayerMob>(gunpoint.target)
        .map(|p| p.client)
    {
        tell(world, c, Span::BoldWarning, theirs);
    }
}

/// `alert/status_effect/holdup/Click`: `gunpoint.cancel()`.
fn holdup_alert_clicks(world: &mut World, mut acts: Local<MessageCursor<UiActed<AlertHudUi>>>) {
    let acts: Vec<(Entity, Entity, crate::breath::AlertClick)> = acts
        .read(world.resource::<Messages<UiActed<AlertHudUi>>>())
        .map(|a| (a.actor, a.source, a.act.clone()))
        .collect();
    for (actor, source, click) in acts {
        if actor == source && clicked_alert(world, actor, &click) == Some(AlertCategory::Holdup) {
            cancel(world, actor);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_aim_steadies_by_stage() {
        let mut g = Gunpoint {
            client: Entity::PLACEHOLDER,
            target: Entity::PLACEHOLDER,
            weapon: Entity::PLACEHOLDER,
            stage: 1,
            next_stage_at: 0.0,
            target_pos: TilePos::new(0, 0, 0),
        };
        assert_eq!(g.damage_mult(), 1.25);
        g.stage = 3;
        assert_eq!(g.damage_mult(), 2.25);
    }
}
