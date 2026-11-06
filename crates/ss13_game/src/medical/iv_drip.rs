//! The IV drip's `IVDrip` window (server): tg `/obj/machinery/iv_drip` `ui_interact`, `ui_data`
//! and `ui_act` (`code/game/machinery/iv_drip.dm`); the drip itself is [`super::blood`].
//!
//! An empty-hand left click opens it: the flow rate (0-5 u/s in 0.01 steps), the mode, the
//! container's fill and the attached mob. Its acts set the rate (`changeRate`), switch the mode
//! (`changeMode`), drop the container on the drip's tile (`eject`) and pull the needle out
//! (`detach`), each checked against the drip's state at the moment of effect.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::iv_drip::{IvDripData, IvDripUi};
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiKind, UiParams, UiReject, UiSystems, UiWindow,
};

use super::blood::{IvDrip, refresh_drip};
use crate::chem::{Chem, Reagents};

/// `IV_TRANSFER_RATE_STEP`, `MIN_IV_TRANSFER_RATE`, `MAX_IV_TRANSFER_RATE`.
pub const RATE_STEP: f32 = 0.01;
pub const MIN_RATE: f32 = 0.0;
pub const MAX_RATE: f32 = 5.0;

/// The window data of a drip someone looks at.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct IvView(IvDripData);

pub(super) fn build(app: &mut App) {
    app.register_ui::<IvDripWindow>().add_systems(
        Update,
        (
            acts.in_set(UiSystems::Handle),
            refresh_views
                .after(UiSystems::Handle)
                .before(UiSystems::Send),
        ),
    );
}

/// `set_transfer_rate`: clamped and rounded to the step.
pub fn transfer_rate(rate: f32) -> f32 {
    let rate = (rate.clamp(MIN_RATE, MAX_RATE) / RATE_STEP).round() * RATE_STEP;
    rate.clamp(MIN_RATE, MAX_RATE)
}

/// `ui_interact`. True when `drip` is an IV drip.
pub fn interact(world: &mut World, client: Entity, mob: Entity, drip: Entity) -> bool {
    if world.get::<IvDrip>(drip).is_none() {
        return false;
    }
    refresh_view(world, drip);
    let title = super::name_of(world, drip);
    world.write_message(OpenUi::new::<IvDripWindow>(client, mob, drip, title));
    true
}

/// `ui_data`.
fn compute_view(world: &World, drip: Entity) -> Option<IvDripData> {
    let state = world.get::<IvDrip>(drip)?;
    let holder = state
        .container
        .and_then(|c| world.get::<Reagents>(c))
        .map(|r| &r.0);
    let color = holder.and_then(|h| {
        world
            .get_resource::<Chem>()
            .and_then(|chem| ss13_reagents::mix_color(&chem.0, &h.reagents))
    });
    Some(IvDripData {
        has_internal_storage: false,
        has_container: state.container.is_some(),
        can_remove_container: true,
        mode: state.injecting,
        // Only mobs are attached here (tg: no drawing from an attached object).
        can_draw: true,
        transfer_rate: state.transfer_rate,
        transfer_step: RATE_STEP,
        min_transfer_rate: MIN_RATE,
        max_transfer_rate: MAX_RATE,
        has_object_attached: state.attached.is_some(),
        object_name: state
            .attached
            .map(|m| super::name_of(world, m))
            .unwrap_or_default(),
        container_reagent_color: color,
        container_current_volume: holder
            .map_or(0.0, |h| (h.total_volume / RATE_STEP).round() * RATE_STEP),
        container_max_volume: holder.map_or(0.0, |h| h.maximum_volume),
    })
}

fn refresh_view(world: &mut World, drip: Entity) {
    let Some(view) = compute_view(world, drip) else {
        return;
    };
    match world.get_mut::<IvView>(drip) {
        Some(mut v) => {
            if v.0 != view {
                v.0 = view;
            }
        }
        None => {
            world.entity_mut(drip).insert(IvView(view));
        }
    }
}

