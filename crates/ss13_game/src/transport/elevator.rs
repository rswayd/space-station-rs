//! Elevators (server): tg `move_to_zlevel` and `update_lift_doors` of
//! `/datum/transport_controller/linear` (`linear_controller.dm`), the panel
//! `/obj/machinery/elevator_control_panel` (`elevator/elev_panel.dm`, window
//! `ElevatorPanel.tsx`), the call button's `/obj/item/assembly/control/elevator`
//! (`elev_controller.dm`) and the doors it drives (lift blast doors `/obj/machinery/door/poddoor/
//! lift`, elevator windoors, any door with `elevator_mode` and a `transport_linked_id`).
//!
//! - A trip to a level locks the controls and closes the lift's doors; 1.1 s later it goes a
//!   level at a time: each level, if the lift can still go that way and its panel (powered) or
//!   button still stands, the hydraulics sound and the lift moves after `elevator_vertical_speed`
//!   (2 s, the public lift 3 s), then 0.1 s on to the next. At the end the doors of the levels
//!   it is on unlock, ping and open 0.7 s later, and the controls unlock.
//! - Elevator doors are locked (no hand opens them) while the lift is away; an unlocked
//!   elevator windoor opens to a bump without access (`crate::doors`).
//! - The panel lists the levels the lift can reach (`preset_destination_names`, else
//!   "Floor N" by tg's level numbers), moves the lift with its access, and at red alert or above
//!   opens every door of the lift for 3 minutes (or until reset). An emag overrides the safeties:
//!   violent landings, half the travel time, every door forced open for good.
//! - The call button (cooldown 2 s) refuses while the lift moves, opens the doors when it is
//!   already on the button's level, else calls it there and says how the trip ended.
// shortcut: no radial up/down on the platform itself (tramstation's public lifts have none), no
// floor indicators, no warnings on the level below, no multitool reset of the panel or button,
// no emag of the button, no elevator music, no `simple_move_wrapper`.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmName, DmTransport, MapLoadReport};
use ss13_shared::TilePos;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};
use ss13_world::Mapped;

use super::{CONTROLS_LOCKED, Step, TYPE_ELEVATOR, Transport, check_lift_move, move_transport, zs};
use crate::balloon::balloon_alert;
use crate::click::{ClickResolved, ResolveClicks};
use crate::doors::{Door, with_door};
use crate::inventory::{Inventory, id_allowed};
use crate::power::Unpowered;
use crate::security_level::{SecLevel, SecurityLevel};
use crate::sound::{PlaySound, playsound};
use crate::tools::now;
pub use crate::transport_ui::{
    EMERGENCY_DOOR, ElevatorPanelData, ElevatorPanelStatic, ElevatorPanelUi, MOVE_LIFT,
    RESET_DOORS, Z,
};

pub const PANEL: &str = "/obj/machinery/elevator_control_panel";
const LIFT_DOOR: &str = "/obj/machinery/door/poddoor/lift";
/// `sleep(1.1 SECONDS)` after the doors close, `stoplag(travel_speed + 0.1 SECONDS)`.
const CLOSE_SECS: f64 = 1.1;
const STEP_GAP_SECS: f64 = 0.1;
/// `addtimer(open, 0.7 SECONDS)` after the ping.
const DOOR_OPEN_SECS: f64 = 0.7;
/// `addtimer(reset_doors, 3 MINUTES)` of the emergency door button.
const EMERGENCY_SECS: f64 = 180.0;
const HYDRAULIC: &str = "sound/vehicles/mecha/hydraulic.ogg";
const PING: &str = "sound/machines/ping.ogg";
const BUZZ: &str = "sound/machines/buzz/buzz-two.ogg";
const BUZZ_SIGH: &str = "sound/machines/buzz/buzz-sigh.ogg";

/// `/obj/machinery/elevator_control_panel`.
#[derive(Component, Clone, Debug, Default)]
pub struct ElevatorPanel {
    /// `linked_elevator_id`.
    pub linked: String,
    pub lift: Option<Entity>,
    /// `linked_elevator_destination`: `(name, z)`, top floor first.
    pub floors: Vec<(String, u16)>,
    pub req_access: Vec<String>,
    pub emagged: bool,
    /// `last_move_target`.
    pub last_target: Option<u16>,
    /// `door_reset_timerid`: when the emergency-opened doors go back.
    pub doors_reset_at: Option<f64>,
}

