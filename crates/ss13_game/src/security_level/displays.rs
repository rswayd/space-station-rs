//! Evac status displays (tg `/obj/machinery/status_display/evac`), with BandaStation's
//! `status_display.dmi` icons (Gamma and Epsilon included):
//! - a new security level shows for 30 s (`on_sec_level_change` -> `show_temporary_display`,
//!   then `revert_from_alert_display`);
//! - the communications console's `post_status` ([`post_status`]): blank, a picture or two lines
//!   of text, remembered as the manual display the alert reverts to (`manual_display_state`);
//!   the logo clears it.
// shortcut: text is not drawn (no maptext): a message blanks the screen and shows on examine
// ("The display says:"); "Shuttle ETA / Off" is blank (no shuttle timer). No Friend Computer,
// emergency pictures from events or circuit USB; an unpowered display keeps its map look and
// does not change.

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

/// What an evac display shows (tg `current_mode` with its picture or lines).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum DisplayState {
    /// `SD_BLANK`, or `SD_EMERGENCY` without a timer.
    Blank,
    /// `SD_PICTURE`.
    Picture(String),
    /// `SD_MESSAGE`.
    Message(String, String),
    /// The logo (`SD_PICTURE` "default", `DISPLAY_PRIORITY_LOGO`).
    #[default]
    Logo,
}

/// An evac status display: what it shows and what was set manually.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct EvacDisplay {
    pub showing: DisplayState,
    /// `manual_display_state`: the console's choice, shown again after an alert.
    pub manual: Option<DisplayState>,
}

/// tg `post_status(command, data1, data2)` commands of the communications console.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StatusCommand {
    Blank,
    Shuttle,
    Message(String, String),
    /// `"alert"` with a picture state.
    Alert(String),
}

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
        world.entity_mut(e).insert(EvacDisplay::default());
    }
}

/// Draws `state`: a picture with the lit outline, or nothing.
fn show(world: &mut World, e: Entity, state: DisplayState) {
    match &state {
        DisplayState::Picture(picture) => set_picture(world, e, picture),
        DisplayState::Logo => set_picture(world, e, LOGO),
        DisplayState::Blank | DisplayState::Message(..) => {
            if let Some(mut appearance) = world.get_mut::<Appearance>(e)
                && !appearance.overlays.is_empty()
            {
                appearance.overlays.clear();
            }
        }
    }
    if let Some(mut display) = world.get_mut::<EvacDisplay>(e) {
        display.showing = state;
    }
}

/// `receive_signal` of every powered evac display: `force_override` sets, ending a temporary
/// alert display.
pub fn post_status(world: &mut World, command: StatusCommand) {
    let mut query = world.query_filtered::<Entity, (With<EvacDisplay>, Without<Unpowered>)>();
    let displays: Vec<Entity> = query.iter(world).collect();
    *world.resource_mut::<AlertDisplay>() = AlertDisplay::default();
    for e in displays {
        let Some(mut display) = world.get_mut::<EvacDisplay>(e) else {
            continue;
        };
        let state = match &command {
            StatusCommand::Blank => DisplayState::Blank,
            StatusCommand::Shuttle => DisplayState::Blank,
            StatusCommand::Message(a, b) => DisplayState::Message(a.clone(), b.clone()),
            StatusCommand::Alert(p) if p == LOGO => DisplayState::Logo,
            StatusCommand::Alert(p) => DisplayState::Picture(p.clone()),
        };
        // `set_display_with_priority`: the shuttle mode is not manual, the logo clears it.
        match &command {
            StatusCommand::Shuttle => {}
            StatusCommand::Alert(p) if p == LOGO => display.manual = None,
            _ => display.manual = Some(state.clone()),
        }
        show(world, e, state);
    }
}

/// `examine`: the lines of a message.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(EvacDisplay {
        showing: DisplayState::Message(a, b),
        ..
    }) = world.get::<EvacDisplay>(target)
    else {
        return;
    };
    lines.push((
        ss13_shared::chat::Span::Plain,
        "The display says:".to_owned(),
    ));
    for line in [a, b].into_iter().filter(|l| !l.is_empty()) {
        lines.push((ss13_shared::chat::Span::Plain, format!("\t{line}")));
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

/// `revert_from_alert_display`: back to the manual display, else the logo.
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
        let state = world
            .get::<EvacDisplay>(e)
            .and_then(|d| d.manual.clone())
            .unwrap_or_default();
        show(world, e, state);
    }
}
