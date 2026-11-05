//! `emp_act` of machines and structures: doors, security cameras, R&D servers, evac status
//! displays, bar signs, telecomms, closets, thermomachines, portable pumps and scrubbers, the
//! chem dispenser, fire alarms, sparkers, flashers, mass drivers, cyborg recharging stations.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use super::{EmpAct, EmpRng, EmpSystems, Emped, emp_acts, emped_expired, path_of, set_emped};
use crate::atmos_machinery::devices::ThermoMachine;
use crate::atmos_machinery::portable::{PortablePump, PortableScrubber};
use crate::barsign::Barsign;
use crate::chem::Reagents;
use crate::chem_dispenser::{ChemDispenser, InsertedBeaker};
use crate::closets::Closet;
use crate::doors::Door;
use crate::mecha::core::ItemCell;
use crate::power::Unpowered;
use crate::radio::TelecommsOutage;
use crate::science::points::ResearchServer;
use crate::security_level::displays::{DisplayState, EvacDisplay};
use crate::silicon::cameras::{Camera, CameraNet, NetCamera};
use crate::silicon::cyborg::RechargeStation;
use crate::wallmount::controls::Flasher;
use crate::wallmount::fire_alarm::FireAlarm;
use crate::wallmount::machines::{MassDriver, Sparker};
use crate::wires::Electrified;

/// `/obj/machinery/camera/emp_act` `reset_time`.
const CAMERA_EMP_SECS: f64 = 90.0;
/// `/obj/machinery/rnd/server/emp_act`, `/obj/machinery/barsign/emp_act`: `fix_emp` after 60 s.
const SERVER_EMP_SECS: f64 = 60.0;
const BARSIGN_EMP_SECS: f64 = 60.0;
/// `/datum/barsign/hiddensigns/empbarsign`.
const EMP_SIGN: &str = "EMP'd";
/// `/obj/machinery/telecomms/emp_act`: `(300 SECONDS) / severity`, ±2 s.
const TELECOMMS_EMP_SECS: f64 = 300.0;
const TELECOMMS: &str = "/obj/machinery/telecomms";
/// Relays only serve their own level (tg `relay`), not the station's network.
const TELECOMMS_RELAY: &str = "/obj/machinery/telecomms/relay";
/// `ONE_ATMOSPHERE`, kPa.
const ONE_ATMOSPHERE: f64 = 101.325;

pub(super) struct EmpMachinesPlugin;

impl Plugin for EmpMachinesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                (
                    doors,
                    cameras,
                    servers,
                    displays,
                    barsigns,
                    telecomms,
                    closets,
                    atmos_machines,
                    dispensers,
                    wall_mounts,
                    stations,
                )
                    .in_set(EmpSystems::Effects),
                (restore_cameras, restore_servers, restore_barsigns).in_set(EmpSystems::Restore),
            ),
        );
    }
}

fn prob(world: &mut World, percent: f64) -> bool {
    world.resource_mut::<EmpRng>().prob(percent)
}

/// `/obj/machinery/door/emp_act`: an airlock or windoor opens at `20 / severity` percent;
/// `/obj/machinery/door/airlock/emp_act`: electrified for 30 s at `severity * 10 - 20` percent
/// (only through extra protection: never for a plain pulse).
fn doors(world: &mut World, mut cursor: Local<MessageCursor<EmpAct>>) {
    for ev in emp_acts(world, &mut cursor) {
        if !ev.hits_self() {
            continue;
        }
        let Some(kind) = world.get::<Door>(ev.target).map(|d| d.kind) else {
            continue;
        };
        let sev = f64::from(ev.severity);
        let opens = matches!(
            kind,
            ss13_mapload::DoorKind::Airlock | ss13_mapload::DoorKind::Windoor
        );
        if opens && prob(world, 20.0 / sev) {
            crate::doors::with_door(world, ev.target, |door, appearance, ctx| {
                let moved = ctx.open(door, appearance, false);
                (moved, moved)
            });
        }
        if kind == ss13_mapload::DoorKind::Airlock && prob(world, sev * 10.0 - 20.0) {
            let shocked = world.get::<Electrified>(ev.target).copied();
            let below = match shocked {
                None => true,
                Some(Electrified::Timed(secs)) => secs < 30.0,
                Some(Electrified::Permanent) => false,
            };
            if below {
                crate::wires::set_electrified(world, ev.target, Some(Electrified::Timed(30.0)));
            }
        }
    }
}