/// A lift on its way (`move_to_zlevel`), on the [`Transport`].
#[derive(Component, Clone, Debug)]
pub struct LiftTrip {
    pub target_z: u16,
    pub up: bool,
    pub steps_left: u32,
    /// The next level's checks.
    pub next_at: f64,
    /// The pending move of this level.
    pub move_at: Option<f64>,
    /// The panel or button whose loss stops the trip (`loop_callback`).
    pub check: Option<Entity>,
    /// The mob that called it with a button, and the button (its arrival messages).
    pub caller: Option<(Entity, Entity)>,
}

/// The elevator doors (`GLOB.elevator_doors`) with their `transport_linked_id`, and the
/// openings waiting for their 0.7 s.
#[derive(Resource, Default, Debug)]
pub struct ElevatorDoors {
    pub doors: Vec<(Entity, String)>,
    pub opening: Vec<(Entity, f64)>,
}

pub struct ElevatorPlugin;

impl Plugin for ElevatorPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // An emag on the panel (`emag_act`).
        app.add_click_handler(ClickPhase::HeldOnTarget, 355, |w, c| {
            !c.modifiers.right && c.held.is_some_and(|h| emag_panel(w, c.mob, h, c.target))
        });
        app.init_resource::<ElevatorDoors>()
            .register_ui::<ElevatorPanelUi>()
            .add_systems(
                ss13_world::MapInitLate,
                link_elevators.after(super::link_transports),
            )
            .add_systems(
                Update,
                (
                    panel_clicks.after(ResolveClicks).before(UiSystems::Open),
                    panel_acts.in_set(UiSystems::Handle),
                    tick_lifts,
                ),
            );
    }
}

/// The panels' links and floors, the elevator doors (`GLOB.elevator_doors`, locked).
pub fn link_elevators(world: &mut World) {
    let mut q = world.query_filtered::<(Entity, &DmTransport), With<Mapped>>();
    let found: Vec<(Entity, DmTransport)> = q.iter(world).map(|(e, t)| (e, t.clone())).collect();
    for (e, dm) in found {
        if is_under(&dm.path, PANEL) {
            let mut panel = ElevatorPanel {
                linked: dm.linked_elevator_id.clone(),
                req_access: dm.req_access.clone(),
                ..default()
            };
            panel.lift = super::find(world, TYPE_ELEVATOR, &dm.linked_elevator_id);
            match panel.lift {
                Some(lift) => panel.floors = floors(world, lift, &dm.preset_destination_names),
                None => warn!(
                    "elevator panel {e} found no lift {:?}",
                    dm.linked_elevator_id
                ),
            }
            world.entity_mut(e).insert(panel);
            continue;
        }
        let elevator_mode = dm.elevator_mode || is_under(&dm.path, LIFT_DOOR);
        if elevator_mode && !dm.transport_linked_id.is_empty() && world.get::<Door>(e).is_some() {
            if let Some(mut door) = world.get_mut::<Door>(e) {
                door.elevator_locked = Some(true);
            }
            world
                .resource_mut::<ElevatorDoors>()
                .doors
                .push((e, dm.transport_linked_id.clone()));
        }
    }
}

/// tg's level number of world level `z`: the station's first level is 2 (CentCom is 1).
fn tg_z(world: &World, z: u16) -> u16 {
    world
        .get_resource::<MapLoadReport>()
        .and_then(|r| {
            r.maps
                .iter()
                .find(|m| z >= m.first_z && u32::from(z) < u32::from(m.first_z) + m.size.2)
        })
        .map_or(z, |m| z - m.first_z + 2)
}