/// Drips with an open window get fresh data.
// Cost: per tick, one pass over the open windows; a drip's data is a dozen fields.
fn refresh_views(world: &mut World) {
    let mut q = world.query_filtered::<&UiWindow, With<UiKind<IvDripWindow>>>();
    let mut drips: Vec<Entity> = q.iter(world).map(|w| w.source).collect();
    drips.sort();
    drips.dedup();
    for drip in drips {
        refresh_view(world, drip);
    }
}

/// The server side of the `IVDrip` interface.
pub struct IvDripWindow;

impl UiInterface for IvDripWindow {
    const NAME: &'static str = IvDripUi::NAME;
    type Data = IvDripData;
    type StaticData = ();
}

#[derive(Clone, Debug, PartialEq)]
pub enum IvAct {
    ChangeMode,
    Eject,
    Detach,
    ChangeRate(f32),
}

impl ServerUi for IvDripWindow {
    type Source = &'static IvView;
    type UserParam = ();
    type Act = IvAct;

    fn static_data(_: &IvView) {}

    fn data(view: &IvView, _: Entity, _: &()) -> IvDripData {
        view.0.clone()
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<IvAct, UiReject> {
        Ok(match action {
            "changeMode" => IvAct::ChangeMode,
            "eject" => IvAct::Eject,
            "detach" => IvAct::Detach,
            "changeRate" => IvAct::ChangeRate(params.number("rate")? as f32),
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

fn acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<IvDripWindow>>>) {
    let pending: Vec<(Entity, Entity, IvAct)> = acts
        .read(world.resource::<Messages<UiActed<IvDripWindow>>>())
        .map(|a| (a.source, a.actor, a.act.clone()))
        .collect();
    for (drip, actor, act) in pending {
        apply(world, drip, actor, act);
        refresh_view(world, drip);
    }
}

/// One act on `drip` by `actor`, against the drip's current state.
pub fn apply(world: &mut World, drip: Entity, actor: Entity, act: IvAct) {
    let Some(mut state) = world.get::<IvDrip>(drip).cloned() else {
        return;
    };
    match act {
        // `toggle_mode`.
        IvAct::ChangeMode => state.injecting = !state.injecting,
        // `eject_beaker`.
        IvAct::Eject => {
            let Some(container) = state.container.take() else {
                return;
            };
            if let Some(&pos) = world.get::<TilePos>(drip) {
                crate::storage::take_out(world, container, pos);
            }
        }
        // `detach_iv`.
        IvAct::Detach => {
            let Some(mob) = state.attached.take() else {
                return;
            };
            let (who, what) = (super::name_of(world, mob), super::name_of(world, drip));
            let text = format!("{who} отключается от {what}.");
            super::visible(world, actor, &text, &text, Span::Notice);
        }
        IvAct::ChangeRate(rate) => state.transfer_rate = transfer_rate(rate),
    }
    world.entity_mut(drip).insert(state);
    refresh_drip(world, drip);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rate_is_clamped_and_rounded() {
        assert_eq!(transfer_rate(7.0), MAX_RATE);
        assert_eq!(transfer_rate(-1.0), MIN_RATE);
        assert!((transfer_rate(2.344) - 2.34).abs() < 1e-4);
    }

    #[test]
    fn acts_change_the_drip() {
        let mut world = World::new();
        let mob = world.spawn_empty().id();
        let drip = world
            .spawn(IvDrip {
                attached: Some(mob),
                ..IvDrip::default()
            })
            .id();
        apply(&mut world, drip, mob, IvAct::ChangeRate(1.5));
        apply(&mut world, drip, mob, IvAct::ChangeMode);
        let state = world.get::<IvDrip>(drip).unwrap();
        assert!((state.transfer_rate - 1.5).abs() < 1e-4);
        assert!(!state.injecting);
        apply(&mut world, drip, mob, IvAct::Detach);
        assert_eq!(world.get::<IvDrip>(drip).unwrap().attached, None);
    }
}
