//! The teleporter (server): `/obj/machinery/teleport/hub`, `/obj/machinery/teleport/station`
//! and `/obj/machinery/computer/teleporter` (tgui `Teleporter`).
//!
//! - The three link side by side: a hub's station and a console's station are the first
//!   station in a cardinal direction (`link_power_station`), a station's hub and console the
//!   first ones next to it (`link_console_and_hub`). Looked up on use, so built or moved
//!   machines link as in tg's `Initialize`.
//! - The console's regime is "Teleporter" (targets: the enabled tracking beacons, named by
//!   their area) or "Gate" (targets: the stations a multitool linked to this one, each with a
//!   hub; locking one in links it back). A target must be teleportable to the console's tile
//!   (`is_eligible`). Changing the regime or the target disengages the station and loses the
//!   calibration; a beacon switched off clears the target.
//! - "Calibrate" takes 5 s × (3 - accuracy) (`accuracy` = the hub's matter bin tiers); a hub
//!   with accuracy 3 or more counts as calibrated.
//! - An empty hand on the station toggles it (it needs a target and a working hub). A hub whose
//!   station is engaged and working sends whatever bumps into it onto the target's tile
//!   (bluespace channel), uses its active power; an uncalibrated hub turns a human into a fly
//!   (moth in Moth Week) with `30 - accuracy × 10` % and loses its calibration after every
//!   use. Opening the hub's panel disengages the station.
//! - A multitool saves a station with its panel open; on a closed one it links the saved
//!   station (up to `efficiency` = capacitor tiers - 1).
// shortcut: no beacon implants (`/obj/item/implant/beacon`, not ported) as targets, no renamed
// beacons (no beacon rename), no forced power of a locked-in remote station (`lock_in_station`
// sets its `NOPOWER` off; the port's power state belongs to the grid), no portal animation, no
// USB circuit component, no hub/station icon change on power loss (only on use and toggles).
// CentCom and away levels are not modelled, so `is_eligible` and `Bumped` skip that check.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::DmObj;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};
use ss13_tgui::server::{ServerUi, UiActed, UiParams, UiReject};
use ss13_world::SpatialIndex;

use super::server::{
    adjacent, area_name_at, can_reach, is_obj, open, operational, sort_by_key, unique_key,
    use_active_energy, visible,
};
use super::{
    ACT_CALIBRATE, ACT_REGIME, ACT_TARGET, REGIME_GATE, REGIME_TELEPORTER, TeleporterData,
    TeleporterUi,
};
use crate::construction::{PanelOpen, panel_open};
use crate::examine::Line;
use crate::obj_damage::is_under;
use crate::stock_parts::PartKind;
use crate::teleport::{Channel, Tele, check_teleport_valid, do_teleport};
use crate::tools::{now, tell};

pub const HUB: &str = "/obj/machinery/teleport/hub";
pub const STATION: &str = "/obj/machinery/teleport/station";
pub const CONSOLE: &str = "/obj/machinery/computer/teleporter";
/// `SFX_PORTAL_ENTER`, `SHORT_RANGE_SOUND_EXTRARANGE`.
const SFX_ENTER: &str = "portal_enter";
const SHORT_RANGE_EXTRARANGE: f32 = -7.0;
/// `addtimer(..., 5 SECONDS * (3 - accuracy))`.
const CALIBRATE_SECS_PER_STEP: f64 = 5.0;

/// A hub's `calibrated` (absent: not calibrated).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TeleHub {
    pub calibrated: bool,
}

/// A station's `engaged` and `linked_stations`.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct TeleStation {
    pub engaged: bool,
    pub linked: Vec<Entity>,
}

/// A console's `regime_set`, `target_ref` and `calibrating` (until when).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct TeleConsole {
    /// `REGIME_GATE`.
    pub gate: bool,
    pub target: Option<Entity>,
    pub calibrating_until: Option<f64>,
}

/// What the console's window shows.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct TeleporterView(pub TeleporterData);