/// `populate_destinations_list`: the levels the lift is on and every level it can reach up and
/// down (`add_destinations_in_a_direction_recursively`), named, top first.
pub fn floors(world: &World, lift: Entity, presets: &[(String, String)]) -> Vec<(String, u16)> {
    let Some(transport) = world.get::<Transport>(lift) else {
        return Vec::new();
    };
    let levels = world.resource::<crate::multiz::ZLevels>();
    let start = super::tiles(world, transport);
    let mut found: Vec<u16> = start.iter().map(|p| p.z).collect();
    for up in [false, true] {
        let mut current = start.clone();
        'levels: loop {
            let mut next = Vec::with_capacity(current.len());
            for &p in &current {
                if !up && !crate::multiz::is_openspace(world, p) {
                    break 'levels;
                }
                let Some(n) = levels.step(p, up) else {
                    break 'levels;
                };
                if up && !crate::multiz::is_openspace(world, n) {
                    break 'levels;
                }
                next.push(n);
            }
            if next.is_empty() {
                break;
            }
            found.extend(next.iter().map(|p| p.z));
            current = next;
        }
    }
    found.sort_unstable();
    found.dedup();
    found
        .into_iter()
        .rev()
        .map(|z| {
            let tz = tg_z(world, z);
            let name = presets
                .iter()
                .find(|(k, _)| k.parse::<u16>().ok() == Some(tz))
                .map_or_else(
                    || format!("Floor {}", i32::from(tz) - 1),
                    |(_, n)| n.clone(),
                );
            (name, z)
        })
        .collect()
}

/// `return_closest_platform_to_z`: the level of the module vertically closest to `z`.
pub fn closest_z(world: &World, transport: &Transport, z: u16) -> Option<u16> {
    super::tiles(world, transport)
        .into_iter()
        .map(|p| p.z)
        .min_by_key(|&mz| (i32::from(mz) - i32::from(z)).abs())
}

/// tg `update_lift_doors(on_z_level, action)` for the lift with `specific_id`.
pub fn update_lift_doors(world: &mut World, specific_id: &str, on: Option<&[u16]>, open: bool) {
    let doors: Vec<Entity> = world
        .resource::<ElevatorDoors>()
        .doors
        .iter()
        .filter(|(_, id)| id == specific_id)
        .map(|(e, _)| *e)
        .collect();
    let at = now(world) + DOOR_OPEN_SECS;
    let mut pinged = false;
    for door in doors {
        let Some(&pos) = world.get::<TilePos>(door) else {
            continue;
        };
        if on.is_some_and(|zs| !zs.contains(&pos.z)) {
            continue;
        }
        if let Some(mut d) = world.get_mut::<Door>(door) {
            d.elevator_locked = Some(!open);
        }
        if open {
            if !pinged {
                playsound(world, PlaySound::new(pos, PING, 50.0, true));
                pinged = true;
            }
            world
                .resource_mut::<ElevatorDoors>()
                .opening
                .push((door, at));
        } else {
            world
                .resource_mut::<ElevatorDoors>()
                .opening
                .retain(|(d, _)| *d != door);
            with_door(world, door, |d, appearance, ctx| {
                let moved = ctx.close(d, appearance, false);
                ((), moved)
            });
        }
    }
}

fn set_locked(world: &mut World, lift: Entity, locked: bool) {
    if let Some(mut t) = world.get_mut::<Transport>(lift) {
        if locked {
            t.status |= CONTROLS_LOCKED;
        } else {
            t.status &= !CONTROLS_LOCKED;
        }
    }
}

/// tg `move_to_zlevel(target_z, loop_callback)`: starts the trip. False when the lift is on
/// that level already or cannot go that way.
pub fn move_to_zlevel(
    world: &mut World,
    lift: Entity,
    target_z: u16,
    check: Option<Entity>,
    caller: Option<(Entity, Entity)>,
) -> bool {
    let Some(transport) = world.get::<Transport>(lift).cloned() else {
        return false;
    };
    let Some(lift_z) = closest_z(world, &transport, target_z) else {
        return false;
    };
    if lift_z == target_z {
        return false;
    }
    let up = lift_z < target_z;
    if !check_lift_move(world, &transport, up) {
        return false;
    }
    set_locked(world, lift, true);
    update_lift_doors(world, &transport.specific_id, None, false);
    let next_at = now(world) + CLOSE_SECS;
    world.entity_mut(lift).insert(LiftTrip {
        target_z,
        up,
        steps_left: u32::from(lift_z.abs_diff(target_z)),
        next_at,
        move_at: None,
        check,
        caller,
    });
    true
}

