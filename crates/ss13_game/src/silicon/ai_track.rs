//! AI camera tracking (server), after tg `/datum/trackable` (`code/game/machinery/camera/trackable.dm`)
//! and the AI's `on_track_target`:
//!
//! - "Track With Camera" lists the trackable mobs (tg `find_trackable_mobs`: living, not the AI
//!   itself, visible, on a turf, on a tile the cameras see), humans first, each group sorted, a
//!   repeated name as "name (2)". The chosen one is followed: the eye goes to its tile whenever
//!   it moves and the cameras still see it.
//! - Out of the cameras' sight the AI is told "Target is not near any active cameras. Attempting
//!   to reacquire...", then tries again every second (`SSprocessing`); after 10 failures
//!   "Unable to reacquire, cancelling track...". A deleted or dead target, a dead AI, or the eye
//!   moved by anything else (steps, the core button, a camera jump) ends the tracking.
//! - Not trackable (tg `can_track`): an invisible or faded (alpha 50 or less) mob, one not on a
//!   turf (in a closet, a mech, someone's hands), on the syndicate base's level, a morph, or
//!   one wearing an agent ID card or an abductor helmet.
// shortcut: no "track" verb by name (there is no verb bar), no double click on a mob, no
// glide-size following. No CentCom or away-level exclusion (the port has no level traits for
// them). The changeling's and heretic's digital camouflage (no such powers yet) and the
// ninja's camera vision module (`crate::modsuit::ninja`, the modsuit group) do not block yet.
// Consoles track only through SecurEye (`docs/parity/comms/ntos-apps.md`).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_net::server::Invisibility;
use ss13_shared::chat::Span;
use ss13_shared::protocol::InventorySlot;
use ss13_shared::{Appearance, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems};
use ss13_tgui::server::UiSystems;

use super::ai::{AiCore, AiEye};
use super::cameras::CameraNet;
use super::chassis::name_of;
use super::notice;
use crate::health::{Health, Stat};
use crate::human::HumanLook;
use crate::inventory::Inventory;

/// tg `CAMERA_TICK_LIMIT`.
pub const CAMERA_TICK_LIMIT: u32 = 10;
/// `SSprocessing` wait: one recheck per second.
const RECHECK_SECS: f64 = 1.0;

/// On the AI eye: who it follows (tg `tracked_mob`, `rechecking`, `cameraticks`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct AiTracking {
    pub target: Entity,
    /// Where the eye was put last: an eye found elsewhere was moved by something else.
    pub placed_at: Option<TilePos>,
    /// The target's tile when last looked at.
    pub target_at: Option<TilePos>,
    pub rechecking: bool,
    pub ticks: u32,
    pub next_check: f64,
}

#[derive(Component, Clone, Debug)]
struct TrackPrompt {
    eye: Entity,
    targets: Vec<Entity>,
}

pub struct AiTrackPlugin;

impl Plugin for AiTrackPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                track_chosen
                    .in_set(UiSystems::Handle)
                    .after(PromptSystems::Answer),
                follow.after(ss13_world::WorldSystems::Movement),
            ),
        );
    }
}

/// What makes a mob untrackable while worn (`COMSIG_LIVING_CAN_TRACK` -> `COMPONENT_CANT_TRACK`):
/// an agent ID card in the ID slot, an abductor helmet on the head.
const UNTRACKABLE_GEAR: [(InventorySlot, &str); 2] = [
    (InventorySlot::Id, "/obj/item/card/id/advanced/chameleon"),
    (
        InventorySlot::Head,
        "/obj/item/clothing/head/helmet/abductor",
    ),
];