/// An open target list of `console`: the entity behind each item.
#[derive(Component, Clone, Debug)]
struct TargetPrompt {
    console: Entity,
    gate: bool,
    options: Vec<Entity>,
}

pub fn hub_state(world: &World, hub: Entity) -> TeleHub {
    world.get::<TeleHub>(hub).copied().unwrap_or_default()
}

pub fn station_state(world: &World, station: Entity) -> TeleStation {
    world
        .get::<TeleStation>(station)
        .cloned()
        .unwrap_or_default()
}

pub fn console_state(world: &World, console: Entity) -> TeleConsole {
    world
        .get::<TeleConsole>(console)
        .copied()
        .unwrap_or_default()
}

/// The hub's or the console's `power_station`.
pub fn station_of(world: &World, e: Entity) -> Option<Entity> {
    adjacent(world, e, STATION)
}

/// The station's `teleporter_hub`.
pub fn hub_of(world: &World, station: Entity) -> Option<Entity> {
    adjacent(world, station, HUB)
}

/// The station's `teleporter_console`.
pub fn console_of(world: &World, station: Entity) -> Option<Entity> {
    adjacent(world, station, CONSOLE)
}

/// `RefreshParts` of the hub: `accuracy` = matter bin tiers.
pub fn accuracy(world: &World, hub: Entity) -> u32 {
    crate::stock_parts::ratings(world, hub).sum(PartKind::MatterBin)
}

/// `RefreshParts` of the station: `efficiency` = capacitor tiers - 1.
pub fn efficiency(world: &World, station: Entity) -> usize {
    (crate::stock_parts::ratings(world, station).sum(PartKind::Capacitor) as usize)
        .saturating_sub(1)
}

/// The console's target, cleared when it is gone (`target_ref.resolve()`).
pub fn resolve_target(world: &mut World, console: Entity) -> Option<Entity> {
    let mut c = console_state(world, console);
    let target = c.target?;
    if world.get_entity(target).is_ok() {
        return Some(target);
    }
    c.target = None;
    world.entity_mut(console).insert(c);
    None
}

/// `is_ready()` of a hub.
pub fn is_ready(world: &World, hub: Entity) -> bool {
    if panel_open(world, hub) || !operational(world, hub) {
        return false;
    }
    station_of(world, hub).is_some_and(|s| station_state(world, s).engaged && operational(world, s))
}

/// `update_icon_state` of a hub: `tele-o`, `tele1` (ready) or `tele0`.
fn update_hub(world: &mut World, hub: Entity) {
    let state = if panel_open(world, hub) {
        "tele-o"
    } else if is_ready(world, hub) {
        "tele1"
    } else {
        "tele0"
    };
    crate::obj_damage::set_icon_state(world, hub, state);
}

/// `update_icon_state` of a station: `controller-o`, `-p` (off), `-c` (calibrating).
fn update_station(world: &mut World, station: Entity) {
    let calibrating = console_of(world, station)
        .is_some_and(|c| console_state(world, c).calibrating_until.is_some());
    let state = if panel_open(world, station) {
        "controller-o"
    } else if !operational(world, station) {
        "controller-p"
    } else if calibrating {
        "controller-c"
    } else {
        "controller"
    };
    crate::obj_damage::set_icon_state(world, station, state);
}

/// `turn_off()`: the station disengaged, the hub's calibration lost.
fn turn_off(world: &mut World, console: Entity) {
    let Some(station) = station_of(world, console) else {
        return;
    };
    let mut s = station_state(world, station);
    s.engaged = false;
    world.entity_mut(station).insert(s);
    if let Some(hub) = hub_of(world, station) {
        world.entity_mut(hub).insert(TeleHub { calibrated: false });
        update_hub(world, hub);
    }
}

fn set_target(world: &mut World, console: Entity, target: Option<Entity>) {
    let mut c = console_state(world, console);
    c.target = target;
    world.entity_mut(console).insert(c);
}

