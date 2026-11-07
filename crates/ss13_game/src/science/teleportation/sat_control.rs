//! The satellite control console (server): tg `/obj/machinery/computer/sat_control` (tgui
//! `SatelliteControl`). The satellites themselves are [`crate::station_goals`].
//!
//! - Every satellite gets the next number when it appears (`global_id`, from 0). The window
//!   lists them all with their mode ("NTPROBEV0.8", meteor shields "M-SHIELD") and, while the
//!   meteor shield is a station goal, the shield's coverage against the goal.
//! - "Toggle" switches a satellite on the console's level (or both on station levels) like
//!   its own button, without the 2 s search (switching on still needs space).
// shortcut: no emagged satellites ("doesn't seem to respond"): satellite emagging is not
// ported.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::DmObj;
use ss13_shared::TilePos;
use ss13_tgui::server::{ServerUi, UiActed, UiParams, UiReject};

use super::server::{is_obj, open, operational};
use super::{ACT_TOGGLE, SatelliteControlData, SatelliteControlUi, SatelliteEntry};
use crate::obj_damage::is_under;
use crate::station_goals::{
    COVERAGE_GOAL, METEOR_SHIELD, SATELLITE, STATION_SHIELD, Satellite, StationGoals,
};

pub const CONSOLE: &str = "/obj/machinery/computer/sat_control";

/// A satellite's `id`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SatelliteId(pub u32);

/// What the console's window shows.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct SatControlView(pub SatelliteControlData);

/// `id = global_id++` for every new satellite.
pub(super) fn assign_ids(
    added: Query<(Entity, &DmObj), Added<DmObj>>,
    mut next: Local<u32>,
    mut commands: Commands,
) {
    for (e, obj) in &added {
        if is_under(&obj.path, SATELLITE) {
            commands.entity(e).try_insert(SatelliteId(*next));
            *next += 1;
        }
    }
}

fn mode(path: &str) -> &'static str {
    if is_under(path, METEOR_SHIELD) {
        "M-SHIELD"
    } else {
        "NTPROBEV0.8"
    }
}

/// `ui_data`.
pub(super) fn view(world: &mut World, console: Entity) -> Option<SatControlView> {
    if !is_obj(world, console, CONSOLE) {
        return None;
    }
    let mut satellites: Vec<SatelliteEntry> = world
        .query::<(&DmObj, &SatelliteId, Option<&Satellite>)>()
        .iter(world)
        .filter(|(o, ..)| is_under(&o.path, SATELLITE))
        .map(|(o, id, s)| SatelliteEntry {
            id: id.0,
            active: s.is_some_and(|s| s.active),
            mode: mode(&o.path).to_owned(),
        })
        .collect();
    satellites.sort_by_key(|s| s.id);
    let goals = world.get_resource::<StationGoals>();
    let meteor_shield = goals.is_some_and(|g| g.has(STATION_SHIELD));
    let coverage = goals.map_or(0, |g| g.coverage) as u32;
    Some(SatControlView(SatelliteControlData {
        satellites,
        meteor_shield,
        meteor_shield_coverage: if meteor_shield { coverage } else { 0 },
        meteor_shield_coverage_max: if meteor_shield {
            COVERAGE_GOAL as u32
        } else {
            0
        },
    }))
}

/// `attack_hand`: the window. True when the click was the console's.
pub fn attack_hand(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if !is_obj(world, target, CONSOLE) {
        return false;
    }
    if operational(world, target)
        && let Some(v) = view(world, target)
    {
        open::<SatelliteControlUi, _>(world, client, mob, target, v);
    }
    true
}

impl ServerUi for SatelliteControlUi {
    type Source = &'static SatControlView;
    type UserParam = ();
    type Act = f64;

    fn static_data(_: &SatControlView) {}

    fn data(view: &SatControlView, _: Entity, _: &()) -> SatelliteControlData {
        view.0.clone()
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<f64, UiReject> {
        match action {
            ACT_TOGGLE => params.number("id"),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// `is_valid_z_level`: the same level, or two station levels.
fn same_level(world: &World, a: TilePos, b: TilePos) -> bool {
    if a.z == b.z {
        return true;
    }
    let levels = crate::events::controller::station_levels(world);
    levels.contains(&a.z) && levels.contains(&b.z)
}

/// `toggle(id)`.
pub fn toggle_id(world: &mut World, console: Entity, id: u32) {
    let Some(&here) = world.get::<TilePos>(console) else {
        return;
    };
    let found: Vec<(Entity, TilePos)> = world
        .query::<(Entity, &SatelliteId, &TilePos)>()
        .iter(world)
        .filter(|(_, s, _)| s.0 == id)
        .map(|(e, _, &p)| (e, p))
        .collect();
    for (sat, at) in found {
        if same_level(world, at, here) {
            crate::station_goals::toggle(world, sat, None);
        }
    }
}

pub(super) fn console_acts(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<SatelliteControlUi>>>,
) {
    let pending: Vec<(Entity, f64)> = acts
        .read(world.resource::<Messages<UiActed<SatelliteControlUi>>>())
        .map(|a| (a.source, a.act))
        .collect();
    for (console, id) in pending {
        if !is_obj(world, console, CONSOLE) || !operational(world, console) {
            continue;
        }
        if id < 0.0 || id.fract() != 0.0 || id > f64::from(u32::MAX) {
            continue;
        }
        toggle_id(world, console, id as u32);
    }
}
