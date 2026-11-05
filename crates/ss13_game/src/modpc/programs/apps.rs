//! The station programs of a modular computer (server), on the systems that already exist:
//!
//! - Canary (`/datum/computer_file/program/alarm_monitor`): the station's atmosphere, fire and
//!   power alarms (`crate::silicon::alarms`), the header icon red while any is on;
//! - Security and Medical Records (`/datum/computer_file/program/records`): the readable part of
//!   every crew record (`crate::records::manifest`);
//! - SignalCommander (`/datum/computer_file/program/signal_commander`): a frequency and code,
//!   sent to every signaler on it (`crate::assemblies::server::post_signal`), 1 s cooldown;
//! - Status Display (`/datum/computer_file/program/status`): two lines or a picture on every
//!   evac display (`crate::security_level::displays::post_status`);
//! - Newscaster (`/datum/computer_file/program/newscaster`): tg runs a hidden newscaster's
//!   interface; here the computer itself becomes that newscaster
//!   (`crate::newscaster_server::Newscaster`, never announcing, never clicked) and the program
//!   opens its `PhysicalNewscaster` window.
// shortcut: no circuit components, no signaler investigate log, no NTNet requirement (the
// signal is always good); the records are read when shown, not only on open (`ui_static_data`).

use bevy::prelude::*;
use ss13_chat::radio::{MAX_FREE_FREQ, MIN_FREE_FREQ, sanitize_frequency};
use ss13_net::chat::server::SendChat;
use ss13_shared::chat::{ChatChannel, ChatMessage, Span};
use ss13_tgui::server::{OpenUi, UiParams, UiReject};

use super::ProgramKind;
use crate::assemblies::acts;
use crate::assemblies::server::{DEFAULT_SIGNALER_CODE, FREQ_SIGNALER};
use crate::comms::{
    LOWER_TEXT, SET_STATUS_MESSAGE, SET_STATUS_PICTURE, STATUS_PICTURES, UPPER_TEXT,
};
use crate::comms_server::{MAX_STATUS_LINE_LENGTH, reject_bad_text};
use crate::modpc::pda::ModularComputer;
use crate::modpc::{MedicalRow, SecurityRow, StatusDisplayData};
use crate::records::manifest::Manifest;
use crate::security_level::displays::StatusCommand;
use crate::silicon::alarms::{AlarmKind, AlertListener, AlertScope, StationAlarms};

/// `signal_cooldown_time`.
const SIGNAL_COOLDOWN: f64 = 1.0;
/// The `picture` param of `setStatusPicture`.
const PICTURE: &str = "picture";

/// The program state a computer keeps besides [`ModularComputer`].
#[derive(Component, Clone, Debug, PartialEq)]
pub struct AppState {
    /// SignalCommander `signal_frequency`, `signal_code` and the cooldown's end
    /// (`Time::elapsed_secs_f64`).
    pub signal_frequency: u16,
    pub signal_code: i32,
    pub signal_ready: f64,
    /// Status Display `upper_text`, `lower_text`.
    pub upper_text: String,
    pub lower_text: String,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            signal_frequency: FREQ_SIGNALER,
            signal_code: DEFAULT_SIGNALER_CODE,
            signal_ready: 0.0,
            upper_text: String::new(),
            lower_text: String::new(),
        }
    }
}

/// `alert_control = new(computer, list(ALARM_ATMOS, ALARM_FIRE, ALARM_POWER), listener_areas =
/// station areas + mining)`.
pub fn alarm_listener() -> AlertListener {
    AlertListener {
        kinds: vec![AlarmKind::Atmosphere, AlarmKind::Fire, AlarmKind::Power],
        scope: AlertScope::Station,
        camera_view: false,
    }
}

/// `has_alert`: the listener has an alarm.
pub fn has_alert(alarms: Option<&StationAlarms>) -> bool {
    let listener = alarm_listener();
    alarms.is_some_and(|a| a.listed(&listener).next().is_some())
}