/// A tracking beacon switched off (`COMSIG_BEACON_DISABLED`): the consoles locked on it turn
/// off and lose it.
pub fn beacon_disabled(world: &mut World, beacon: Entity) {
    let consoles: Vec<Entity> = world
        .query::<(Entity, &TeleConsole)>()
        .iter(world)
        .filter(|(_, c)| c.target == Some(beacon))
        .map(|(e, _)| e)
        .collect();
    for console in consoles {
        turn_off(world, console);
        set_target(world, console, None);
    }
}

/// `is_eligible(atom)`: on the map and teleportable to the console's tile.
fn is_eligible(world: &World, target: Entity, console: Entity) -> bool {
    let (Some(at), Some(&here)) = (
        crate::explosions::turf_of(world, target),
        world.get::<TilePos>(console),
    ) else {
        return false;
    };
    check_teleport_valid(world, at, here)
}

/// `get_targets()`: name and target, sorted by name (`sort_list`).
pub fn get_targets(world: &mut World, console: Entity) -> Vec<(String, Entity)> {
    let mut used = Vec::new();
    let mut targets = Vec::new();
    if !console_state(world, console).gate {
        for beacon in crate::devices::beacons::enabled_beacons(world) {
            if !is_eligible(world, beacon, console) {
                continue;
            }
            let Some(at) = crate::explosions::turf_of(world, beacon) else {
                continue;
            };
            let name = unique_key(area_name_at(world, at), &mut used);
            targets.push((name, beacon));
        }
    } else if let Some(station) = station_of(world, console) {
        for linked in station_state(world, station).linked {
            if world.get_entity(linked).is_err()
                || !is_obj(world, linked, STATION)
                || !is_eligible(world, linked, console)
                || hub_of(world, linked).is_none()
            {
                continue;
            }
            let Some(&at) = world.get::<TilePos>(linked) else {
                continue;
            };
            let name = unique_key(area_name_at(world, at), &mut used);
            targets.push((name, linked));
        }
    }
    sort_by_key(&mut targets);
    targets
}

/// `lock_in_station`: the target station links back to ours.
fn lock_in_station(world: &mut World, console: Entity, target_station: Entity) {
    let Some(ours) = station_of(world, console) else {
        return;
    };
    let mut s = station_state(world, target_station);
    if !s.linked.contains(&ours) {
        s.linked.push(ours);
    }
    world.entity_mut(target_station).insert(s);
}

/// `ui_data`.
pub(super) fn view(world: &mut World, console: Entity) -> Option<TeleporterView> {
    if !is_obj(world, console, CONSOLE) {
        return None;
    }
    let target = resolve_target(world, console);
    let c = console_state(world, console);
    let station = station_of(world, console);
    let hub = station.and_then(|s| hub_of(world, s));
    let target_text = match target {
        None => "None".to_owned(),
        Some(t) => {
            let area = crate::explosions::turf_of(world, t)
                .map(|p| area_name_at(world, p))
                .unwrap_or_default();
            // tg: `"[get_area(target)] [(regime_set != REGIME_GATE) ? "" : REGIME_TELEPORTER]"`.
            if c.gate {
                format!("{area} {REGIME_TELEPORTER}")
            } else {
                format!("{area} ")
            }
        }
    };
    let calibrated = hub.is_some_and(|h| hub_state(world, h).calibrated || accuracy(world, h) >= 3);
    Some(TeleporterView(TeleporterData {
        power_station: station.is_some(),
        teleporter_hub: hub.is_some(),
        regime_set: if c.gate {
            REGIME_GATE
        } else {
            REGIME_TELEPORTER
        }
        .to_owned(),
        target: target_text,
        calibrating: c.calibrating_until.is_some(),
        calibrated,
    }))
}