/// The trip's `loop_callback`: its panel still works, or its button still stands.
fn check_ok(world: &World, check: Option<Entity>) -> bool {
    check.is_none_or(|e| {
        world.get_entity(e).is_ok()
            && (world.get::<ElevatorPanel>(e).is_none() || world.get::<Unpowered>(e).is_none())
    })
}

/// The lifts' trips, the delayed door openings and the emergency door resets.
pub fn tick_lifts(world: &mut World) {
    let t = now(world);
    let mut q = world.query::<(Entity, &LiftTrip)>();
    let trips: Vec<(Entity, LiftTrip)> = q.iter(world).map(|(e, l)| (e, l.clone())).collect();
    for (lift, mut trip) in trips {
        if let Some(at) = trip.move_at {
            if t < at {
                continue;
            }
            move_transport(world, lift, if trip.up { Step::Up } else { Step::Down });
            trip.move_at = None;
            trip.steps_left = trip.steps_left.saturating_sub(1);
            trip.next_at = at + STEP_GAP_SECS;
        }
        if t < trip.next_at {
            world.entity_mut(lift).insert(trip);
            continue;
        }
        let Some(transport) = world.get::<Transport>(lift).cloned() else {
            continue;
        };
        if trip.steps_left == 0
            || !check_lift_move(world, &transport, trip.up)
            || !check_ok(world, trip.check)
        {
            finish(world, lift, &transport, &trip);
            continue;
        }
        // `move_after_delay`.
        if let Some(pos) = super::tiles(world, &transport).into_iter().max_by_key(|p| {
            if trip.up {
                i32::from(p.z)
            } else {
                -i32::from(p.z)
            }
        }) {
            playsound(world, PlaySound::new(pos, HYDRAULIC, 25.0, true));
        }
        trip.move_at = Some(t + transport.vertical_secs);
        world.entity_mut(lift).insert(trip);
    }

    let due: Vec<Entity> = {
        let mut doors = world.resource_mut::<ElevatorDoors>();
        let due = doors
            .opening
            .iter()
            .filter(|(_, at)| t >= *at)
            .map(|(e, _)| *e)
            .collect();
        doors.opening.retain(|(_, at)| t < *at);
        due
    };
    for door in due {
        with_door(world, door, |d, appearance, ctx| {
            let moved = ctx.open(d, appearance, false);
            ((), moved)
        });
    }

    let mut q = world.query::<(Entity, &ElevatorPanel)>();
    let resets: Vec<Entity> = q
        .iter(world)
        .filter(|(_, p)| p.doors_reset_at.is_some_and(|at| t >= at))
        .map(|(e, _)| e)
        .collect();
    for panel in resets {
        reset_doors(world, panel);
    }
}

fn finish(world: &mut World, lift: Entity, transport: &Transport, trip: &LiftTrip) {
    world.entity_mut(lift).remove::<LiftTrip>();
    let here = zs(world, transport);
    update_lift_doors(world, &transport.specific_id, Some(&here), true);
    set_locked(world, lift, false);
    let Some((mob, button)) = trip.caller else {
        return;
    };
    if world.get_entity(button).is_err() {
        return;
    }
    let arrived = closest_z(world, transport, trip.target_z) == Some(trip.target_z);
    if arrived {
        balloon_alert(world, mob, button, "elevator arrived");
    } else {
        balloon_alert(world, mob, button, "elevator out of service!");
        if let Some(&pos) = world.get::<TilePos>(button) {
            playsound(world, PlaySound::new(pos, BUZZ_SIGH, 50.0, true));
        }
    }
}

