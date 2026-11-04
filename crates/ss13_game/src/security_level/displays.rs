//! Evac status displays showing a new security level (tg `/obj/machinery/status_display/evac`
//! `on_sec_level_change` -> `show_temporary_display`, then `revert_from_alert_display` after 30
//! s), with BandaStation's `status_display.dmi` icons (Gamma and Epsilon included).
// shortcut: displays show only their logo and the level picture: no shuttle timer, messages,
// pictures set from the communications console, Friend Computer or circuit USB; an unpowered
// display keeps its map look and does not show the level. Add with the status display panel.

use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmAtom, DmObj};
use ss13_shared::{Appearance, EMISSIVE_PLANE, Overlay};

use super::SecLevel;
use crate::power::Unpowered;

const EVAC_DISPLAY: &str = "/obj/machinery/status_display/evac";
/// `show_temporary_display`'s `alert_display_timer`.
pub const ALERT_SECS: f64 = 30.0;
/// The picture an evac display goes back to (`get_highest_priority_display`: the logo).
const LOGO: &str = "default";

/// An evac status display.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct EvacDisplay;

/// Until when the evac displays show the level's picture (every display's `alert_display_timer`
/// starts with the same level change).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct AlertDisplay {
    pub revert_at: Option<f64>,
}

pub(super) struct DisplaysPlugin;

impl Plugin for DisplaysPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AlertDisplay>()
            .add_systems(Startup, setup_displays)
            .add_systems(Update, revert_displays);
    }
}

fn setup_displays(world: &mut World) {
    let mut query = world.query::<(Entity, &DmObj)>();
    let displays: Vec<Entity> = query
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, EVAC_DISPLAY))
        .map(|(e, _)| e)
        .collect();
    for e in displays {
        world.entity_mut(e).insert(EvacDisplay);
    }
}

/// `set_picture(state)`: the picture and the lit outline.
fn set_picture(world: &mut World, e: Entity, state: &str) {
    let Some(icon) = world.get::<DmAtom>(e).map(|a| a.icon.clone()) else {
        return;
    };
    let Some(index) = world.get_resource::<IconIndex>() else {
        return;
    };
    let (Some(picture), Some(outline)) = (index.get(&icon, state), index.get(&icon, "outline"))
    else {
        warn!("status display {e}: no {state} or outline in {icon}");
        return;
    };
    let overlays = vec![
        Overlay::new(picture),
        Overlay {
            plane: Some(EMISSIVE_PLANE),
            ..Overlay::new(outline)
        },
    ];
    if let Some(mut appearance) = world.get_mut::<Appearance>(e)
        && appearance.overlays != overlays
    {
        appearance.overlays = overlays;
    }
}

/// `on_sec_level_change`: every powered evac display shows the level for 30 s.
pub(super) fn on_security_level_changed(world: &mut World, level: SecLevel) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut query = world.query_filtered::<Entity, (With<EvacDisplay>, Without<Unpowered>)>();
    let displays: Vec<Entity> = query.iter(world).collect();
    for e in displays {
        set_picture(world, e, level.status_display_icon_state());
    }
    if let Some(mut alert) = world.get_resource_mut::<AlertDisplay>() {
        alert.revert_at = Some(now + ALERT_SECS);
    }
}

/// `revert_from_alert_display`.
fn revert_displays(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if !world
        .resource::<AlertDisplay>()
        .revert_at
        .is_some_and(|at| now >= at)
    {
        return;
    }
    *world.resource_mut::<AlertDisplay>() = AlertDisplay::default();
    let mut query = world.query_filtered::<Entity, (With<EvacDisplay>, Without<Unpowered>)>();
    let displays: Vec<Entity> = query.iter(world).collect();
    for e in displays {
        set_picture(world, e, LOGO);
    }
}