/// `GetRecordsReadable()` in `security` mode.
pub fn security_rows(manifest: Option<&Manifest>) -> Vec<SecurityRow> {
    manifest
        .map(|m| {
            m.general
                .iter()
                .map(|r| SecurityRow {
                    name: r.name.clone(),
                    rank: r.rank.clone(),
                    species: r.species.clone(),
                    gender: r.gender.clone(),
                    age: r.age,
                    fingerprint: r.fingerprint.clone(),
                    wanted: r.wanted_status.clone(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `GetRecordsReadable()` in `medical` mode.
pub fn medical_rows(manifest: Option<&Manifest>) -> Vec<MedicalRow> {
    manifest
        .map(|m| {
            m.general
                .iter()
                .map(|r| MedicalRow {
                    name: r.name.clone(),
                    bloodtype: r.blood_type.clone(),
                    mi_dis: r.minor_disabilities_desc.clone(),
                    ma_dis: r.major_disabilities_desc.clone(),
                    physical_status: r.physical_status.clone(),
                    mental_status: r.mental_status.clone(),
                    notes: r
                        .medical_notes
                        .iter()
                        .map(|n| n.content.as_str())
                        .collect::<Vec<_>>()
                        .join(" "),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The SignalCommander's `ui_data`.
pub fn signaler_data(state: Option<&AppState>) -> crate::assemblies::SignalerData {
    let fallback = AppState::default();
    let state = state.unwrap_or(&fallback);
    crate::assemblies::SignalerData {
        frequency: state.signal_frequency,
        cooldown: (SIGNAL_COOLDOWN * 10.0) as u32,
        code: state.signal_code,
        min_frequency: MIN_FREE_FREQ,
        max_frequency: MAX_FREE_FREQ,
    }
}

/// The Status Display's `ui_data` + `ui_static_data`.
pub fn status_data(state: Option<&AppState>) -> StatusDisplayData {
    StatusDisplayData {
        upper_text: state.map(|s| s.upper_text.clone()).unwrap_or_default(),
        lower_text: state.map(|s| s.lower_text.clone()).unwrap_or_default(),
        max_status_line_length: MAX_STATUS_LINE_LENGTH as u32,
    }
}

/// The acts of these programs, well-formed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppAct {
    /// `signal`.
    Signal,
    /// `freq`: the sanitized frequency (deci-kHz).
    Frequency(u16),
    /// `code`: rounded, 1-100.
    Code(i32),
    /// `reset`: the frequency (true) or the code.
    Reset(bool),
    /// `setStatusMessage`: the raw lines.
    StatusMessage(String, String),
    /// `setStatusPicture`: an approved picture.
    StatusPicture(String),
}

/// `None` for an action that is not one of these programs'.
pub fn parse(action: &str, params: &UiParams) -> Option<Result<AppAct, UiReject>> {
    let line = |key: &str| params.get(key).unwrap_or_default().to_owned();
    Some(match action {
        acts::SIGNAL => Ok(AppAct::Signal),
        acts::FREQ => params
            .number(acts::FREQ)
            .map(|khz| AppAct::Frequency(sanitize_frequency(khz * 10.0, true, false))),
        // tg takes any number (`round(text2num)`); the circuit port clamps it to 1-100.
        acts::CODE => params
            .number(acts::CODE)
            .map(|c| AppAct::Code((c.round() as i64).clamp(1, 100) as i32)),
        acts::RESET => Ok(AppAct::Reset(params.get(acts::RESET) == Some(acts::FREQ))),
        SET_STATUS_MESSAGE => Ok(AppAct::StatusMessage(line(UPPER_TEXT), line(LOWER_TEXT))),
        SET_STATUS_PICTURE => match params.get(PICTURE) {
            Some(p) if STATUS_PICTURES.contains(&p) => Ok(AppAct::StatusPicture(p.to_owned())),
            _ => Err(UiReject::BadParams(
                "picture: not an approved picture".into(),
            )),
        },
        _ => return None,
    })
}

fn tell(world: &mut World, client: Entity, span: Span, text: &str) {
    world.write_message(SendChat {
        clients: vec![client],
        message: ChatMessage::simple(ChatChannel::System, span, text),
    });
}

/// `ui_act` of the program in front of `computer`.
pub fn apply(
    world: &mut World,
    client: Entity,
    actor: Entity,
    pc: Entity,
    computer: &ModularComputer,
    act: AppAct,
) -> Result<(), &'static str> {
    let program = match act {
        AppAct::Signal | AppAct::Frequency(_) | AppAct::Code(_) | AppAct::Reset(_) => {
            ProgramKind::Signaler
        }
        AppAct::StatusMessage(..) | AppAct::StatusPicture(_) => ProgramKind::StatusDisplay,
    };
    if computer.active != Some(program) {
        return Err("program not in front");
    }
    let mut state = world.get::<AppState>(pc).cloned().unwrap_or_default();
    match act {
        AppAct::Signal => {
            let now = world.resource::<Time>().elapsed_secs_f64();
            if now < state.signal_ready {
                tell(world, client, Span::Warning, "cooling down!");
                return Err("cooling down");
            }
            state.signal_ready = now + SIGNAL_COOLDOWN;
            info!(
                "{actor} used the computer {pc}: {}/{}",
                crate::assemblies::format_frequency(state.signal_frequency),
                state.signal_code
            );
            let (frequency, code) = (state.signal_frequency, state.signal_code);
            world.entity_mut(pc).insert(state);
            crate::assemblies::server::post_signal(world, pc, frequency, code, 0);
            tell(world, client, Span::Notice, "signaled");
            return Ok(());
        }
        AppAct::Frequency(f) => state.signal_frequency = f,
        AppAct::Code(c) => state.signal_code = c,
        AppAct::Reset(true) => state.signal_frequency = FREQ_SIGNALER,
        AppAct::Reset(false) => state.signal_code = DEFAULT_SIGNALER_CODE,
        AppAct::StatusMessage(upper, lower) => {
            state.upper_text = reject_bad_text(&upper, MAX_STATUS_LINE_LENGTH).unwrap_or_default();
            state.lower_text = reject_bad_text(&lower, MAX_STATUS_LINE_LENGTH).unwrap_or_default();
            info!(
                "{actor} has changed the station status display message to \"{} {}\"",
                state.upper_text, state.lower_text
            );
            let command =
                StatusCommand::Message(state.upper_text.clone(), state.lower_text.clone());
            world.entity_mut(pc).insert(state);
            crate::security_level::displays::post_status(world, command);
            return Ok(());
        }
        AppAct::StatusPicture(picture) => {
            let command = match picture.as_str() {
                "blank" => StatusCommand::Blank,
                "shuttle" => StatusCommand::Shuttle,
                // You cannot set Code Blue display during Code Red and similar.
                "currentalert" => StatusCommand::Alert(
                    crate::security_level::current(world)
                        .status_display_icon_state()
                        .to_owned(),
                ),
                p => StatusCommand::Alert(p.to_owned()),
            };
            info!("{actor} has changed the station status display message to \"{picture}\"");
            crate::security_level::displays::post_status(world, command);
            return Ok(());
        }
    }
    world.entity_mut(pc).insert(state);
    Ok(())
}

/// The newscaster app: the computer's hidden newscaster window (`newscaster_ui.ui_interact`).
pub fn open_newscaster(world: &mut World, client: Entity, actor: Entity, pc: Entity) {
    if world
        .get::<crate::newscaster_server::Newscaster>(pc)
        .is_none()
    {
        world
            .entity_mut(pc)
            .insert(crate::newscaster_server::Newscaster::default());
    }
    world.write_message(OpenUi::new::<crate::newscaster::NewscasterUi>(
        client,
        actor,
        pc,
        ProgramKind::Newscaster.filedesc(),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_with(action: &str, params: &[(&str, &str)]) -> Option<Result<AppAct, UiReject>> {
        let params: Vec<(String, String)> = params
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        parse(action, &UiParams::new(&params))
    }

    #[test]
    fn acts_parse_hostile_params() {
        assert_eq!(
            parse_with(acts::FREQ, &[(acts::FREQ, "145.9")]),
            Some(Ok(AppAct::Frequency(1459)))
        );
        // Out of the free band: clamped, odd.
        assert_eq!(
            parse_with(acts::FREQ, &[(acts::FREQ, "999")]),
            Some(Ok(AppAct::Frequency(MAX_FREE_FREQ)))
        );
        assert!(matches!(
            parse_with(acts::FREQ, &[(acts::FREQ, "NaN")]),
            Some(Err(_))
        ));
        assert_eq!(
            parse_with(acts::CODE, &[(acts::CODE, "1e9")]),
            Some(Ok(AppAct::Code(100)))
        );
        assert!(matches!(
            parse_with(SET_STATUS_PICTURE, &[(PICTURE, "nukeops")]),
            Some(Err(_))
        ));
        assert_eq!(
            parse_with(SET_STATUS_MESSAGE, &[(UPPER_TEXT, "ЭВАК")]),
            Some(Ok(AppAct::StatusMessage("ЭВАК".into(), String::new())))
        );
        assert_eq!(parse_with("PC_exit", &[]), None);
    }
}