/// `/obj/item/assembly/control/elevator/activate` -> `call_elevator`, from the button `button`
/// with `id`.
pub fn call_elevator(world: &mut World, mob: Entity, button: Entity, id: &str) -> bool {
    let Some(&pos) = world.get::<TilePos>(button) else {
        return false;
    };
    let called = (|| {
        let Some(lift) = super::find(world, TYPE_ELEVATOR, id) else {
            balloon_alert(world, mob, button, "no elevator connected!");
            return false;
        };
        let transport = world.get::<Transport>(lift).cloned().expect("found");
        if transport.locked() {
            balloon_alert(world, mob, button, "elevator is moving!");
            return false;
        }
        if closest_z(world, &transport, pos.z) == Some(pos.z) {
            let here = zs(world, &transport);
            update_lift_doors(world, &transport.specific_id, Some(&here), true);
            balloon_alert(world, mob, button, "elevator is here!");
            return true;
        }
        balloon_alert(world, mob, button, "elevator called");
        if !move_to_zlevel(world, lift, pos.z, Some(button), Some((mob, button))) {
            balloon_alert(world, mob, button, "elevator out of service!");
            return false;
        }
        true
    })();
    if !called {
        playsound(world, PlaySound::new(pos, BUZZ, 50.0, true));
    }
    called
}

fn panel_clicks(
    mut clicks: MessageReader<ClickResolved>,
    panels: Query<&DmName, With<ElevatorPanel>>,
    inventories: Query<&Inventory>,
    mut open: MessageWriter<OpenUi>,
) {
    for click in clicks.read() {
        let Ok(name) = panels.get(click.target) else {
            continue;
        };
        if inventories
            .get(click.mob)
            .is_ok_and(|inv| inv.active_item().is_none())
        {
            open.write(OpenUi::new::<ElevatorPanelUi>(
                click.client,
                click.mob,
                click.target,
                crate::vending::plain_name(&name.name),
            ));
        }
    }
}

impl ServerUi for ElevatorPanelUi {
    type Source = &'static ElevatorPanel;
    type UserParam = (
        Query<'static, 'static, &'static Transport>,
        Query<'static, 'static, &'static TilePos>,
        Option<Res<'static, SecurityLevel>>,
    );
    type Act = PanelAct;

    fn static_data(panel: &ElevatorPanel) -> ElevatorPanelStatic {
        ElevatorPanelStatic {
            all_floor_data: panel.floors.clone(),
        }
    }

