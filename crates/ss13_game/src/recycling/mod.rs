//! Conveyor belts and switches (`code/modules/recycling/conveyor.dm`), the recycler
//! (`game/machinery/recycler.dm`) and plastic flaps (`game/objects/structures/plasticflaps.dm`).
//! Disposal constructs and pipe dispensers are [`crate::disposals::build`].

pub mod conveyor;
pub mod flaps;
pub mod recycler;

use bevy::prelude::*;
use ss13_world::WorldSystems;

use crate::click_handlers::{AddClickHandler, ClickPhase};
use crate::tools::{Actor, DoAfter, tool_of};

pub use conveyor::{Belts, Conveying, Conveyor, ConveyorSwitch};
pub use flaps::Flaps;
pub use recycler::Recycler;

/// Timed steps (`crate::construction::Job::Recycling`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecyclingJob {
    /// Crowbar on a belt.
    PryBelt,
    /// Welder on an anchored disposal construct.
    WeldConstruct,
    /// Welder on a placed disposal pipe.
    SlicePipe,
    /// Welder on a disposal bin, chute or outlet.
    SliceMachine,
    /// Screwdriver on flaps: the state it started from.
    FlapsScrew { anchored: bool },
    /// Wirecutters on unanchored flaps.
    FlapsCut,
    /// Wrench on a recycler: the state it started from.
    RecyclerUnfasten { anchored: bool },
    /// A cable coil mending a robotic limb's burns.
    CoilHeal { zone: crate::zone::BodyZone },
}

pub struct RecyclingServerPlugin;

impl Plugin for RecyclingServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_click_handler(ClickPhase::AttackSelf, 3285, |w, c| {
            conveyor::attack_self(w, c.client, c.target)
        });
        app.add_click_handler(ClickPhase::OtherHand, 1385, |w, c| {
            c.held
                .is_some_and(|h| conveyor::link_stack(w, c.client, h, c.target))
        });
        // tg `attack_hand` of a switch.
        app.add_click_handler(ClickPhase::EmptyHand, 3565, |w, c| {
            conveyor::switch_hand(w, c.mob, c.target, c.modifiers.right)
        });
        app.add_click_handler(ClickPhase::HeldOnTarget, 2785, |w, c| {
            c.held.is_some_and(|held| {
                let actor = Actor {
                    client: c.client,
                    mob: c.mob,
                    item: Some(held),
                };
                held_on_target(w, actor, c.target, c.pos, c.modifiers.right)
            })
        });
        app.init_resource::<Belts>()
            .init_resource::<conveyor::BeltClock>()
            .add_systems(
                Update,
                (
                    conveyor::init_new,
                    flaps::init_new,
                    recycler::init_new,
                    (conveyor::tick, recycler::tick)
                        .chain()
                        .after(WorldSystems::Movement),
                ),
            );
    }
}

/// Tools and items on belts, switches, flaps, recyclers and disposal parts; a belt stack or a
/// switch assembly on a floor.
fn held_on_target(
    world: &mut World,
    actor: Actor,
    target: Entity,
    pos: ss13_shared::TilePos,
    right: bool,
) -> bool {
    let held = actor.item.expect("checked by the caller");
    let kind = tool_of(world, held).map(|t| t.0);
    if let Some(kind) = kind
        && (conveyor::tool_act(world, actor, kind, target, right)
            || flaps::tool_act(world, actor, kind, target)
            || recycler::tool_act(world, actor, kind, target)
            || crate::disposals::build::item_interaction(world, actor, kind, target))
    {
        return true;
    }
    conveyor::item_interaction(world, actor, held, target, pos)
}

/// tg `/datum/element/simple_rotation` of the things here.
pub fn click_alt(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    crate::disposals::build::click_alt(world, client, mob, target)
        || recycler::rotate(world, target)
}

/// End of a [`RecyclingJob`] `do_after`.
pub fn complete(world: &mut World, _mob: Entity, action: &DoAfter, job: RecyclingJob) {
    match job {
        RecyclingJob::PryBelt => conveyor::complete_pry(world, action.client, action.target),
        RecyclingJob::WeldConstruct | RecyclingJob::SlicePipe | RecyclingJob::SliceMachine => {
            crate::disposals::build::complete_weld(world, action, job);
        }
        RecyclingJob::FlapsScrew { anchored } => {
            flaps::complete_screw(world, action.client, action.target, anchored);
        }
        RecyclingJob::FlapsCut => flaps::complete_cut(world, action.client, action.target),
        RecyclingJob::RecyclerUnfasten { anchored } => {
            recycler::complete_unfasten(world, action.target, anchored);
        }
        RecyclingJob::CoilHeal { zone } => {
            crate::power::cable_coil::complete_heal(world, action, zone);
        }
    }
}
