//! Plastic flaps (`/obj/structure/plasticflaps`): airtight while screwed down; a standing mob
//! does not get through (`CanAllowThrough`) unless it is tiny, a ventcrawler or lying down.
//! Screwdriver (10 s) unscrews, wirecutters (5 s) cut the loose flaps into five plastic.
// shortcut: no melting from fire (`check_melt`), no plastic sound, the mover check is the
// server's only (the client predicts the step and is corrected); no beds, mechs, cardboard
// boxes or mulebot riders special-cased; no examine text.

use bevy::prelude::*;
use ss13_atmos::{AtmosBlocker, AtmosPass};
use ss13_mapload::DmObj;
use ss13_mapload::engmisc::FLAPS_ROOT;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_world::{EnterPolicy, PassFlags};

use super::RecyclingJob;
use crate::construction::{Job, spawn_obj};
use crate::health::{BodyPosition, Health};
use crate::obj_damage::{Anchored, anchored};
use crate::tools::{Actor, ToolKind, tell, use_tool};

const SCREW_DS: f32 = 100.0;
const CUT_DS: f32 = 50.0;
const KITCHEN: &str = "/obj/structure/plasticflaps/kitchen";
const PLASTIC_FIVE: &str = "/obj/item/stack/sheet/plastic/five";
/// `PASSFLAPS`.
const PASSFLAPS: u32 = 1 << 9;

/// `/obj/structure/plasticflaps`: `require_resting`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Flaps {
    pub require_resting: bool,
}

/// Flaps get their state as they appear (map load or a later spawn).
pub fn init_new(mut commands: Commands, added: Query<(Entity, &DmObj), Added<DmObj>>) {
    for (e, obj) in &added {
        if is_under(&obj.path, FLAPS_ROOT) {
            commands.entity(e).insert((
                Flaps {
                    require_resting: !is_under(&obj.path, KITCHEN),
                },
                EnterPolicy(allows),
            ));
        }
    }
}

/// `CanAllowThrough`.
fn allows(world: &World, mover: Entity, flaps: Entity) -> bool {
    if !world.get::<Flaps>(flaps).is_some_and(|f| f.require_resting) {
        return true;
    }
    if world
        .get::<PassFlags>(mover)
        .is_some_and(|p| p.0 & PASSFLAPS != 0)
    {
        return true;
    }
    if world.get::<Health>(mover).is_some() {
        let standing = !world.get::<BodyPosition>(mover).is_some_and(|b| b.lying());
        let tiny = crate::mobs::pets::mob_size(world, mover) == 0;
        let crawler = crate::ventcrawl::ventcrawler(world, mover).is_some();
        return !standing || tiny || crawler;
    }
    true
}

pub(super) fn tool_act(world: &mut World, actor: Actor, kind: ToolKind, target: Entity) -> bool {
    if world.get::<Flaps>(target).is_none() {
        return false;
    }
    let screwed = anchored(world, target);
    let client = actor.client;
    match kind {
        ToolKind::Screwdriver => {
            let job = Job::Recycling(RecyclingJob::FlapsScrew { anchored: screwed });
            if use_tool(world, actor, target, SCREW_DS, 0, job) {
                let text = if screwed {
                    "Вы начинаете откручивать завесы от пола..."
                } else {
                    "Вы начинаете прикручивать завесы к полу..."
                };
                tell(world, client, Span::Notice, text);
            }
            true
        }
        ToolKind::Wirecutters if !screwed => {
            if use_tool(
                world,
                actor,
                target,
                CUT_DS,
                0,
                Job::Recycling(RecyclingJob::FlapsCut),
            ) {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "Вы начинаете резать завесы...",
                );
            }
            true
        }
        _ => false,
    }
}

/// `screwdriver_act` after its `do_after`: `check_anchored_state`, `update_atmos_behaviour`.
pub(super) fn complete_screw(world: &mut World, client: Entity, flaps: Entity, was: bool) {
    if world.get::<Flaps>(flaps).is_none() || anchored(world, flaps) != was {
        return;
    }
    let screwed = !was;
    world.entity_mut(flaps).insert(Anchored(screwed));
    if screwed {
        world.entity_mut(flaps).insert(AtmosBlocker(AtmosPass::No));
    } else {
        world.entity_mut(flaps).remove::<AtmosBlocker>();
    }
    let text = if screwed {
        "Вы прикручиваете завесы к полу."
    } else {
        "Вы откручиваете завесы от пола."
    };
    tell(world, client, Span::Notice, text);
}

/// `wirecutter_act` after its `do_after`.
pub(super) fn complete_cut(world: &mut World, client: Entity, flaps: Entity) {
    if world.get::<Flaps>(flaps).is_none() || anchored(world, flaps) {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(flaps) else {
        return;
    };
    world.despawn(flaps);
    spawn_obj(world, PLASTIC_FIVE, pos);
    tell(world, client, Span::Notice, "Вы разрезаете завесы.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standing_mobs_are_refused_but_things_and_lying_mobs_pass() {
        let mut app = App::new();
        app.add_plugins(ss13_world::SpatialIndexPlugin);
        let world = app.world_mut();
        let flaps = world
            .spawn(Flaps {
                require_resting: true,
            })
            .id();
        let kitchen = world
            .spawn(Flaps {
                require_resting: false,
            })
            .id();
        let thing = world.spawn_empty().id();
        let tagged = world.spawn(PassFlags(PASSFLAPS)).id();
        assert!(allows(world, thing, flaps));
        assert!(allows(world, tagged, flaps));
        assert!(allows(world, thing, kitchen));
    }
}
