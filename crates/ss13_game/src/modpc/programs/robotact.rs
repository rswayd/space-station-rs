//! RoboTact (`/datum/computer_file/program/robotact`), a cyborg's self-management program on
//! its built-in computer (`crate::modpc::silicon`):
//!
//! - the unit, its model and master AI, cell charge (a Power Alert button with an empty or
//!   missing cell), chassis integrity, sensors, upgrades, the wire diagnostics (AI connection,
//!   LawSync, camera, model and motor controllers, maintenance cover), the laws with State Laws
//!   and the announcement channel ([`crate::silicon::state_laws`]), the event journal
//!   ([`logevent`]) and the other cyborgs on the same AI (`evaluate_borg`);
//! - Unlock opens the cover lock (`toggle_cover`), Sensor Overlay toggles the sensors, Self
//!   Destruct (emagged or syndicate cyborgs, not while stunned or locked down) detonates the
//!   cyborg (`self_destruct`).
// shortcut: no photo buttons (no silicon camera or printer), toner, ion thrusters or lamp
// intensity (none of them exist here); the camera wire reads FAULT only when cut (cyborgs have
// no built-in camera).

use bevy::prelude::*;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, Span};

use crate::health::{Health, Stat};
use crate::modpc::station_ui::*;
use crate::silicon::Silicons;
use crate::silicon::chassis::Chassis;
use crate::silicon::cyborg::{Cyborg, SYNDICATE_MODELS};
use crate::silicon::laws::Laws;

const BUZZ_TWO: &str = "sound/machines/buzz/buzz-two.ogg";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RobotactAct {
    CoverUnlock,
    LawChannel,
    LawState,
    AlertPower,
    Sensors,
    SelfDestruct,
}

pub fn parse(action: &str) -> Option<RobotactAct> {
    Some(match action {
        ROBOTACT_COVER_UNLOCK => RobotactAct::CoverUnlock,
        ROBOTACT_LAW_CHANNEL => RobotactAct::LawChannel,
        ROBOTACT_LAW_STATE => RobotactAct::LawState,
        ROBOTACT_ALERT_POWER => RobotactAct::AlertPower,
        ROBOTACT_SENSORS => RobotactAct::Sensors,
        ROBOTACT_SELF_DESTRUCT => RobotactAct::SelfDestruct,
        _ => return None,
    })
}

/// tg `stat` codes of the window.
fn stat_code(stat: Option<&Stat>) -> u8 {
    match stat {
        Some(Stat::Conscious) => 0,
        Some(Stat::Dead) => 4,
        _ => 3,
    }
}

/// `(health + 100) / 2`: a cyborg's health runs from -100 to 100.
fn integrity(world: &World, borg: Entity) -> f32 {
    world
        .get::<Health>(borg)
        .map_or(0.0, |h| (h.health() + 100.0) / 2.0)
}

fn model_name(world: &World, cyborg: &Cyborg) -> String {
    cyborg
        .model
        .and_then(|i| world.resource::<Silicons>().0.models.get(i))
        .map_or_else(|| "Default".to_owned(), |m| m.name.clone())
}

/// A cyborg of a syndicate model (tg `istype(cyborg, /mob/living/silicon/robot/model/syndicate)`).
pub fn is_syndicate(world: &World, cyborg: &Cyborg) -> bool {
    cyborg
        .model
        .and_then(|i| world.resource::<Silicons>().0.models.get(i))
        .is_some_and(|m| SYNDICATE_MODELS.iter().any(|(p, _)| *p == m.path))
}

/// tg `evaluate_borg`: cyborgs on the same AI see each other; two AI-less ones only when both
/// have scrambled codes.
pub fn evaluate_borg(
    this_ai: Option<Entity>,
    this_scrambled: bool,
    other_ai: Option<Entity>,
    other_scrambled: bool,
) -> bool {
    if this_ai != other_ai {
        return false;
    }
    (this_scrambled && other_scrambled) || this_ai.is_some()
}