/// `/obj/machinery/camera/emp_act`: an enabled camera goes dark at `150 / severity` percent
/// (off the network, its viewers back to their bodies) for 90 s after the last pulse.
// shortcut: no "static" message to its viewers, `emp` icon state or camera alarm.
fn cameras(world: &mut World, mut cursor: Local<MessageCursor<EmpAct>>) {
    for ev in emp_acts(world, &mut cursor) {
        if !ev.hits_self() {
            continue;
        }
        let emped = world.get::<Emped>(ev.target).is_some();
        let Some(cam) = world.get::<Camera>(ev.target) else {
            continue;
        };
        if !cam.enabled && !emped {
            continue;
        }
        if !prob(world, 150.0 / f64::from(ev.severity)) {
            continue;
        }
        set_camera(world, ev.target, false);
        set_emped(world, ev.target, CAMERA_EMP_SECS);
    }
}

/// The camera's `can_use()` and its place on the network.
pub(crate) fn set_camera(world: &mut World, camera: Entity, enabled: bool) {
    let (Some(range), Some(&pos)) = (
        world.get::<Camera>(camera).map(|c| c.view_range),
        world.get::<TilePos>(camera),
    ) else {
        return;
    };
    if let Some(mut c) = world.get_mut::<Camera>(camera) {
        c.enabled = enabled;
    }
    world.resource_mut::<CameraNet>().set_camera(
        camera,
        NetCamera {
            pos,
            range,
            enabled,
        },
    );
}

fn restore_cameras(world: &mut World) {
    for camera in emped_expired::<With<Camera>>(world) {
        world.entity_mut(camera).remove::<Emped>();
        set_camera(world, camera, true);
    }
}

/// `/obj/machinery/rnd/server/emp_act`: `EMPED` for 60 s, no research meanwhile.
fn servers(world: &mut World, mut cursor: Local<MessageCursor<EmpAct>>) {
    for ev in emp_acts(world, &mut cursor) {
        if ev.hits_self() && world.get::<ResearchServer>(ev.target).is_some() {
            set_emped(world, ev.target, SERVER_EMP_SECS);
        }
    }
}

fn restore_servers(world: &mut World) {
    for server in emped_expired::<With<ResearchServer>>(world) {
        world.entity_mut(server).remove::<Emped>();
    }
}

/// `/obj/machinery/status_display/emp_act`: a powered display shows `ai_bsod`.
fn displays(world: &mut World, mut cursor: Local<MessageCursor<EmpAct>>) {
    for ev in emp_acts(world, &mut cursor) {
        if ev.hits_self()
            && world.get::<EvacDisplay>(ev.target).is_some()
            && world.get::<Unpowered>(ev.target).is_none()
        {
            crate::security_level::displays::show(
                world,
                ev.target,
                DisplayState::Picture("ai_bsod".to_owned()),
            );
        }
    }
}

/// The sign a bar sign goes back to after its EMP (`fix_emp(chosen_sign)`).
#[derive(Component, Clone, Debug)]
pub struct SignBeforeEmp(pub String);

/// `/obj/machinery/barsign/emp_act`: the "EMP'd" sign for 60 s, then the one it had.
fn barsigns(world: &mut World, mut cursor: Local<MessageCursor<EmpAct>>) {
    for ev in emp_acts(world, &mut cursor) {
        if !ev.hits_self() {
            continue;
        }
        let Some(sign) = world.get::<Barsign>(ev.target).map(|b| b.sign.clone()) else {
            continue;
        };
        if world.get::<SignBeforeEmp>(ev.target).is_none() {
            world.entity_mut(ev.target).insert(SignBeforeEmp(sign));
        }
        set_emped(world, ev.target, BARSIGN_EMP_SECS);
        crate::barsign::set_sign(world, ev.target, EMP_SIGN);
    }
}

fn restore_barsigns(world: &mut World) {
    for sign in emped_expired::<With<Barsign>>(world) {
        world.entity_mut(sign).remove::<Emped>();
        if let Some(before) = world.get::<SignBeforeEmp>(sign).map(|s| s.0.clone()) {
            world.entity_mut(sign).remove::<SignBeforeEmp>();
            crate::barsign::set_sign(world, sign, &before);
        }
    }
}