    fn data(
        panel: &ElevatorPanel,
        _: Entity,
        (transports, positions, level): &bevy::ecs::system::SystemParamItem<
            '_,
            '_,
            Self::UserParam,
        >,
    ) -> ElevatorPanelData {
        let level = level.as_deref().map_or(SecLevel::Green, |l| l.current);
        let mut data = ElevatorPanelData {
            emergency_level: ss13_shared::capitalize(level.name()),
            is_emergency: level >= SecLevel::Red,
            doors_open: panel.doors_reset_at.is_some(),
            ..default()
        };
        if let Some(lift) = panel.lift.and_then(|l| transports.get(l).ok()) {
            data.lift_exists = true;
            data.currently_moving = lift.locked();
            data.currently_moving_to_floor = panel.last_target;
            data.current_floor = lift
                .modules
                .first()
                .and_then(|&m| positions.get(m).ok())
                .map_or(0, |p| p.z);
        }
        data
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<PanelAct, UiReject> {
        match action {
            MOVE_LIFT => params
                .get(Z)
                .and_then(|z| z.parse::<u16>().ok())
                .map(PanelAct::MoveLift)
                .ok_or_else(|| UiReject::BadParams("z: missing".into())),
            EMERGENCY_DOOR => Ok(PanelAct::EmergencyDoor),
            RESET_DOORS => Ok(PanelAct::ResetDoors),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelAct {
    MoveLift(u16),
    EmergencyDoor,
    ResetDoors,
}

/// `/obj/machinery/elevator_control_panel/ui_act`, every check at the moment of the act.
fn panel_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<ElevatorPanelUi>>>) {
    let pending: Vec<(Entity, Entity, PanelAct)> = acts
        .read(world.resource::<Messages<UiActed<ElevatorPanelUi>>>())
        .map(|a| (a.actor, a.source, a.act))
        .collect();
    for (actor, panel_e, act) in pending {
        let Some(panel) = world.get::<ElevatorPanel>(panel_e).cloned() else {
            continue;
        };
        if !check_ok(world, Some(panel_e)) {
            continue;
        }
        match act {
            PanelAct::MoveLift(z) => {
                move_lift(world, actor, panel_e, &panel, z);
            }
            PanelAct::EmergencyDoor => {
                let Some(lift) = panel.lift else {
                    continue;
                };
                let red = world
                    .get_resource::<SecurityLevel>()
                    .is_some_and(|l| l.current >= SecLevel::Red);
                let Some(id) = world.get::<Transport>(lift).map(|t| t.specific_id.clone()) else {
                    continue;
                };
                if !red {
                    continue;
                }
                update_lift_doors(world, &id, None, true);
                let at = now(world) + EMERGENCY_SECS;
                if let Some(mut p) = world.get_mut::<ElevatorPanel>(panel_e) {
                    p.doors_reset_at = Some(at);
                }
            }
            PanelAct::ResetDoors => {
                if panel.doors_reset_at.is_some() {
                    reset_doors(world, panel_e);
                }
            }
        }
    }
}

/// The panel's `move_lift` act. True when the lift set off.
pub fn move_lift(
    world: &mut World,
    actor: Entity,
    panel_e: Entity,
    panel: &ElevatorPanel,
    z: u16,
) -> bool {
    if !id_allowed(world, actor, &panel.req_access, &[]) {
        balloon_alert(world, actor, panel_e, "в доступе отказано!");
        return false;
    }
    if !panel.floors.iter().any(|(_, f)| *f == z) {
        return false;
    }
    let Some(lift) = panel.lift else {
        return false;
    };
    if world.get::<Transport>(lift).is_none_or(Transport::locked) {
        return false;
    }
    let moved = move_to_zlevel(world, lift, z, Some(panel_e), None);
    if let Some(mut p) = world.get_mut::<ElevatorPanel>(panel_e) {
        p.last_target = Some(z);
    }
    moved
}

/// `reset_doors`: open on the levels the lift is on, closed elsewhere.
fn reset_doors(world: &mut World, panel_e: Entity) {
    let Some(panel) = world.get::<ElevatorPanel>(panel_e).cloned() else {
        return;
    };
    if let Some(mut p) = world.get_mut::<ElevatorPanel>(panel_e) {
        p.doors_reset_at = None;
    }
    let Some(transport) = panel.lift.and_then(|l| world.get::<Transport>(l).cloned()) else {
        return;
    };
    let here = zs(world, &transport);
    let away: Vec<u16> = panel
        .floors
        .iter()
        .map(|(_, z)| *z)
        .filter(|z| !here.contains(z))
        .collect();
    update_lift_doors(world, &transport.specific_id, Some(&here), true);
    update_lift_doors(world, &transport.specific_id, Some(&away), false);
}

/// `/obj/machinery/elevator_control_panel/emag_act`.
fn emag_panel(world: &mut World, mob: Entity, held: Entity, target: Entity) -> bool {
    let Some(panel) = world.get::<ElevatorPanel>(target).cloned() else {
        return false;
    };
    if !crate::emag::is_emag(world, held) || panel.emagged {
        return false;
    }
    if let Some(mut p) = world.get_mut::<ElevatorPanel>(target) {
        p.emagged = true;
    }
    let Some(lift) = panel.lift else {
        return true;
    };
    let base = world
        .get::<Transport>(lift)
        .and_then(|t| t.modules.first().copied())
        .and_then(|m| world.get::<DmTransport>(m))
        .map_or(2.0, |d| f64::from(d.elevator_vertical_speed) / 10.0);
    if let Some(mut t) = world.get_mut::<Transport>(lift) {
        t.violent_landing = true;
        t.vertical_secs = base * 0.5;
    }
    let doors: Vec<Entity> = world
        .resource::<ElevatorDoors>()
        .doors
        .iter()
        .filter(|(_, id)| *id == panel.linked)
        .map(|(e, _)| *e)
        .collect();
    for door in doors {
        if world.get::<Door>(door).is_some_and(|d| d.emagged) {
            continue;
        }
        with_door(world, door, |d, appearance, ctx| {
            d.elevator_locked = Some(false);
            let moved = ctx.open_bypass(d, appearance);
            d.emagged = true;
            ((), moved)
        });
    }
    if let Some(&pos) = world.get::<TilePos>(target) {
        playsound(world, PlaySound::new(pos, "sparks", 100.0, true));
    }
    balloon_alert(world, mob, target, "safeties overridden");
    true
}