/// `ui_data` + `ui_static_data` of the program on `pc`; empty off a cyborg.
pub fn data(world: &World, pc: Entity) -> RobotactData {
    use crate::device_wires::{WIRE_CAMERA, WIRE_LAWSYNC, WIRE_LOCKDOWN, WIRE_RESET_MODEL};
    use crate::wires::WIRE_AI;
    let Some(borg) = crate::modpc::silicon::owner(world, pc) else {
        return RobotactData::default();
    };
    let (Some(cyborg), Some(chassis)) = (world.get::<Cyborg>(borg), world.get::<Chassis>(borg))
    else {
        return RobotactData::default();
    };
    let name = |e: Entity| crate::silicon::chassis::name_of(world, e);
    let cut = |wire: &str| {
        world
            .get::<crate::wires::Wires>(borg)
            .is_some_and(|w| w.is_cut(wire))
    };
    let master_ai_online = cyborg
        .connected_ai
        .is_some_and(|ai| world.get::<Stat>(ai) == Some(&Stat::Conscious));
    let mut cyborgs = Vec::new();
    if master_ai_online || (cyborg.connected_ai.is_none() && chassis.scrambledcodes) {
        let mut others = world.try_query::<(Entity, &Cyborg, &Chassis)>();
        if let Some(others) = others.as_mut() {
            for (other, other_borg, other_chassis) in others.iter(world) {
                if !evaluate_borg(
                    cyborg.connected_ai,
                    chassis.scrambledcodes,
                    other_borg.connected_ai,
                    other_chassis.scrambledcodes,
                ) {
                    continue;
                }
                let shell_discon = world
                    .get::<crate::silicon::ai_shell::AiShell>(other)
                    .is_some()
                    && world.get::<crate::server::PlayerMob>(other).is_none();
                cyborgs.push(RobotactPeer {
                    name: name(other),
                    integ: integrity(world, other).round() as i32,
                    locked_down: other_chassis.lockcharge,
                    status: stat_code(world.get::<Stat>(other)),
                    shell_discon,
                    charge: other_chassis.cell.map(|_| {
                        if other_borg.max_charge > 0.0 {
                            (100.0 * other_borg.charge / other_borg.max_charge).round() as u8
                        } else {
                            0
                        }
                    }),
                    module: model_name(world, other_borg),
                });
            }
        }
    }
    cyborgs.sort_by(|a, b| a.name.cmp(&b.name));
    let (charge, max_charge) = if chassis.cell.is_some() {
        (cyborg.charge, cyborg.max_charge)
    } else {
        (0.0, 1.0)
    };
    RobotactData {
        borg_name: name(borg),
        designation: model_name(world, cyborg),
        master_ai: cyborg.connected_ai.map(name),
        master_ai_online,
        charge,
        max_charge,
        integrity: integrity(world, borg),
        sensors: world.get::<crate::records::hud::SensorsOff>(borg).is_none(),
        self_destruct_able: chassis.emagged || is_syndicate(world, cyborg),
        cyborgs,
        cover_locked: chassis.locked,
        locomotion: if cut(WIRE_LOCKDOWN) {
            "FAULT"
        } else if chassis.lockcharge {
            "DISABLED"
        } else {
            "ENABLED"
        }
        .to_owned(),
        wire_module: if cut(WIRE_RESET_MODEL) {
            "FAULT"
        } else {
            "NOMINAL"
        }
        .to_owned(),
        wire_camera: if cut(WIRE_CAMERA) { "FAULT" } else { "NOMINAL" }.to_owned(),
        wire_ai: if cut(WIRE_AI) {
            "FAULT"
        } else if cyborg.connected_ai.is_some() {
            "CONNECTED"
        } else {
            "READY"
        }
        .to_owned(),
        wire_law: if cut(WIRE_LAWSYNC) {
            "FAULT"
        } else {
            "NOMINAL"
        }
        .to_owned(),
        laws: world
            .get::<Laws>(borg)
            .map(|l| l.law_list(true))
            .unwrap_or_default(),
        borg_log: world
            .get::<crate::modpc::silicon::BorgLog>(pc)
            .map(|l| l.0.clone())
            .unwrap_or_default(),
        upgrades: chassis.upgrades.iter().map(|&u| name(u)).collect(),
    }
}