/// `attack_hand`: the console's window, the station's toggle. True when the click was theirs.
pub fn attack_hand(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if is_obj(world, target, CONSOLE) {
        if !operational(world, target) {
            return true;
        }
        if let Some(v) = view(world, target) {
            open::<TeleporterUi, _>(world, client, mob, target, v);
        }
        return true;
    }
    if is_obj(world, target, STATION) {
        toggle(world, client, target);
        return true;
    }
    false
}

/// The station's `toggle(user)`.
pub fn toggle(world: &mut World, client: Entity, station: Entity) {
    let (Some(hub), Some(console)) = (hub_of(world, station), console_of(world, station)) else {
        return;
    };
    if !operational(world, station) {
        return;
    }
    let mut s = station_state(world, station);
    if resolve_target(world, console).is_some() {
        if panel_open(world, hub) || !operational(world, hub) {
            tell(
                world,
                client,
                Span::Warning,
                "The teleporter hub isn't responding.",
            );
        } else {
            s.engaged = !s.engaged;
            world.entity_mut(station).insert(s.clone());
            use_active_energy(world, station);
            let text = if s.engaged {
                "Teleporter engaged!"
            } else {
                "Teleporter disengaged!"
            };
            tell(world, client, Span::Notice, text);
        }
    } else {
        set_target(world, console, None);
        tell(world, client, Span::Warning, "No target detected.");
        s.engaged = false;
        world.entity_mut(station).insert(s);
    }
    update_hub(world, hub);
}

/// `multitool_act` of a station. True when handled.
pub fn multitool_act(world: &mut World, client: Entity, tool: Entity, station: Entity) -> bool {
    if !is_obj(world, station, STATION) {
        return false;
    }
    if panel_open(world, station) {
        crate::tools::set_multitool_buffer(world, tool, station);
        tell(world, client, Span::Notice, "saved to multitool buffer");
        return true;
    }
    let Some(buffer) = crate::tools::multitool_buffer(world, tool)
        .filter(|&b| b != station && is_obj(world, b, STATION))
    else {
        return true;
    };
    let mut s = station_state(world, station);
    if s.linked.len() < efficiency(world, station) {
        s.linked.push(buffer);
        world.entity_mut(station).insert(s);
        world
            .entity_mut(tool)
            .remove::<crate::tools::MultitoolBuffer>();
        tell(world, client, Span::Notice, "data uploaded from buffer");
    }
    true
}

/// The hub's `Bumped`: a ready hub teleports the bumper. True when `obstacle` was a hub.
pub fn bumped(world: &mut World, mover: Entity, obstacle: Entity) -> bool {
    if !is_obj(world, obstacle, HUB) {
        return false;
    }
    if is_ready(world, obstacle) {
        teleport(world, obstacle, mover);
    }
    true
}

/// The hub's `teleport(M)`.
pub fn teleport(world: &mut World, hub: Entity, mover: Entity) {
    let Some(console) = station_of(world, hub).and_then(|s| console_of(world, s)) else {
        return;
    };
    let Some(&hub_pos) = world.get::<TilePos>(hub) else {
        return;
    };
    let Some(dest) =
        resolve_target(world, console).and_then(|t| crate::explosions::turf_of(world, t))
    else {
        set_target(world, console, None);
        visible(
            world,
            hub_pos,
            Span::Warning,
            "Cannot authenticate locked on coordinates. Please reinstate coordinate matrix.",
        );
        return;
    };
    let tele = Tele {
        channel: Channel::Bluespace,
        ..default()
    };
    if !do_teleport(world, mover, dest, tele) {
        return;
    }
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(hub_pos, SFX_ENTER, 50.0, true)
            .extrarange(SHORT_RANGE_EXTRARANGE),
    );
    use_active_energy(world, hub);
    let calibrated = hub_state(world, hub).calibrated;
    let acc = accuracy(world, hub) as i32;
    if !calibrated && world.get::<crate::species::Species>(mover).is_some() {
        let chance = 30 - acc * 10;
        let roll = world
            .resource_mut::<crate::combat::CombatRng>()
            .range(0, 99) as i32;
        if roll < chance {
            mutate(world, mover);
        }
    }
    world.entity_mut(hub).insert(TeleHub { calibrated: false });
}

