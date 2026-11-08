//! tg `show_radial_menu` (`code/_onclick/hud/screen_objects/radial.dm`): a circular choice around
//! an atom. The server asks through a modal ([`PromptKind::Radial`]: the choice comes back as a
//! [`PromptAnswer::Item`] checked against the slices, as any `tgui_input_list`), this module is
//! the anchor's side: the menu closes when the anchor is gone or, with `require_near`, out of
//! reach (`/datum/radial_menu/wait`), and a second click on the same anchor closes it
//! (`uniqueid`). The caller puts its own component on the prompt, reads `PromptAnswered` and
//! re-checks the target at the moment of effect ([`can_still_use`]).
//!
//! Single-choice menus are not auto-picked here (`autopick_single_option`): such a caller has
//! nothing to ask.

use bevy::prelude::*;
use ss13_mapload::IconIndex;
use ss13_net::server::Interest;
use ss13_shared::IconStateId;
use ss13_tgui::interfaces::radial::RadialFrame;
use ss13_tgui::prompt::{Ask, Prompt, PromptKind, PromptSystems, RadialChoice, ask_world};
use ss13_tgui::server::UiSystems;

use crate::health::Stat;

/// The frame sprites and tg's stock choice icons.
const RADIAL_DMI: &str = "icons/hud/radial.dmi";

pub struct RadialMenu {
    pub client: Entity,
    pub actor: Entity,
    pub anchor: Entity,
    pub title: String,
    pub choices: Vec<RadialChoice>,
    /// tg `require_near`.
    pub require_near: bool,
}

/// On a radial prompt: what keeps it open.
#[derive(Component, Clone, Copy, Debug)]
pub struct RadialGuard {
    pub anchor: Entity,
    pub require_near: bool,
}

pub struct RadialServerPlugin;

impl Plugin for RadialServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            close_stale
                .in_set(UiSystems::Handle)
                .after(PromptSystems::Answer),
        );
    }
}

/// The icon of `state` in `dmi`, when the assets have it.
pub fn icon_of(world: &World, dmi: &str, state: &str) -> Option<IconStateId> {
    world
        .get_resource::<IconIndex>()?
        .0
        .get(&(dmi.to_owned(), state.to_owned()))
        .copied()
}

/// A state of `icons/hud/radial.dmi`.
pub fn radial_icon(world: &World, state: &str) -> Option<IconStateId> {
    icon_of(world, RADIAL_DMI, state)
}

/// tg `can_perform_action` for an answered radial: conscious, hands free, the target in reach
/// (carried things are in reach of their carrier).
pub fn can_still_use(world: &World, mob: Entity, target: Entity) -> bool {
    if world.get::<Stat>(mob) != Some(&Stat::Conscious) {
        return false;
    }
    let incap = crate::status_effects::incapacitation(world, mob);
    !incap.incapacitated && !incap.hands_blocked && near(world, mob, target)
}

fn near(world: &World, mob: Entity, target: Entity) -> bool {
    match crate::balloon::atom_on_turf(world, target) {
        Some(on_turf) if on_turf == mob => true,
        Some(on_turf) => crate::storage::in_reach(world, mob, on_turf),
        None => false,
    }
}

/// tg `show_radial_menu`: the prompt, or `None` when there was nothing to ask or this closed the
/// menu already open on the same anchor.
pub fn show(world: &mut World, menu: RadialMenu) -> Option<Entity> {
    if menu.choices.is_empty() {
        return None;
    }
    let mut open = world.query::<(Entity, &RadialGuard, &Prompt)>();
    let same: Option<Entity> = open
        .iter(world)
        .find(|(_, guard, prompt)| guard.anchor == menu.anchor && prompt.client == menu.client)
        .map(|(e, ..)| e);
    if let Some(prompt) = same {
        world.despawn(prompt);
        return None;
    }
    let anchor_visual = crate::balloon::atom_on_turf(world, menu.anchor).unwrap_or(menu.actor);
    let anchor = world
        .get_resource::<Interest>()
        .and_then(|i| i.net_id(menu.client, anchor_visual));
    let frame = RadialFrame {
        slice: radial_icon(world, "radial_slice"),
        center: radial_icon(world, "radial_center"),
        next: radial_icon(world, "radial_next"),
    };
    let prompt = ask_world(
        world,
        Ask::new(
            menu.client,
            menu.actor,
            menu.title,
            String::new(),
            PromptKind::Radial {
                items: menu.choices,
                anchor,
                frame,
            },
        ),
    );
    world.entity_mut(prompt).insert(RadialGuard {
        anchor: menu.anchor,
        require_near: menu.require_near,
    });
    Some(prompt)
}

/// tg `/datum/radial_menu/wait`: the menu goes away with its reason to exist.
/// Cost: per open radial (a handful), one reach check per tick.
fn close_stale(world: &mut World) {
    let mut open = world.query::<(Entity, &RadialGuard, &Prompt)>();
    let stale: Vec<Entity> = open
        .iter(world)
        .filter(|(_, guard, prompt)| {
            world.get_entity(guard.anchor).is_err()
                || (guard.require_near && !near(world, prompt.actor, guard.anchor))
        })
        .map(|(e, ..)| e)
        .collect();
    for prompt in stale {
        world.despawn(prompt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_shared::TilePos;
    use ss13_world::SpatialIndex;

    fn world_with(mob_at: TilePos, thing_at: TilePos) -> (World, Entity, Entity) {
        let mut world = World::new();
        world.init_resource::<SpatialIndex>();
        let mob = world.spawn((mob_at, Stat::Conscious)).id();
        let thing = world.spawn(thing_at).id();
        (world, mob, thing)
    }

    #[test]
    fn a_carried_anchor_is_near_its_carrier_a_distant_one_is_not() {
        let (mut world, mob, thing) = world_with(TilePos::new(1, 1, 1), TilePos::new(9, 9, 1));
        assert!(!near(&world, mob, thing));
        let held = world.spawn(crate::inventory::CarriedBy(mob)).id();
        assert!(near(&world, mob, held));
    }
}