/// `/obj/machinery/telecomms/emp_act` of the station's network: at `100 / severity` percent,
/// unless already `EMPED`, the telecomms are down for `300 s / severity` ±2 s
/// ([`TelecommsOutage`] stands in for the machines).
pub fn telecomms_emp(world: &mut World, severity: u8) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if world.resource::<TelecommsOutage>().until > now {
        return;
    }
    let mut rng = world.resource_mut::<EmpRng>();
    if !rng.prob(100.0 / f64::from(severity.max(1))) {
        return;
    }
    let base = (TELECOMMS_EMP_SECS / f64::from(severity.max(1))).round() as i64;
    let secs = rng.range(base - 2, base + 2);
    world.resource_mut::<TelecommsOutage>().until = now + secs as f64;
}

fn telecomms(world: &mut World, mut cursor: Local<MessageCursor<EmpAct>>) {
    for ev in emp_acts(world, &mut cursor) {
        if !ev.hits_self() {
            continue;
        }
        let is_telecomms = path_of(world, ev.target)
            .is_some_and(|p| is_under(&p, TELECOMMS) && !is_under(&p, TELECOMMS_RELAY));
        if is_telecomms {
            telecomms_emp(world, ev.severity);
        }
    }
}

/// `/obj/structure/closet/emp_act` of a secure closet whose lock is not broken: the lock flips
/// at `50 / severity` percent; at `20 / severity` a closed one opens if unlocked, else its lock
/// takes one random station access.
fn closets(world: &mut World, mut cursor: Local<MessageCursor<EmpAct>>) {
    for ev in emp_acts(world, &mut cursor) {
        let working_lock = world
            .get::<Closet>(ev.target)
            .is_some_and(|c| c.secure && !c.broken);
        if !ev.hits_self() || !working_lock {
            continue;
        }
        let sev = f64::from(ev.severity);
        if prob(world, 50.0 / sev) {
            if let Some(mut c) = world.get_mut::<Closet>(ev.target) {
                c.locked = !c.locked;
            }
            crate::closets::refresh_look(world, ev.target);
        }
        let Some((opened, locked)) = world.get::<Closet>(ev.target).map(|c| (c.opened, c.locked))
        else {
            continue;
        };
        if opened || !prob(world, 20.0 / sev) {
            continue;
        }
        if !locked {
            crate::closets::open(world, ev.target);
            continue;
        }
        let access = crate::id_access::all_station_access();
        let pick = world
            .resource_mut::<EmpRng>()
            .range(0, access.len() as i64 - 1) as usize;
        if let (Some(&one), Some(mut c)) = (access.get(pick), world.get_mut::<Closet>(ev.target)) {
            c.req_access = vec![one.to_owned()];
            c.req_one_access.clear();
        }
    }
}

/// Thermomachines (`75 / severity` percent each: power flips, a random target temperature),
/// portable pumps (`50 / severity` power flips, `100 / severity` pumping out, a random target
/// pressure) and scrubbers (`50 / severity` power flips).
fn atmos_machines(world: &mut World, mut cursor: Local<MessageCursor<EmpAct>>) {
    for ev in emp_acts(world, &mut cursor) {
        if !ev.hits_self() {
            continue;
        }
        let sev = f64::from(ev.severity);
        if let Some(m) = world.get::<ThermoMachine>(ev.target).cloned() {
            let flip = prob(world, 75.0 / sev);
            let retarget = prob(world, 75.0 / sev);
            let target = world
                .resource_mut::<EmpRng>()
                .range(m.min_temperature as i64, m.max_temperature as i64)
                as f64;
            let mut m = world.get_mut::<ThermoMachine>(ev.target).expect("checked");
            if flip {
                m.on = !m.on;
            }
            if retarget {
                m.target_temperature = target;
            }
            continue;
        }
        if world.get::<PortablePump>(ev.target).is_some() {
            let flip = prob(world, 50.0 / sev);
            let out = prob(world, 100.0 / sev);
            let pressure = world
                .resource_mut::<EmpRng>()
                .range(0, (100.0 * ONE_ATMOSPHERE) as i64) as f64;
            let mut p = world.get_mut::<PortablePump>(ev.target).expect("checked");
            if flip {
                p.on = !p.on;
            }
            if out {
                p.pump_in = false;
            }
            p.target_pressure = pressure;
            continue;
        }
        if world.get::<PortableScrubber>(ev.target).is_some() && prob(world, 50.0 / sev) {
            let mut s = world
                .get_mut::<PortableScrubber>(ev.target)
                .expect("checked");
            s.on = !s.on;
        }
    }
}