/// tg `/mob/living/can_track(user)`, `user` being the AI core.
pub fn can_track(world: &World, target: Entity, ai: Entity) -> bool {
    if target == ai || world.get::<Health>(target).is_none() {
        return false;
    }
    if let Some(inventory) = world.get::<Inventory>(target) {
        let blocked = UNTRACKABLE_GEAR.iter().any(|&(slot, path)| {
            inventory
                .slot(slot)
                .and_then(|item| crate::emp::path_of(world, item))
                .is_some_and(|p| is_under(&p, path))
        });
        if blocked {
            return false;
        }
    }
    // `/mob/living/basic/morph/can_track`: never.
    if world.get::<crate::antag::morph::Morph>(target).is_some() {
        return false;
    }
    // `invisibility || alpha <= 50`: cloaked.
    let faded = world
        .get::<Appearance>(target)
        .and_then(|a| a.color)
        .is_some_and(|c| c[3] <= 50);
    if faded || world.get::<Invisibility>(target).is_some_and(|i| i.0 > 0) {
        return false;
    }
    // `!isturf(loc)`: in a closet, a mech or someone's hands.
    if world.get::<crate::closets::InCloset>(target).is_some()
        || world.get::<crate::mecha::Pilot>(target).is_some()
        || world.get::<crate::inventory::CarriedBy>(target).is_some()
    {
        return false;
    }
    let Some(&pos) = world.get::<TilePos>(target) else {
        return false;
    };
    // `onSyndieBase() && !(ROLE_SYNDICATE in user.faction)`: the AI is no syndicate.
    if world
        .get_resource::<crate::antag::nukeop::server::Infiltrator>()
        .is_some_and(|i| i.level_z == pos.z)
    {
        return false;
    }
    world
        .get_resource::<CameraNet>()
        .is_some_and(|net| net.visible(pos))
}

/// tg `find_trackable_mobs`: names with repeats numbered, humans first, each group sorted.
pub fn trackable_names(mut mobs: Vec<(String, bool, Entity)>) -> Vec<(String, Entity)> {
    let mut seen: Vec<(String, u32)> = Vec::new();
    for (name, _, _) in &mut mobs {
        let mut unique = name.clone();
        while let Some(entry) = seen.iter_mut().find(|(n, _)| *n == unique) {
            entry.1 += 1;
            unique = format!("{name} ({})", entry.1);
        }
        seen.push((unique.clone(), 1));
        *name = unique;
    }
    mobs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    mobs.into_iter().map(|(n, _, e)| (n, e)).collect()
}

