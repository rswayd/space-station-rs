//! Resting (server), after tg `code/modules/mob/living/living.dm` (`toggle_resting`,
//! `set_resting`, `get_up`, `rest_checks_callback`): Shift+B (BandaStation's `rest` keybinding)
//! or the HUD rest button (`/atom/movable/screen/rest`, `act_rest`/`act_rest_on`).
//!
//! - Lying down is at once ("Вы ложитесь."). Lying by choice works like any lying: the mob is
//!   drawn lying, is not dense, crawls (`carbon_crawling`) and cannot pull (`TRAIT_PULL_BLOCKED`,
//!   `crate::pull`); its hands stay free.
//! - Standing up takes 1 s (`get_up`, which ignores moves and the held item); resting again or
//!   being floored meanwhile cancels it. A mob floored by something else when it stops resting
//!   stands when that ends, at once like every other floored mob here.
// shortcut: standing up after a knockdown is instant (tg `on_floored_end` -> `get_up` takes 1 s
// too); no `TRAIT_FORCED_STANDING` and no buckling that forces a posture; add with those.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_net::server::IntentReceived;
use ss13_shared::chat::Span;
use ss13_shared::protocol::ClientIntent;

use crate::health::{BodyPosition, Stat};
use crate::server::Players;
use crate::status_effects::{Incapacitation, StatusEffects};
use crate::tools::tell;

/// `get_up_time`, seconds.
pub const GET_UP_SECS: f64 = 1.0;

pub struct RestPlugin;

impl Plugin for RestPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (rest_intents, get_up).chain());
    }
}

/// tg `resting`, on a living mob, and the lying it chose: `down` until standing up finishes.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Rest {
    pub resting: bool,
    pub down: bool,
    /// When standing up finishes (`get_up`'s `do_after`).
    pub up_at: Option<f64>,
}

fn rest_intents(world: &mut World, mut cursor: Local<MessageCursor<IntentReceived>>) {
    let toggles: Vec<Entity> = cursor
        .read(world.resource::<Messages<IntentReceived>>())
        .filter(|i| i.intent == ClientIntent::ToggleRest)
        .map(|i| i.client)
        .collect();
    for client in toggles {
        let Some(mob) = world.resource::<Players>().mob(client) else {
            continue;
        };
        let resting = world.get::<Rest>(mob).is_some_and(|r| r.resting);
        set_resting(world, Some(client), mob, !resting);
    }
}

/// Whether something other than resting floors `mob` (tg `TRAIT_FLOORED`).
fn floored(world: &World, mob: Entity) -> bool {
    world.get::<Incapacitation>(mob).is_some_and(|t| t.floored)
        || world
            .get::<StatusEffects>(mob)
            .is_some_and(StatusEffects::floored)
}

/// tg `set_resting(new_resting, silent = !client)`: a conscious living mob only (tg
/// `MOBILITY_REST`).
pub fn set_resting(world: &mut World, client: Option<Entity>, mob: Entity, resting: bool) {
    if world.get::<Stat>(mob) != Some(&Stat::Conscious) {
        return;
    }
    let mut rest = world.get::<Rest>(mob).copied().unwrap_or_default();
    if rest.resting == resting {
        return;
    }
    rest.resting = resting;
    let lying = world.get::<BodyPosition>(mob).is_some_and(|b| b.lying());
    let text = if resting {
        rest.up_at = None;
        rest.down = true;
        if lying {
            "Вы будете пытаться лежать на полу."
        } else {
            "Вы ложитесь."
        }
    } else if !lying {
        rest.down = false;
        "Вы будете пытаться встать с пола."
    } else if floored(world, mob) {
        rest.down = false;
        "Вы встанете с пола как только это станет возможным."
    } else {
        let now = world.resource::<Time>().elapsed_secs_f64();
        rest.up_at = Some(now + GET_UP_SECS);
        "Вы встаете."
    };
    world.entity_mut(mob).insert(rest);
    if let Some(client) = client {
        tell(world, client, Span::Notice, text);
    }
}

/// tg `get_up` after its `do_after`: not resting again (`rest_checks_callback`), the chosen
/// lying ends; a mob floored meanwhile keeps lying for that reason and stands when it ends.
fn get_up(mut mobs: Query<&mut Rest>, time: Res<Time>) {
    let now = time.elapsed_secs_f64();
    for mut rest in &mut mobs {
        if rest.up_at.is_some_and(|t| now >= t) {
            rest.up_at = None;
            if !rest.resting {
                rest.down = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    fn tick(world: &mut World) {
        world.run_system_once(super::get_up).unwrap();
    }

    fn world() -> (World, Entity) {
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        world.insert_resource(Players::default());
        let mob = world.spawn((Stat::Conscious, BodyPosition::default())).id();
        (world, mob)
    }

    fn advance(world: &mut World, secs: f64) {
        world
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f64(secs));
    }

    #[test]
    fn lies_down_at_once_and_gets_up_after_a_second() {
        let (mut world, mob) = world();
        set_resting(&mut world, None, mob, true);
        assert_eq!(
            world.get::<Rest>(mob).map(|r| (r.resting, r.down)),
            Some((true, true))
        );
        world
            .entity_mut(mob)
            .insert(BodyPosition { lying_angle: 90 });
        set_resting(&mut world, None, mob, false);
        tick(&mut world);
        assert!(world.get::<Rest>(mob).unwrap().down, "still getting up");
        advance(&mut world, GET_UP_SECS);
        tick(&mut world);
        assert!(!world.get::<Rest>(mob).unwrap().down);
    }

    #[test]
    fn resting_again_keeps_the_mob_down() {
        let (mut world, mob) = world();
        world
            .entity_mut(mob)
            .insert(BodyPosition { lying_angle: 90 });
        set_resting(&mut world, None, mob, true);
        set_resting(&mut world, None, mob, false);
        set_resting(&mut world, None, mob, true);
        advance(&mut world, GET_UP_SECS);
        tick(&mut world);
        assert!(world.get::<Rest>(mob).unwrap().down, "rested again");

        // Floored meanwhile: the chosen lying still ends (the knockdown keeps the mob down).
        set_resting(&mut world, None, mob, false);
        let mut effects = StatusEffects::default();
        effects.knockdown = 5.0;
        world.entity_mut(mob).insert(effects);
        advance(&mut world, GET_UP_SECS);
        tick(&mut world);
        assert!(!world.get::<Rest>(mob).unwrap().down);
    }

    #[test]
    fn the_unconscious_do_not_toggle() {
        let (mut world, mob) = world();
        world.entity_mut(mob).insert(Stat::SoftCrit);
        set_resting(&mut world, None, mob, true);
        assert!(world.get::<Rest>(mob).is_none());
    }
}