/// `ui_act`: only the cyborg the computer is built into acts.
pub fn apply(
    world: &mut World,
    client: Entity,
    actor: Entity,
    pc: Entity,
    act: RobotactAct,
) -> Result<(), &'static str> {
    let Some(borg) = crate::modpc::silicon::owner(world, pc) else {
        return Err("not a silicon computer");
    };
    if actor != borg {
        return Err("not the computer's cyborg");
    }
    let Some(chassis) = world.get::<Chassis>(borg).cloned() else {
        return Err("not a cyborg");
    };
    let conscious = world.get::<Stat>(borg) == Some(&Stat::Conscious);
    match act {
        RobotactAct::CoverUnlock => {
            if chassis.locked {
                crate::silicon::chassis::toggle_cover(world, borg, None);
            }
        }
        RobotactAct::LawChannel => crate::silicon::state_laws::set_autosay(world, client, borg),
        RobotactAct::LawState => crate::silicon::state_laws::checklaws(world, client, borg),
        RobotactAct::AlertPower => {
            let empty =
                chassis.cell.is_none() || world.get::<Cyborg>(borg).is_none_or(|c| c.charge <= 0.0);
            if conscious && empty {
                alert_power(world, borg);
            }
        }
        RobotactAct::Sensors => crate::silicon::ai_hud::toggle_sensors(world, client, borg),
        RobotactAct::SelfDestruct => {
            // "No detonation while stunned or locked down".
            let stunned = crate::status_effects::incapacitation(world, borg).incapacitated;
            if !conscious || stunned || chassis.lockcharge {
                return Err("incapacitated");
            }
            let syndicate = world
                .get::<Cyborg>(borg)
                .is_some_and(|c| is_syndicate(world, c));
            if !chassis.emagged && !syndicate {
                return Err("not emagged");
            }
            self_destruct(world, borg, chassis.emagged);
        }
    }
    Ok(())
}

/// `alertPower`: the warning light, seen around; the buzz.
fn alert_power(world: &mut World, borg: Entity) {
    let Some(&pos) = world.get::<TilePos>(borg) else {
        return;
    };
    let name = crate::silicon::chassis::name_of(world, borg);
    let messages = crate::pull::visible_message(
        world,
        pos,
        ChatChannel::Local,
        (
            Span::Notice,
            format!("The power warning light on {name} flashes urgently."),
        ),
        &[(
            borg,
            Span::Plain,
            "You announce you are operating in low power mode.".to_owned(),
        )],
    );
    for message in messages {
        world.write_message(message);
    }
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, BUZZ_TWO, 50.0, false),
    );
}

/// tg `self_destruct`: the AI is told, an explosion (bigger when emagged), the cyborg gibbed.
pub fn self_destruct(world: &mut World, borg: Entity, emagged: bool) {
    let Some(pos) = crate::explosions::turf_of(world, borg) else {
        return;
    };
    let name = crate::silicon::chassis::name_of(world, borg);
    info!("CYBORG: {name} ({borg}) has self-destructed.");
    if let Some(ai) = world.get::<Cyborg>(borg).and_then(|c| c.connected_ai) {
        crate::silicon::chassis::tell_ai(
            world,
            ai,
            Span::Warning,
            format!("ALERT - Cyborg detonation detected: {name}"),
        );
    }
    let mut explosion = if emagged {
        crate::explosions::Explosion::new(1, 2, 4).flame(2)
    } else {
        crate::explosions::Explosion::new(-1, 0, 2)
    };
    explosion.cause = format!("{name} self-destruct");
    crate::explosions::explosion(world, pos, explosion);
    if world.get_entity(borg).is_ok() {
        crate::health::gib(world, borg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cyborgs_see_their_ai_mates_and_scrambled_ones_each_other() {
        let mut world = World::new();
        let (ai, other_ai) = (world.spawn_empty().id(), world.spawn_empty().id());
        assert!(evaluate_borg(Some(ai), false, Some(ai), false));
        assert!(!evaluate_borg(Some(ai), false, Some(other_ai), false));
        assert!(!evaluate_borg(None, false, None, false));
        assert!(evaluate_borg(None, true, None, true));
        assert!(!evaluate_borg(None, true, None, false));
        // A scrambled cyborg on an AI sees only that AI's cyborgs.
        assert!(!evaluate_borg(Some(ai), true, None, true));
    }

    #[test]
    fn acts_parse_by_tgui_name() {
        assert_eq!(parse("coverunlock"), Some(RobotactAct::CoverUnlock));
        assert_eq!(parse("selfDestruct"), Some(RobotactAct::SelfDestruct));
        assert_eq!(parse("printImage"), None);
    }
}