/// The "Track With Camera" button (tg `track_input`).
pub fn ask(world: &mut World, client: Entity, eye: Entity) {
    let Some(ai) = world.get::<AiEye>(eye).map(|e| e.ai) else {
        return;
    };
    if world.get::<Stat>(ai) == Some(&Stat::Dead) {
        return;
    }
    let z = world.get::<TilePos>(eye).map(|p| p.z);
    let mut living = world.query_filtered::<(Entity, &TilePos, Has<HumanLook>), With<Health>>();
    let candidates: Vec<(Entity, bool)> = living
        .iter(world)
        .filter(|(_, p, _)| Some(p.z) == z)
        .map(|(e, _, human)| (e, human))
        .collect();
    let mobs: Vec<(String, bool, Entity)> = candidates
        .into_iter()
        .filter(|&(e, _)| can_track(world, e, ai))
        .map(|(e, human)| (name_of(world, e), human, e))
        .collect();
    let list = trackable_names(mobs);
    let prompt = ss13_tgui::prompt::ask_world(
        world,
        Ask::new(
            client,
            eye,
            "Tracking",
            "Select a target",
            PromptKind::List {
                items: list.iter().map(|(n, _)| n.clone()).collect(),
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(TrackPrompt {
        eye,
        targets: list.into_iter().map(|(_, e)| e).collect(),
    });
}

fn track_chosen(world: &mut World, mut answers: Local<MessageCursor<PromptAnswered>>) {
    let pending: Vec<(Entity, Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.answer.clone()))
        .collect();
    for (prompt, client, answer) in pending {
        let Some(asked) = world.get::<TrackPrompt>(prompt).cloned() else {
            continue;
        };
        let Some(PromptAnswer::Item(i)) = answer else {
            continue;
        };
        let Some(&target) = asked.targets.get(i) else {
            continue;
        };
        let Some(ai) = world.get::<AiEye>(asked.eye).map(|e| e.ai) else {
            continue;
        };
        if world.get::<Stat>(ai) == Some(&Stat::Dead) || world.get_entity(target).is_err() {
            notice(
                world,
                client,
                Span::Notice,
                "Target is not on or near any active cameras. Tracking failed.",
            );
            continue;
        }
        let now = world.resource::<Time>().elapsed_secs_f64();
        let mut tracking = AiTracking {
            target,
            placed_at: None,
            target_at: None,
            rechecking: false,
            ticks: 0,
            next_check: now,
        };
        attempt(world, asked.eye, ai, &mut tracking);
        if tracking.placed_at.is_none() {
            // tg `set_tracked_mob`: a first failed attempt waits for the target to move.
            tracking.target_at = world.get::<TilePos>(target).copied();
            tracking.placed_at = world.get::<TilePos>(asked.eye).copied();
        }
        world.entity_mut(asked.eye).insert(tracking);
    }
}

/// tg `attempt_track` + `on_track_target`: the eye onto the target when the cameras see it.
fn attempt(world: &mut World, eye: Entity, ai: Entity, tracking: &mut AiTracking) -> bool {
    if !can_track(world, tracking.target, ai) {
        return false;
    }
    let Some(&pos) = world.get::<TilePos>(tracking.target) else {
        return false;
    };
    tracking.rechecking = false;
    tracking.target_at = Some(pos);
    tracking.placed_at = Some(pos);
    if world.get::<TilePos>(eye) != Some(&pos) {
        world.entity_mut(eye).insert(pos);
    }
    true
}

/// Each tick for each tracking eye: a moved target is followed, a lost one rechecked once a
/// second.
fn follow(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut query = world.query::<(Entity, &AiEye, &AiTracking, &TilePos)>();
    let eyes: Vec<(Entity, Entity, AiTracking, TilePos)> = query
        .iter(world)
        .map(|(e, eye, t, p)| (e, eye.ai, *t, *p))
        .collect();
    for (eye, ai, mut tracking, eye_pos) in eyes {
        let target_alive = world
            .get::<Stat>(tracking.target)
            .is_some_and(|s| *s != Stat::Dead);
        let ai_alive = world.get::<Stat>(ai).is_some_and(|s| *s != Stat::Dead)
            && world.get::<AiCore>(ai).is_some();
        let moved_away = tracking.placed_at.is_some_and(|p| p != eye_pos);
        if !target_alive || !ai_alive || moved_away {
            world.entity_mut(eye).remove::<AiTracking>();
            continue;
        }
        let target_pos = world.get::<TilePos>(tracking.target).copied();
        if tracking.rechecking {
            if now < tracking.next_check {
                continue;
            }
            tracking.next_check = now + RECHECK_SECS;
            if attempt(world, eye, ai, &mut tracking) {
                world.entity_mut(eye).insert(tracking);
                continue;
            }
            if tracking.ticks < CAMERA_TICK_LIMIT {
                if tracking.ticks == 0 {
                    tell_eye(
                        world,
                        eye,
                        "Target is not near any active cameras. Attempting to reacquire...",
                    );
                }
                tracking.ticks += 1;
                world.entity_mut(eye).insert(tracking);
            } else {
                tell_eye(world, eye, "Unable to reacquire, cancelling track...");
                world.entity_mut(eye).remove::<AiTracking>();
            }
            continue;
        }
        if target_pos == tracking.target_at {
            continue;
        }
        // tg `target_moved`: follow, else start rechecking.
        if !attempt(world, eye, ai, &mut tracking) {
            tracking.target_at = target_pos;
            tracking.rechecking = true;
            tracking.ticks = 0;
            tracking.next_check = now;
        }
        world.entity_mut(eye).insert(tracking);
    }
}

fn tell_eye(world: &mut World, eye: Entity, text: &str) {
    if let Some(client) = world.get::<crate::server::PlayerMob>(eye).map(|p| p.client) {
        notice(world, client, Span::Warning, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trackable_names_number_repeats_and_put_humans_first() {
        let e = |i: u32| Entity::from_raw_u32(i).unwrap();
        let list = trackable_names(vec![
            ("mouse".into(), false, e(1)),
            ("Bob".into(), true, e(2)),
            ("Bob".into(), true, e(3)),
            ("Alice".into(), true, e(4)),
        ]);
        let names: Vec<&str> = list.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["Alice", "Bob", "Bob (2)", "mouse"]);
        assert_eq!(list[2].1, e(3));
    }
}
