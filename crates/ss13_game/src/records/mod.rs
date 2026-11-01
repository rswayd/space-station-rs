//! Crew records after tg `GLOB.manifest` (`/datum/manifest`, `/datum/record/crew`,
//! `/datum/record/locked`, `/datum/crime`), the security and medical records consoles
//! (`/obj/machinery/computer/records/{security,medical}`, tgui `SecurityRecords` and
//! `MedicalRecords`) and the security and medical data HUDs (`/datum/atom_hud/data/human`).
//!
//! [`manifest`] holds the records, [`console`] the consoles' server half, [`hud`] computes the
//! HUD icons, [`client`] draws the windows and the icons. This module holds what both halves
//! share: the windows' data and the headless `DataHud` interface.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

use crate::hud::HudIcon;

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod console;
#[cfg(feature = "server")]
pub mod hud;
#[cfg(feature = "server")]
pub mod manifest;

/// Records on the server: the manifest, the consoles, the data HUDs.
#[cfg(feature = "server")]
pub struct RecordsServerPlugin;

#[cfg(feature = "server")]
impl bevy::prelude::Plugin for RecordsServerPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.init_resource::<manifest::Manifest>()
            .add_plugins((console::RecordsConsolePlugin, hud::DataHudPlugin));
    }
}

/// `WANTED_STATUSES()` (BandaStation's Russian names), in tg's order.
pub const WANTED_NONE: &str = "Нет";
pub const WANTED_SUSPECT: &str = "Подозрение";
pub const WANTED_ARREST: &str = "Арест";
pub const WANTED_PRISONER: &str = "Заключение";
pub const WANTED_PAROLE: &str = "УДО";
pub const WANTED_DISCHARGED: &str = "Оправдано";
pub const WANTED_STATUSES: [&str; 6] = [
    WANTED_NONE,
    WANTED_SUSPECT,
    WANTED_ARREST,
    WANTED_PRISONER,
    WANTED_PAROLE,
    WANTED_DISCHARGED,
];

/// `PHYSICAL_STATUSES`.
pub const PHYSICAL_ACTIVE: &str = "Активен";
pub const PHYSICAL_DECEASED: &str = "Мертв";
pub const PHYSICAL_STATUSES: [&str; 4] = [
    PHYSICAL_ACTIVE,
    "Болезнь",
    "Бессознательность",
    PHYSICAL_DECEASED,
];
/// `MENTAL_STATUSES`.
pub const MENTAL_STABLE: &str = "Стабильно";
pub const MENTAL_STATUSES: [&str; 4] = [MENTAL_STABLE, "Наблюдение", "Нестабильно", "Безумство"];
/// `get_roundstart_blood_types()` names.
pub const BLOOD_TYPES: [&str; 8] = ["A-", "A+", "B-", "B+", "AB-", "AB+", "O-", "O+"];
/// `AGE_MIN`, `AGE_MAX`.
pub const AGE_MIN: u32 = 17;
pub const AGE_MAX: u32 = 85;

/// tgui `SecurityRecords`.
pub struct SecurityRecordsUi;

impl UiInterface for SecurityRecordsUi {
    const NAME: &'static str = "SecurityRecords";
    type Data = SecurityRecordsData;
    type StaticData = AgeLimits;
}

/// `ui_static_data` of both consoles (`min_age`, `max_age`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgeLimits {
    pub min_age: u32,
    pub max_age: u32,
}

/// `/obj/machinery/computer/records/security/ui_data`; only `authenticated` until a login.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityRecordsData {
    pub authenticated: bool,
    pub station_z: bool,
    pub available_statuses: Vec<String>,
    pub current_user: String,
    pub higher_access: bool,
    pub records: Vec<SecurityRecord>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityRecord {
    pub age: u32,
    pub citations: Vec<CrimeEntry>,
    pub crew_ref: String,
    pub crimes: Vec<CrimeEntry>,
    pub fingerprint: String,
    pub gender: String,
    pub name: String,
    pub note: String,
    pub rank: String,
    pub species: String,
    pub trim: String,
    pub wanted_status: String,
    pub voice: String,
}

/// A crime or a citation (`fine` set) of a record.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrimeEntry {
    pub author: String,
    pub crime_ref: String,
    pub details: String,
    pub fine: Option<u32>,
    pub name: String,
    pub paid: u32,
    pub time: String,
    pub valid: bool,
    pub voider: Option<String>,
}

/// tgui `MedicalRecords`.
pub struct MedicalRecordsUi;

impl UiInterface for MedicalRecordsUi {
    const NAME: &'static str = "MedicalRecords";
    type Data = MedicalRecordsData;
    type StaticData = MedicalRecordsStatic;
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MedicalRecordsStatic {
    pub min_age: u32,
    pub max_age: u32,
    pub physical_statuses: Vec<String>,
    pub mental_statuses: Vec<String>,
    pub blood_types: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MedicalRecordsData {
    pub authenticated: bool,
    pub station_z: bool,
    pub records: Vec<MedicalRecord>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MedicalRecord {
    pub age: u32,
    pub blood_type: String,
    pub crew_ref: String,
    pub dna: String,
    pub gender: String,
    pub major_disabilities: String,
    pub minor_disabilities: String,
    pub physical_status: String,
    pub cause_of_death: String,
    pub mental_status: String,
    pub name: String,
    pub notes: Vec<MedicalNoteEntry>,
    pub quirk_notes: String,
    pub rank: String,
    pub species: String,
    pub trim: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MedicalNoteEntry {
    pub author: String,
    pub content: String,
    pub note_ref: String,
    pub time: String,
}

// Acts of both consoles (`act('...')`).
pub const LOGIN: &str = "login";
pub const LOGOUT: &str = "logout";
pub const EDIT_FIELD: &str = "edit_field";
pub const EXPUNGE_RECORD: &str = "expunge_record";
pub const PURGE_RECORDS: &str = "purge_records";
pub const VIEW_RECORD: &str = "view_record";
pub const ADD_CRIME: &str = "add_crime";
pub const DELETE_RECORD: &str = "delete_record";
pub const EDIT_CRIME: &str = "edit_crime";
pub const INVALIDATE_CRIME: &str = "invalidate_crime";
pub const SET_NOTE: &str = "set_note";
pub const SET_WANTED: &str = "set_wanted";
pub const ADD_NOTE: &str = "add_note";
pub const DELETE_NOTE: &str = "delete_note";
pub const SET_PHYSICAL_STATUS: &str = "set_physical_status";
pub const SET_MENTAL_STATUS: &str = "set_mental_status";
pub const SET_CAUSE_OF_DEATH: &str = "set_cause_of_death";
pub const SET_BLOOD_TYPE: &str = "set_blood_type";

/// The data HUD images one mob sees over others (no window): tg's `hud_list` images of the
/// security and medical data HUDs it wears glasses for.
pub struct DataHudUi;

impl UiInterface for DataHudUi {
    const NAME: &'static str = "DataHud";
    type Data = DataHudData;
    type StaticData = ();
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(bevy::prelude::Component))]
pub struct DataHudData {
    /// Per seen mob, by the viewer's net id of it: its HUD images, bottom first.
    pub entries: Vec<DataHudEntry>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataHudEntry {
    pub net_id: u64,
    pub icons: Vec<HudIcon>,
}