/// "oh dear a problem": a fly (a moth in Moth Week), unless robotic, mineral, undead or a
/// spirit, or already one.
fn mutate(world: &mut World, mob: Entity) {
    use crate::species::SpeciesId;
    use crate::species::biotype::{MINERAL, ROBOTIC, SPIRIT, UNDEAD};
    if crate::species::def_of(world, mob).biotypes & (ROBOTIC | MINERAL | UNDEAD | SPIRIT) != 0 {
        return;
    }
    let id = if crate::events::holidays::check_holidays(world, crate::events::holidays::MOTH_WEEK) {
        SpeciesId::Moth
    } else {
        SpeciesId::Fly
    };
    if crate::species::species_of(world, mob) == id {
        return;
    }
    crate::tools::tell_mob(
        world,
        mob,
        Span::Italics,
        "You hear a buzzing in your ears.",
    );
    crate::species::set_species(world, mob, id);
}

/// Opening the hub's panel disengages its station (`on_set_panel_open`).
pub(super) fn hub_panels(
    hubs: Query<(Entity, &TilePos, &PanelOpen, &DmObj), Changed<PanelOpen>>,
    index: Res<SpatialIndex>,
    objs: Query<&DmObj>,
    mut stations: Query<&mut TeleStation>,
) {
    for (_, &pos, open, obj) in &hubs {
        if !open.0 || !is_under(&obj.path, HUB) {
            continue;
        }
        let station = ss13_shared::Dir::CARDINALS.iter().find_map(|&d| {
            index
                .on_tile(pos.step(d))
                .iter()
                .copied()
                .find(|&e| objs.get(e).is_ok_and(|o| is_under(&o.path, STATION)))
        });
        if let Some(mut s) = station.and_then(|s| stations.get_mut(s).ok()) {
            s.engaged = false;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TeleporterAct {
    Regime,
    Target,
    Calibrate,
}

impl ServerUi for TeleporterUi {
    type Source = &'static TeleporterView;
    type UserParam = ();
    type Act = TeleporterAct;

    fn static_data(_: &TeleporterView) {}

    fn data(view: &TeleporterView, _: Entity, _: &()) -> TeleporterData {
        view.0.clone()
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<TeleporterAct, UiReject> {
        match action {
            ACT_REGIME => Ok(TeleporterAct::Regime),
            ACT_TARGET => Ok(TeleporterAct::Target),
            ACT_CALIBRATE => Ok(TeleporterAct::Calibrate),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

fn say(world: &mut World, console: Entity, text: &str) {
    crate::atmos_machinery::machine_say(world, console, text);
}

/// `ui_act`.
pub(super) fn console_acts(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<TeleporterUi>>>,
) {
    let pending: Vec<(Entity, Entity, Entity, TeleporterAct)> = acts
        .read(world.resource::<Messages<UiActed<TeleporterUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act))
        .collect();
    for (client, actor, console, act) in pending {
        if !is_obj(world, console, CONSOLE) || !operational(world, console) {
            continue;
        }
        let Some(hub) = station_of(world, console).and_then(|s| hub_of(world, s)) else {
            say(world, console, "Error: Unable to detect hub.");
            continue;
        };
        if console_state(world, console).calibrating_until.is_some() {
            say(world, console, "Error: Calibration in progress. Stand by.");
            continue;
        }
        match act {
            TeleporterAct::Regime => {
                turn_off(world, console);
                let mut c = console_state(world, console);
                c.target = None;
                c.gate = !c.gate;
                world.entity_mut(console).insert(c);
            }
            TeleporterAct::Target => {
                turn_off(world, console);
                ask_target(world, client, actor, console);
            }
            TeleporterAct::Calibrate => {
                if resolve_target(world, console).is_none() {
                    say(world, console, "Error: No target set to calibrate to.");
                    continue;
                }
                let acc = accuracy(world, hub);
                if hub_state(world, hub).calibrated || acc >= 3 {
                    say(world, console, "Hub is already calibrated!");
                    continue;
                }
                say(world, console, "Processing hub calibration to target...");
                let mut c = console_state(world, console);
                c.calibrating_until =
                    Some(now(world) + CALIBRATE_SECS_PER_STEP * f64::from(3 - acc));
                world.entity_mut(console).insert(c);
                if let Some(station) = station_of(world, console) {
                    update_station(world, station);
                }
            }
        }
    }
}

/// `set_target(user)`: the list of targets.
fn ask_target(world: &mut World, client: Entity, actor: Entity, console: Entity) {
    let gate = console_state(world, console).gate;
    let targets = get_targets(world, console);
    if gate && targets.is_empty() {
        tell(
            world,
            client,
            Span::Warning,
            "No active connected stations located.",
        );
        return;
    }
    let message = if gate {
        "Select a station to lock in"
    } else {
        "Select a location to lock in"
    };
    let (items, options): (Vec<String>, Vec<Entity>) = targets.into_iter().unzip();
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            actor,
            "Locking Computer",
            message,
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(TargetPrompt {
        console,
        gate,
        options,
    });
}

/// The chosen target, the user still next to the console (`can_perform_action`).
pub(super) fn target_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.actor, a.answer.clone()))
        .collect();
    for (prompt, actor, answer) in answers {
        let Some(TargetPrompt {
            console,
            gate,
            options,
        }) = world.get::<TargetPrompt>(prompt).cloned()
        else {
            continue;
        };
        let Some(PromptAnswer::Item(index)) = answer else {
            continue;
        };
        let Some(&chosen) = options.get(index) else {
            continue;
        };
        if world.get_entity(console).is_err()
            || world.get_entity(chosen).is_err()
            || !can_reach(world, actor, console)
            || console_state(world, console).gate != gate
        {
            continue;
        }
        if !gate {
            set_target(world, console, Some(chosen));
            continue;
        }
        let Some(target_hub) = hub_of(world, chosen) else {
            continue;
        };
        set_target(world, console, Some(target_hub));
        lock_in_station(world, console, chosen);
    }
}

/// `finish_calibration`.
pub(super) fn finish_calibrations(world: &mut World) {
    let t = now(world);
    let due: Vec<Entity> = world
        .query::<(Entity, &TeleConsole)>()
        .iter(world)
        .filter(|(_, c)| c.calibrating_until.is_some_and(|at| at <= t))
        .map(|(e, _)| e)
        .collect();
    for console in due {
        let mut c = console_state(world, console);
        c.calibrating_until = None;
        world.entity_mut(console).insert(c);
        let station = station_of(world, console);
        match station.and_then(|s| hub_of(world, s)) {
            Some(hub) => {
                world.entity_mut(hub).insert(TeleHub { calibrated: true });
                say(world, console, "Calibration complete.");
            }
            None => say(world, console, "Error: Unable to detect hub."),
        }
        if let Some(station) = station {
            update_station(world, station);
        }
    }
}

/// The hub's success chance, the station's links and panel.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<Line>) {
    if is_obj(world, target, HUB) {
        let chance = 70 + accuracy(world, target) * 10;
        lines.push((
            Span::Notice,
            format!("The status display reads: Success chance is {chance}%."),
        ));
    } else if is_obj(world, target, STATION) {
        lines.push((
            Span::Notice,
            if panel_open(world, target) {
                "The linking device is now able to be scanned with a multitool."
            } else {
                "The panel is screwed in, obstructing the linking device and wiring panel."
            }
            .to_owned(),
        ));
        let n = efficiency(world, target);
        lines.push((
            Span::Notice,
            format!(
                "The status display reads: This station can be linked to {n} other station(s)."
            ),
        ));
    } else {
        super::launchpad::examine_lines(world, target, lines);
    }
}
