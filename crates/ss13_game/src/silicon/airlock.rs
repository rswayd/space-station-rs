//! The airlock's silicon control window (server), after tg `/obj/machinery/door/airlock`
//! `attack_ai`, `ui_data` and `ui_act` (`code/game/machinery/doors/airlock.dm`), tgui
//! `AiAirlock`.
//!
//! Opening needs nothing but a silicon click (`attack_ai`): without AI control (the AI wire
//! cut, all power cut) the AI is told the firewall blocks it and the window still opens, but
//! every act is refused (`user_allowed`: `HAS_SILICON_ACCESS && canAIControl`). Acts: disrupt
//! main/backup power, electrify (restore, 30 s, permanent) unless the shock wire is cut, the ID
//! scanner, emergency access, bolts (raising them needs power), feedback lights, force sensors
//! (`safe`), timing safety (`normalspeed`), open/close (not while bolted or welded).
//! An emagged airlock (`crate::emag`) does not answer: no window.
// shortcut: no AI hacking of a firewalled airlock (`canAIHack`/`hack`).

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::DmName;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::ai_airlock::{AiAirlockData, AiAirlockUi, AirlockPower, AirlockWires};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use super::ai::{can_ai_control, toggle_bolt, user_toggle_open, warn};
use super::interact::has_silicon_access;
use crate::doors::{Door, DoorState};
use crate::wires::{
    Electrified, MACHINE_DEFAULT_ELECTRIFY_TIME, WIRE_BACKUP1, WIRE_BACKUP2, WIRE_BOLTS,
    WIRE_FEEDBACK, WIRE_IDSCAN, WIRE_POWER1, WIRE_POWER2, WIRE_SAFETY, WIRE_SHOCK, WIRE_TIMING,
    Wires,
};

pub struct AiAirlockWindow;

impl UiInterface for AiAirlockWindow {
    const NAME: &'static str = AiAirlockUi::NAME;
    type Data = AiAirlockData;
    type StaticData = ();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AirlockAct {
    DisruptMain,
    DisruptBackup,
    ShockRestore,
    ShockTemp,
    ShockPerm,
    IdScan,
    Emergency,
    Bolts,
    Feedback,
    Safe,
    Speed,
    OpenClose,
}

impl ServerUi for AiAirlockWindow {
    type Source = (
        &'static Door,
        Option<&'static Wires>,
        Option<&'static Electrified>,
    );
    type UserParam = ();
    type Act = AirlockAct;

    fn static_data(_: (&Door, Option<&Wires>, Option<&Electrified>)) {}