/// `/obj/machinery/chem_dispenser/emp_act`: `rand(7, 15)` random dispensable reagents (as many
/// as the cell pays for), the beaker emptied, the cost and the cell's own drain.
// shortcut: no `chem_splash`: the sprayed reagents vanish instead of covering the tiles around.
fn dispensers(world: &mut World, mut cursor: Local<MessageCursor<EmpAct>>) {
    for ev in emp_acts(world, &mut cursor) {
        if !ev.hits_self() {
            continue;
        }
        let Some(d) = world.get::<ChemDispenser>(ev.target).cloned() else {
            continue;
        };
        let charge = d
            .cell
            .and_then(|c| world.get::<ItemCell>(c))
            .map_or(0.0, |c| c.0.charge);
        let affordable = if d.power_cost > 0.0 {
            (charge / f64::from(d.power_cost)).floor() as i64
        } else {
            i64::MAX
        };
        let total = world.resource_mut::<EmpRng>().range(7, 15).min(affordable);
        if let Some(beaker) = world.get::<InsertedBeaker>(ev.target).and_then(|b| b.0)
            && let Some(mut r) = world.get_mut::<Reagents>(beaker)
        {
            r.0.clear();
        }
        if let Some(cell) = d.cell {
            if let Some(mut c) = world.get_mut::<ItemCell>(cell) {
                c.0.take(total.max(0) as f64 * f64::from(d.power_cost), false);
            }
            super::power::cell_emp_act(world, cell, ev.severity);
        }
        if let Some(&pos) = world.get::<TilePos>(ev.target) {
            let name = crate::security::name_of(world, ev.target);
            crate::security::show(
                world,
                pos,
                crate::health::DEFAULT_MESSAGE_RANGE,
                ss13_shared::chat::ChatChannel::Local,
                (
                    Span::Danger,
                    format!("{name} даёт сбой и разбрызгивает химикаты во все стороны!"),
                ),
                &[],
            );
        }
    }
}

/// Fire alarms ring at `50 / severity` percent; powered sparkers spark; powered flashers flash
/// and burn their bulb out at `75 / severity` percent; a powered mass driver with its panel
/// shut drives.
fn wall_mounts(world: &mut World, mut cursor: Local<MessageCursor<EmpAct>>) {
    for ev in emp_acts(world, &mut cursor) {
        if !ev.hits_self() {
            continue;
        }
        let e = ev.target;
        let sev = f64::from(ev.severity);
        let powered = world.get::<Unpowered>(e).is_none();
        if world.get::<FireAlarm>(e).is_some() {
            if prob(world, 50.0 / sev) {
                crate::wallmount::fire_alarm::alarm(world, e, None);
            }
        } else if world.get::<Sparker>(e).is_some() {
            if powered {
                crate::wallmount::machines::ignite(world, e);
            }
        } else if world.get::<Flasher>(e).is_some_and(|f| !f.burnt_out) {
            if powered && prob(world, 75.0 / sev) {
                crate::wallmount::controls::flash(world, e);
                if let Some(mut f) = world.get_mut::<Flasher>(e) {
                    f.burnt_out = true;
                }
                crate::wallmount::refresh_look(world, e);
            }
        } else if world.get::<MassDriver>(e).is_some() {
            let panel_open = world
                .get::<crate::construction::PanelOpen>(e)
                .is_some_and(|p| p.0);
            if powered && !panel_open {
                crate::wallmount::machines::drive(world, e);
            }
        }
    }
}

/// `/obj/machinery/recharge_station/emp_act`: a powered station opens.
// The cyborg inside stands on the station's tile, so the pulse already reached it there.
fn stations(world: &mut World, mut cursor: Local<MessageCursor<EmpAct>>) {
    for ev in emp_acts(world, &mut cursor) {
        if !ev.hits_self()
            || world.get::<Unpowered>(ev.target).is_some()
            || !world
                .get::<RechargeStation>(ev.target)
                .is_some_and(|s| !s.open)
        {
            continue;
        }
        let mut e = world.entity_mut(ev.target);
        e.insert(RechargeStation {
            open: true,
            occupant: None,
        });
        e.remove::<ss13_world::Density>();
    }
}
