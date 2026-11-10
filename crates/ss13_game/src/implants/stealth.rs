//! The S3 (stealth) implant (tg `implant_stealth.dm`, `/datum/action/item_action/agent_box`):
//! its button folds the agent's cardboard box (`/obj/structure/closet/cardboard/agent`) around
//! its host on a tile, or opens the box it is in. The box fades out over 2 s; a living mob
//! bumping it (or bumped by it) shows it again for 1 s before it fades anew; opened, it is
//! gone.
// shortcut: the box does not bump doors open for the mob inside, and there is no suicide act;
// its `alpha` is set server-side step by step instead of a client `animate`.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, TilePos};
use ss13_world::Bumped;

use super::{Implants, implants_of};
use crate::closets::InCloset;
use crate::tools::tell_mob;

pub const AGENT_BOX: &str = "/obj/structure/closet/cardboard/agent";
const BOX_SOUND: &str = "sound/misc/box_deploy.ogg";
/// `box_cooldown`.
const BOX_COOLDOWN_SECS: f64 = 10.0;
/// `go_invisible`: `animate(src, alpha = 0, time = 20)`.
const FADE_SECS: f64 = 2.0;
/// `reveal`: `addtimer(go_invisible, 10)`.
const REVEAL_SECS: f64 = 1.0;

/// An agent box: when it starts (or started) fading out.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct AgentBox {
    pub fade_from: f64,
}

pub(super) fn build(app: &mut App) {
    app.add_systems(
        Update,
        (
            reveal_on_bump.after(ss13_world::WorldSystems::Movement),
            fade_boxes,
        )
            .chain()
            .run_if(any_with_component::<AgentBox>),
    );
}

/// `isturf(owner.loc)`: not in a closet (or pod), a pipe, a mech or someone's hands.
fn on_turf(world: &World, mob: Entity) -> bool {
    world.get::<InCloset>(mob).is_none()
        && world.get::<crate::ventcrawl::Ventcrawling>(mob).is_none()
        && world.get::<crate::mecha::Pilot>(mob).is_none()
        && world.get::<crate::inventory::CarriedBy>(mob).is_none()
        && world.get::<TilePos>(mob).is_some()
}

/// `agent_box/do_effect` (`AB_CHECK_INCAPACITATED|HANDS_BLOCKED|IMMOBILE|CONSCIOUS`, the last
/// checked by the button): in an agent box it opens it; on a tile, once per 10 s, a new box
/// closes around the host.
pub(super) fn activate(world: &mut World, mob: Entity, id: u32) {
    let traits = crate::status_effects::incapacitation(world, mob);
    if traits.incapacitated || traits.hands_blocked || traits.immobilized {
        return;
    }
    if let Some(closet) = world.get::<InCloset>(mob).map(|c| c.closet)
        && world.get::<AgentBox>(closet).is_some()
    {
        if crate::closets::open(world, closet) {
            play_to_host(world, mob);
        }
        return;
    }
    if !on_turf(world, mob) {
        tell_mob(
            world,
            mob,
            Span::Warning,
            "Нужно больше места, чтобы активировать этот имплант!",
        );
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(ready_at) = implants_of(world, mob)
        .iter()
        .find(|i| i.id == id)
        .map(|i| i.ready_at)
    else {
        return;
    };
    if now < ready_at {
        return;
    }
    if let Some(mut implants) = world.get_mut::<Implants>(mob)
        && let Some(imp) = implants.0.iter_mut().find(|i| i.id == id)
    {
        imp.ready_at = now + BOX_COOLDOWN_SECS;
    }
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    if deploy(world, mob, pos, now).is_some() {
        play_to_host(world, mob);
    }
}

/// `new boxtype(owner.drop_location())` + `owner.forceMove(box)`.
fn deploy(world: &mut World, mob: Entity, pos: TilePos, now: f64) -> Option<Entity> {
    let closet = crate::construction::spawn_obj(world, AGENT_BOX, pos)?;
    if !crate::closets::kinds::make_closet(world, closet) {
        world.despawn(closet);
        return None;
    }
    world
        .entity_mut(closet)
        .insert((AgentBox { fade_from: now }, ss13_world::Density));
    crate::closets::put_mob_inside(world, mob, closet);
    Some(closet)
}

/// `owner.playsound_local(box, 'sound/misc/box_deploy.ogg', 50, TRUE)`: only the host hears it.
fn play_to_host(world: &mut World, mob: Entity) {
    let at = world.get::<TilePos>(mob).copied();
    crate::hallucination::play_local(world, mob, at, BOX_SOUND, 50.0, true);
}

/// `agent/reveal`: fully seen now, fading again a second later (`TIMER_OVERRIDE`).
pub fn reveal(world: &mut World, closet: Entity) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(mut b) = world.get_mut::<AgentBox>(closet) {
        b.fade_from = now + REVEAL_SECS;
    }
}

/// `agent/Bump` of a box moved by its mob (`crate::closets::kinds::cardboard_step`): a living
/// blocker reveals it.
pub fn bumped_into(world: &mut World, closet: Entity, blockers: &[Entity]) {
    if world.get::<AgentBox>(closet).is_some()
        && blockers
            .iter()
            .any(|&b| world.get::<crate::health::Health>(b).is_some())
    {
        reveal(world, closet);
    }
}

/// `agent/Bumped`: a living mob walking into the box (or the mob in it) reveals it.
fn reveal_on_bump(world: &mut World, mut cursor: Local<MessageCursor<Bumped>>) {
    let bumps: Vec<Bumped> = cursor
        .read(world.resource::<Messages<Bumped>>())
        .copied()
        .collect();
    for bump in bumps {
        let closet = world
            .get::<InCloset>(bump.obstacle)
            .map_or(bump.obstacle, |c| c.closet);
        if world.get::<AgentBox>(closet).is_some()
            && world.get::<crate::health::Health>(bump.entity).is_some()
        {
            reveal(world, closet);
        }
    }
}

/// The box's alpha by its fade (`go_invisible`); an opened box is gone (`agent/after_open`).
fn fade_boxes(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let boxes: Vec<(Entity, f64, bool)> = world
        .query::<(Entity, &AgentBox, &crate::closets::Closet)>()
        .iter(world)
        .map(|(e, b, c)| (e, b.fade_from, c.opened))
        .collect();
    for (closet, fade_from, opened) in boxes {
        if opened {
            world.despawn(closet);
            continue;
        }
        let alpha = box_alpha(now, fade_from);
        if let Some(mut look) = world.get_mut::<Appearance>(closet) {
            let [r, g, b, a] = look.color.unwrap_or([255; 4]);
            if a != alpha {
                look.color = Some([r, g, b, alpha]);
            }
        }
    }
}

/// 255 until `fade_from`, then down to 0 over [`FADE_SECS`].
pub fn box_alpha(now: f64, fade_from: f64) -> u8 {
    let t = ((now - fade_from) / FADE_SECS).clamp(0.0, 1.0);
    (255.0 * (1.0 - t)).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_box_fades_after_a_reveal() {
        assert_eq!(box_alpha(0.0, 0.0), 255);
        assert_eq!(box_alpha(1.0, 0.0), 128);
        assert_eq!(box_alpha(2.0, 0.0), 0);
        assert_eq!(box_alpha(9.0, 0.0), 0);
        // Revealed at 9 s: seen until 10 s, gone again at 12 s.
        assert_eq!(box_alpha(9.5, 10.0), 255);
        assert_eq!(box_alpha(12.0, 10.0), 0);
    }
}