    fn data(
        (door, wires, shock): (&Door, Option<&Wires>, Option<&Electrified>),
        _: Entity,
        _: &SystemParamItem<'_, '_, ()>,
    ) -> AiAirlockData {
        let intact = |w: &str| !wires.is_some_and(|wires| wires.is_cut(w));
        // DM `round(x)` floors.
        let secs = |t: f64| t.floor() as i32;
        AiAirlockData {
            power: AirlockPower {
                main: if door.main_outage > 0.0 { 0 } else { 2 },
                main_timeleft: secs(door.main_outage),
                backup: if door.backup_outage > 0.0 { 0 } else { 2 },
                backup_timeleft: secs(door.backup_outage),
            },
            shock: if shock.is_none() { 2 } else { 0 },
            shock_timeleft: match shock {
                None => 0,
                Some(Electrified::Permanent) => -1,
                Some(Electrified::Timed(t)) => t.ceil() as i32,
            },
            id_scanner: !door.ai_disabled_id_scanner,
            emergency: door.emergency,
            locked: door.locked,
            feedback: door.feedback,
            safe: door.safe,
            speed: door.normalspeed,
            welded: door.welded,
            opened: door.state == DoorState::Open,
            wires: AirlockWires {
                main_1: intact(WIRE_POWER1),
                main_2: intact(WIRE_POWER2),
                backup_1: intact(WIRE_BACKUP1),
                backup_2: intact(WIRE_BACKUP2),
                shock: intact(WIRE_SHOCK),
                id_scanner: intact(WIRE_IDSCAN),
                bolts: intact(WIRE_BOLTS),
                feedback: intact(WIRE_FEEDBACK),
                safe: intact(WIRE_SAFETY),
                timing: intact(WIRE_TIMING),
            },
        }
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<AirlockAct, UiReject> {
        Ok(match action {
            "disrupt-main" => AirlockAct::DisruptMain,
            "disrupt-backup" => AirlockAct::DisruptBackup,
            "shock-restore" => AirlockAct::ShockRestore,
            "shock-temp" => AirlockAct::ShockTemp,
            "shock-perm" => AirlockAct::ShockPerm,
            "idscan-toggle" => AirlockAct::IdScan,
            "emergency-toggle" => AirlockAct::Emergency,
            "bolt-toggle" => AirlockAct::Bolts,
            "light-toggle" => AirlockAct::Feedback,
            "safe-toggle" => AirlockAct::Safe,
            "speed-toggle" => AirlockAct::Speed,
            "open-close" => AirlockAct::OpenClose,
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

pub struct AiAirlockPlugin;

impl Plugin for AiAirlockPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<AiAirlockWindow>()
            .add_systems(Update, apply_acts.in_set(UiSystems::Handle));
    }
}

/// tg airlock `attack_ai`.
pub fn attack_ai(world: &mut World, client: Entity, user: Entity, door: Entity) {
    if !can_ai_control(world, door) {
        warn(
            world,
            client,
            "Airlock AI control has been blocked with a firewall. Unable to hack.",
        );
    }
    if world.get::<Door>(door).is_some_and(|d| d.emagged) {
        warn(
            world,
            client,
            "Unable to interface: Airlock is unresponsive.",
        );
        return;
    }
    let title = world.get::<DmName>(door).map_or_else(String::new, |n| {
        crate::vending::plain_name(&n.name).to_owned()
    });
    world.write_message(OpenUi::new::<AiAirlockWindow>(client, user, door, title));
}

fn apply_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<AiAirlockWindow>>>) {
    let pending: Vec<(Entity, Entity, Entity, AirlockAct)> = acts
        .read(world.resource::<Messages<UiActed<AiAirlockWindow>>>())
        .map(|a| (a.client, a.actor, a.source, a.act))
        .collect();
    for (client, actor, door, act) in pending {
        apply_act(world, client, actor, door, act);
    }
}

/// `ui_act` at the moment of effect.
pub fn apply_act(world: &mut World, client: Entity, actor: Entity, door: Entity, act: AirlockAct) {
    // `user_allowed`.
    if !has_silicon_access(world, actor) || !can_ai_control(world, door) {
        return;
    }
    let Some(d) = world.get::<Door>(door) else {
        return;
    };
    let shock_cut = world
        .get::<Wires>(door)
        .is_some_and(|w| w.is_cut(WIRE_SHOCK));
    match act {
        AirlockAct::DisruptMain => {
            if d.main_outage > 0.0 {
                warn(world, client, "Main power is already offline.");
            } else {
                crate::wires::lose_power(world, door, true);
            }
        }
        AirlockAct::DisruptBackup => {
            if d.backup_outage > 0.0 {
                warn(world, client, "Backup power is already offline.");
            } else {
                crate::wires::lose_power(world, door, false);
            }
        }
        AirlockAct::ShockRestore => {
            if shock_cut {
                warn(
                    world,
                    client,
                    "Can't un-electrify the airlock - The electrification wire is cut.",
                );
            } else if world.get::<Electrified>(door).is_some() {
                crate::wires::set_electrified(world, door, None);
            }
        }
        AirlockAct::ShockTemp | AirlockAct::ShockPerm => {
            if shock_cut {
                warn(world, client, "The electrification wire has been cut.");
            } else {
                let value = if act == AirlockAct::ShockTemp {
                    Electrified::Timed(MACHINE_DEFAULT_ELECTRIFY_TIME)
                } else {
                    Electrified::Permanent
                };
                crate::wires::set_electrified(world, door, Some(value));
            }
        }
        AirlockAct::Bolts => toggle_bolt(world, client, door),
        AirlockAct::OpenClose => user_toggle_open(world, client, door),
        AirlockAct::IdScan | AirlockAct::Emergency | AirlockAct::Safe | AirlockAct::Speed => {
            if let Some(mut d) = world.get_mut::<Door>(door) {
                match act {
                    AirlockAct::IdScan => d.ai_disabled_id_scanner = !d.ai_disabled_id_scanner,
                    AirlockAct::Emergency => d.emergency = !d.emergency,
                    AirlockAct::Safe => d.safe = !d.safe,
                    _ => d.normalspeed = !d.normalspeed,
                }
            }
        }
        AirlockAct::Feedback => {
            crate::doors::with_door(world, door, |d, appearance, _| {
                d.feedback = !d.feedback;
                d.apply_look(appearance);
                ((), false)
            });
        }
    }
}
