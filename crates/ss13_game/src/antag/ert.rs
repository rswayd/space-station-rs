//! Emergency response teams after tg `/datum/ert` (`code/datums/ert.dm`), `/datum/antagonist/ert`
//! (`code/modules/antagonists/ert/ert.dm`), `make_emergency_response_team`
//! (`code/modules/admin/verbs/ert.dm`) and BandaStation's manager
//! (`modular_bandastation/ert`): its templates (amber, red, gamma, ...), `ErtManager` slots.
//!
//! An admin ([`summon`], `ss13_admin`) picks a template and overrides; the manager ([`dispatch`],
//! tg `makeERTFromSlots`) builds one from slots. Either polls every ghost for 30 s
//! ([`crate::ghost_poll`]); of who signed up the leader is the first picked, the rest get the
//! template's roles (`roles[WRAP(numagents, 1, length + 1)]`, tg's mapping from the remaining
//! count). Each body is the player's character at a landmark, gets its name
//! (`"[role] [surname]"`), the job, the outfit, a mindshield, a CentCom headset and the
//! mission ([`ErtTeam`], [`ErtMember`]).
// shortcut: admin console only (the ERT Manager is the egui panel, no tgui window), no custom
// shuttle or armoury doors (both live on the CentCom map, which the port does not load: the
// team appears at the arrivals if no `ert_spawn` landmark is on the loaded maps), no briefing
// officer spawn, no preview icon, no playtime for the experienced leader, no holy roles, no
// plasmaman outfits, every back slot is storage's grey backpack (no MODsuit) and surnames all
// come from `last.txt`.

use bevy::prelude::*;
use ss13_mapload::jobs::Outfit;
use ss13_mapload::{DmName, MapLoadPlugin, Template};
use ss13_net::server::Networked;
use ss13_shared::TilePos;
use ss13_shared::admin::{ErtSlots, ErtSummon};
use ss13_shared::chat::Span;

use crate::combat::CombatRng;
use crate::ghost_poll::{self, Candidate, DEFAULT_POLL_SECS, PollSpec};
use crate::id_access;
use crate::inventory::IdCard;
use crate::noncrew_jobs;
use crate::outfit::IdRegistration;
use crate::species::SpeciesId;
use crate::storage;
use crate::tools::tell;

/// tg `FREQ_CENTCOM`.
const FREQ_CENTCOM: u16 = ss13_chat::radio::FREQ_CENTCOM;
/// The most members the panels may ask for (tg leaves it unbounded).
pub const MAX_TEAM_SIZE: u32 = 30;
const MAX_TEXT: usize = 1024;

/// `/datum/antagonist/ert` vars of one role.
#[derive(Debug)]
pub struct Role {
    pub path: &'static str,
    /// `name`: what the greeting calls it.
    pub name: &'static str,
    /// `role`: the title in random names.
    pub title: &'static str,
    pub outfit: &'static str,
    pub job: &'static str,
}

const fn role(
    path: &'static str,
    name: &'static str,
    title: &'static str,
    outfit: &'static str,
    job: &'static str,
) -> Role {
    Role {
        path,
        name,
        title,
        outfit,
        job,
    }
}

const OFFICER: &str = "Офицер ОБР";
const JOB_ERT: &str = "/datum/job/ert";
const JOB_COMMANDER: &str = "/datum/job/ert/commander";
const JOB_OFFICER: &str = "/datum/job/ert/officer";
const JOB_ENGINEER: &str = "/datum/job/ert/engineer";
const JOB_MEDIC: &str = "/datum/job/ert/medical_doctor";
const JOB_JANITOR: &str = "/datum/job/ert/janitor";
const JOB_CHAPLAIN: &str = "/datum/job/ert/chaplain";
const JOB_CLOWN: &str = "/datum/job/ert/clown";
const JOB_DEATHSQUAD: &str = "/datum/job/ert/deatsquad";
const JOB_OFFICIAL: &str = "/datum/job/ert/official";
const JOB_INTERN: &str = "/datum/job/ert/intern";
const JOB_INTERN_LEADER: &str = "/datum/job/ert/intern/leader";

const SEC: &str = "Сотрудник службы безопасности";
const ENG: &str = "Инженер";
const MED: &str = "Медик";
const CMD: &str = "Командир";
const JAN: &str = "Уборщик";
const CHP: &str = "Священник";
const CLW: &str = "Клоун";

/// Every `/datum/antagonist/ert` subtype and BandaStation's amber/red/gamma ones.
pub static ROLES: &[Role] = &[
    role(
        "/datum/antagonist/ert/security",
        OFFICER,
        SEC,
        "/datum/outfit/centcom/ert/security",
        JOB_OFFICER,
    ),
    role(
        "/datum/antagonist/ert/security/amber",
        OFFICER,
        SEC,
        "/datum/outfit/centcom/ert/security/amber",
        JOB_OFFICER,
    ),
    role(
        "/datum/antagonist/ert/security/red",
        OFFICER,
        SEC,
        "/datum/outfit/centcom/ert/security/red",
        JOB_OFFICER,
    ),
    role(
        "/datum/antagonist/ert/security/gamma",
        OFFICER,
        SEC,
        "/datum/outfit/centcom/ert/security/gamma",
        JOB_OFFICER,
    ),
    role(
        "/datum/antagonist/ert/security/inquisitor",
        OFFICER,
        SEC,
        "/datum/outfit/centcom/ert/security/inquisitor",
        JOB_OFFICER,
    ),
    role(
        "/datum/antagonist/ert/security/party",
        OFFICER,
        "Вышибала праздника",
        "/datum/outfit/centcom/ert/security/party",
        JOB_OFFICER,
    ),
    role(
        "/datum/antagonist/ert/engineer",
        OFFICER,
        ENG,
        "/datum/outfit/centcom/ert/engineer",
        JOB_ENGINEER,
    ),
    role(
        "/datum/antagonist/ert/engineer/amber",
        OFFICER,
        ENG,
        "/datum/outfit/centcom/ert/engineer/amber",
        JOB_ENGINEER,
    ),
    role(
        "/datum/antagonist/ert/engineer/red",
        OFFICER,
        ENG,
        "/datum/outfit/centcom/ert/engineer/red",
        JOB_ENGINEER,
    ),
    role(
        "/datum/antagonist/ert/engineer/gamma",
        OFFICER,
        ENG,
        "/datum/outfit/centcom/ert/engineer/gamma",
        JOB_ENGINEER,
    ),
    role(
        "/datum/antagonist/ert/engineer/party",
        OFFICER,
        "Праздничный строитель",
        "/datum/outfit/centcom/ert/engineer/party",
        JOB_ENGINEER,
    ),
    role(
        "/datum/antagonist/ert/medic",
        OFFICER,
        MED,
        "/datum/outfit/centcom/ert/medic",
        JOB_MEDIC,
    ),
    role(
        "/datum/antagonist/ert/medic/amber",
        OFFICER,
        MED,
        "/datum/outfit/centcom/ert/medic/amber",
        JOB_MEDIC,
    ),
    role(
        "/datum/antagonist/ert/medic/red",
        OFFICER,
        MED,
        "/datum/outfit/centcom/ert/medic/red",
        JOB_MEDIC,
    ),
    role(
        "/datum/antagonist/ert/medic/gamma",
        OFFICER,
        MED,
        "/datum/outfit/centcom/ert/medic/gamma",
        JOB_MEDIC,
    ),
    role(
        "/datum/antagonist/ert/medic/inquisitor",
        OFFICER,
        MED,
        "/datum/outfit/centcom/ert/medic/inquisitor",
        JOB_MEDIC,
    ),
    role(
        "/datum/antagonist/ert/commander",
        OFFICER,
        CMD,
        "/datum/outfit/centcom/ert/commander",
        JOB_COMMANDER,
    ),
    role(
        "/datum/antagonist/ert/commander/amber",
        OFFICER,
        CMD,
        "/datum/outfit/centcom/ert/commander/amber",
        JOB_COMMANDER,
    ),
    role(
        "/datum/antagonist/ert/commander/red",
        OFFICER,
        CMD,
        "/datum/outfit/centcom/ert/commander/red",
        JOB_COMMANDER,
    ),
    role(
        "/datum/antagonist/ert/commander/gamma",
        OFFICER,
        CMD,
        "/datum/outfit/centcom/ert/commander/gamma",
        JOB_COMMANDER,
    ),
    role(
        "/datum/antagonist/ert/commander/inquisitor",
        OFFICER,
        CMD,
        "/datum/outfit/centcom/ert/commander/inquisitor",
        JOB_COMMANDER,
    ),
    role(
        "/datum/antagonist/ert/commander/party",
        OFFICER,
        "Координатор праздника",
        "/datum/outfit/centcom/ert/commander/party",
        JOB_COMMANDER,
    ),
    role(
        "/datum/antagonist/ert/janitor",
        OFFICER,
        JAN,
        "/datum/outfit/centcom/ert/janitor",
        JOB_JANITOR,
    ),
    role(
        "/datum/antagonist/ert/janitor/heavy",
        OFFICER,
        "Уборщик-специалист",
        "/datum/outfit/centcom/ert/janitor/heavy",
        JOB_JANITOR,
    ),
    role(
        "/datum/antagonist/ert/janitor/amber",
        OFFICER,
        JAN,
        "/datum/outfit/centcom/ert/janitor/amber",
        JOB_JANITOR,
    ),
    role(
        "/datum/antagonist/ert/janitor/red",
        OFFICER,
        JAN,
        "/datum/outfit/centcom/ert/janitor/red",
        JOB_JANITOR,
    ),
    role(
        "/datum/antagonist/ert/janitor/gamma",
        OFFICER,
        JAN,
        "/datum/outfit/centcom/ert/janitor/gamma",
        JOB_JANITOR,
    ),
    role(
        "/datum/antagonist/ert/janitor/party",
        OFFICER,
        "Служба уборки праздников",
        "/datum/outfit/centcom/ert/janitor/party",
        JOB_JANITOR,
    ),
    role(
        "/datum/antagonist/ert/chaplain",
        OFFICER,
        CHP,
        "/datum/outfit/centcom/ert/chaplain",
        JOB_CHAPLAIN,
    ),
    role(
        "/datum/antagonist/ert/chaplain/amber",
        OFFICER,
        CHP,
        "/datum/outfit/centcom/ert/chaplain/amber",
        JOB_CHAPLAIN,
    ),
    role(
        "/datum/antagonist/ert/chaplain/red",
        OFFICER,
        CHP,
        "/datum/outfit/centcom/ert/chaplain/red",
        JOB_CHAPLAIN,
    ),
    role(
        "/datum/antagonist/ert/chaplain/gamma",
        OFFICER,
        CHP,
        "/datum/outfit/centcom/ert/chaplain/gamma",
        JOB_CHAPLAIN,
    ),
    role(
        "/datum/antagonist/ert/chaplain/inquisitor",
        OFFICER,
        CHP,
        "/datum/outfit/centcom/ert/chaplain/inquisitor",
        JOB_CHAPLAIN,
    ),
    role(
        "/datum/antagonist/ert/clown",
        OFFICER,
        CLW,
        "/datum/outfit/centcom/ert/clown",
        JOB_CLOWN,
    ),
    role(
        "/datum/antagonist/ert/clown/amber",
        OFFICER,
        CLW,
        "/datum/outfit/centcom/ert/clown/amber",
        JOB_CLOWN,
    ),
    role(
        "/datum/antagonist/ert/clown/red",
        OFFICER,
        CLW,
        "/datum/outfit/centcom/ert/clown/red",
        JOB_CLOWN,
    ),
    role(
        "/datum/antagonist/ert/clown/gamma",
        OFFICER,
        CLW,
        "/datum/outfit/centcom/ert/clown/gamma",
        JOB_CLOWN,
    ),
    role(
        "/datum/antagonist/ert/clown/party",
        OFFICER,
        "Праздничный комик",
        "/datum/outfit/centcom/ert/clown/party",
        JOB_CLOWN,
    ),
    role(
        "/datum/antagonist/ert/deathsquad",
        "Deathsquad Trooper",
        "Trooper",
        "/datum/outfit/centcom/death_commando",
        JOB_DEATHSQUAD,
    ),
    role(
        "/datum/antagonist/ert/deathsquad/leader",
        "Deathsquad Officer",
        "Officer",
        "/datum/outfit/centcom/death_commando/officer",
        JOB_DEATHSQUAD,
    ),
    role(
        "/datum/antagonist/ert/official",
        "Представитель Центрального Командования",
        "Инспектор",
        "/datum/outfit/centcom/centcom_official",
        JOB_OFFICIAL,
    ),
    role(
        "/datum/antagonist/ert/intern",
        "Стажёр ЦК",
        "Стажёр ЦК",
        "/datum/outfit/centcom/centcom_intern",
        JOB_INTERN,
    ),
    role(
        "/datum/antagonist/ert/intern/leader",
        "Главный стажёр ЦК",
        "Главный стажёр",
        "/datum/outfit/centcom/centcom_intern/leader",
        JOB_INTERN_LEADER,
    ),
    role(
        "/datum/antagonist/ert/intern/unarmed",
        "Стажёр ЦК",
        "Стажёр ЦК",
        "/datum/outfit/centcom/centcom_intern/unarmed",
        JOB_INTERN,
    ),
    role(
        "/datum/antagonist/ert/intern/leader/unarmed",
        "Главный стажёр ЦК",
        "Главный стажёр",
        "/datum/outfit/centcom/centcom_intern/leader/unarmed",
        JOB_INTERN_LEADER,
    ),
    role(
        "/datum/antagonist/ert/marine",
        "Командир морпехов",
        CMD,
        "/datum/outfit/centcom/ert/marine",
        JOB_COMMANDER,
    ),
    role(
        "/datum/antagonist/ert/marine/security",
        "Тяжёлый морпех",
        "Боец",
        "/datum/outfit/centcom/ert/marine/security",
        JOB_OFFICER,
    ),
    role(
        "/datum/antagonist/ert/marine/engineer",
        "Инженер морпех",
        ENG,
        "/datum/outfit/centcom/ert/marine/engineer",
        JOB_ENGINEER,
    ),
    role(
        "/datum/antagonist/ert/marine/medic",
        "Медик морпех",
        "Медицинский офицер",
        "/datum/outfit/centcom/ert/marine/medic",
        JOB_MEDIC,
    ),
    role(
        "/datum/antagonist/ert/militia",
        "Пограничное ополчение",
        "Доброволец",
        "/datum/outfit/centcom/militia",
        JOB_ERT,
    ),
    role(
        "/datum/antagonist/ert/militia/general",
        "Генерал пограничного ополчения",
        "Генерал",
        "/datum/outfit/centcom/militia/general",
        JOB_ERT,
    ),
    role(
        "/datum/antagonist/ert/medical_commander",
        OFFICER,
        "Главный парамедик",
        "/datum/outfit/centcom/ert/medical_commander",
        JOB_MEDIC,
    ),
    role(
        "/datum/antagonist/ert/medical_technician",
        OFFICER,
        "Фельдшер",
        "/datum/outfit/centcom/ert/medical_technician",
        JOB_MEDIC,
    ),
    role(
        "/datum/antagonist/ert/bounty_armor",
        OFFICER,
        "Бронированный охотник за головами",
        "/datum/outfit/bountyarmor/ert",
        JOB_ERT,
    ),
    role(
        "/datum/antagonist/ert/bounty_hook",
        OFFICER,
        "Охотник за головами с крюком",
        "/datum/outfit/bountyhook/ert",
        JOB_ERT,
    ),
    role(
        "/datum/antagonist/ert/bounty_synth",
        OFFICER,
        "Синтетический охотник за головами",
        "/datum/outfit/bountysynth/ert",
        JOB_ERT,
    ),
];

/// The role of type `path`.
pub fn role_of(path: &str) -> Option<&'static Role> {
    ROLES.iter().find(|r| r.path == path)
}

/// `/datum/ert`.
#[derive(Debug)]
pub struct ErtTemplate {
    pub path: &'static str,
    /// What the panel lists.
    pub label: &'static str,
    pub leader: &'static str,
    pub roles: &'static [&'static str],
    pub team_name: Option<&'static str>,
    pub mission: &'static str,
    pub team_size: u32,
    pub polldesc: &'static str,
    pub random_names: bool,
    pub leader_experience: bool,
    pub enforce_human: bool,
}

const MISSION_HELP: &str = "Помогите станции.";
#[allow(clippy::too_many_arguments)]
const fn template(
    path: &'static str,
    label: &'static str,
    leader: &'static str,
    roles: &'static [&'static str],
    team_name: Option<&'static str>,
    mission: &'static str,
    team_size: u32,
    polldesc: &'static str,
    random_names: bool,
) -> ErtTemplate {
    ErtTemplate {
        path,
        label,
        leader,
        roles,
        team_name,
        mission,
        team_size,
        polldesc,
        random_names,
        leader_experience: true,
        enforce_human: true,
    }
}

const COMMANDER_ROLES: &[&str] = &[
    "/datum/antagonist/ert/security",
    "/datum/antagonist/ert/medic",
    "/datum/antagonist/ert/engineer",
];

const AMBER_ROLES: &[&str] = &[
    "/datum/antagonist/ert/security/amber",
    "/datum/antagonist/ert/medic/amber",
    "/datum/antagonist/ert/engineer/amber",
    "/datum/antagonist/ert/chaplain/amber",
    "/datum/antagonist/ert/janitor/amber",
    "/datum/antagonist/ert/clown/amber",
];
const RED_ROLES: &[&str] = &[
    "/datum/antagonist/ert/security/red",
    "/datum/antagonist/ert/medic/red",
    "/datum/antagonist/ert/engineer/red",
    "/datum/antagonist/ert/chaplain/red",
    "/datum/antagonist/ert/janitor/red",
    "/datum/antagonist/ert/clown/red",
];
const GAMMA_ROLES: &[&str] = &[
    "/datum/antagonist/ert/security/gamma",
    "/datum/antagonist/ert/medic/gamma",
    "/datum/antagonist/ert/engineer/gamma",
    "/datum/antagonist/ert/chaplain/gamma",
    "/datum/antagonist/ert/janitor/gamma",
    "/datum/antagonist/ert/clown/gamma",
];
const INQ_ROLES: &[&str] = &[
    "/datum/antagonist/ert/security/inquisitor",
    "/datum/antagonist/ert/medic/inquisitor",
    "/datum/antagonist/ert/chaplain/inquisitor",
];

pub const BLUE: &str = "/datum/ert/blue";
pub const AMBER: &str = "/datum/ert/amber";
pub const RED: &str = "/datum/ert/red";
pub const GAMMA: &str = "/datum/ert/gamma";
pub const INQUISITION: &str = "/datum/ert/inquisition";
const DEATHSQUAD: &str = "/datum/ert/deathsquad";
const OFFICIAL: &str = "/datum/ert/centcom_official";

/// `/datum/ert` with BandaStation's `ert_presets.dm`, in tg's file order.
pub static TEMPLATES: &[ErtTemplate] = &[
    template(
        BLUE,
        "Code Blue",
        "/datum/antagonist/ert/commander",
        COMMANDER_ROLES,
        None,
        MISSION_HELP,
        5,
        "Code Blue Nanotrasen Emergency Response Team",
        true,
    ),
    template(
        AMBER,
        "Code Amber",
        "/datum/antagonist/ert/commander/amber",
        AMBER_ROLES,
        None,
        MISSION_HELP,
        5,
        "Code Amber Emergency Response Team",
        true,
    ),
    template(
        RED,
        "Code Red",
        "/datum/antagonist/ert/commander/red",
        RED_ROLES,
        None,
        MISSION_HELP,
        5,
        "Code Red Emergency Response Team",
        true,
    ),
    template(
        GAMMA,
        "Code Gamma",
        "/datum/antagonist/ert/commander/gamma",
        GAMMA_ROLES,
        None,
        MISSION_HELP,
        5,
        "Code Gamma Emergency Response Team",
        true,
    ),
    template(
        DEATHSQUAD,
        "Deathsquad",
        "/datum/antagonist/ert/deathsquad/leader",
        &["/datum/antagonist/ert/deathsquad"],
        Some("Deathsquad"),
        "",
        5,
        "an elite Nanotrasen Strike Team",
        true,
    ),
    template(
        "/datum/ert/marine",
        "Marine Squad",
        "/datum/antagonist/ert/marine",
        &[
            "/datum/antagonist/ert/marine/security",
            "/datum/antagonist/ert/marine/engineer",
            "/datum/antagonist/ert/marine/medic",
        ],
        Some("Marine Squad"),
        MISSION_HELP,
        5,
        "an 'elite' Nanotrasen Strike Team",
        true,
    ),
    template(
        OFFICIAL,
        "CentCom Official",
        "/datum/antagonist/ert/official",
        &["/datum/antagonist/ert/official"],
        Some("CentCom Officials"),
        "",
        1,
        "a CentCom Official",
        false,
    ),
    template(
        INQUISITION,
        "Inquisition",
        "/datum/antagonist/ert/commander/inquisitor",
        INQ_ROLES,
        Some("Inquisition"),
        "Destroy any traces of paranormal activity aboard the station.",
        5,
        "Inquisition Emergency Response Team",
        true,
    ),
    template(
        "/datum/ert/janitor",
        "Janitor",
        "/datum/antagonist/ert/janitor/gamma",
        &[
            "/datum/antagonist/ert/janitor/red",
            "/datum/antagonist/ert/janitor/amber",
        ],
        Some("Janitor"),
        "Clean up EVERYTHING.",
        4,
        "Janitor Emergency Response Team",
        true,
    ),
    template(
        "/datum/ert/intern",
        "Horde of Interns",
        "/datum/antagonist/ert/intern/leader",
        &["/datum/antagonist/ert/intern"],
        Some("Horde of Interns"),
        "Assist in conflict resolution.",
        7,
        "an unpaid internship opportunity with Nanotrasen",
        false,
    ),
    template(
        "/datum/ert/intern/unarmed",
        "Unarmed Horde of Interns",
        "/datum/antagonist/ert/intern/leader/unarmed",
        &["/datum/antagonist/ert/intern/unarmed"],
        Some("Unarmed Horde of Interns"),
        "Assist in conflict resolution.",
        7,
        "an unpaid internship opportunity with Nanotrasen",
        false,
    ),
    template(
        "/datum/ert/erp",
        "Emergency Response Party",
        "/datum/antagonist/ert/commander/party",
        &[
            "/datum/antagonist/ert/security/party",
            "/datum/antagonist/ert/clown/party",
            "/datum/antagonist/ert/engineer/party",
            "/datum/antagonist/ert/janitor/party",
        ],
        Some("Emergency Response Party"),
        "Create entertainment for the crew.",
        5,
        "a Code Rainbow Nanotrasen Emergency Response Party",
        true,
    ),
    template(
        "/datum/ert/bounty_hunters",
        "Bounty Hunters",
        "/datum/antagonist/ert/bounty_armor",
        &[
            "/datum/antagonist/ert/bounty_armor",
            "/datum/antagonist/ert/bounty_hook",
            "/datum/antagonist/ert/bounty_synth",
        ],
        Some("Bounty Hunters"),
        "Assist the station in catching perps, dead or alive.",
        3,
        "a Centcom-hired bounty hunting gang",
        false,
    ),
    template(
        "/datum/ert/militia",
        "Frontier Militia",
        "/datum/antagonist/ert/militia/general",
        &["/datum/antagonist/ert/militia"],
        Some("Frontier Militia"),
        "Having heard the station's request for aid, assist the crew in defending themselves.",
        4,
        "an independent station defense militia",
        true,
    ),
    template(
        "/datum/ert/medical",
        "EMT Squad",
        "/datum/antagonist/ert/medical_commander",
        &["/datum/antagonist/ert/medical_technician"],
        Some("EMT Squad"),
        "Provide emergency medical services to the crew.",
        4,
        "an emergency medical response team",
        true,
    ),
    template(
        "/datum/ert/clown",
        "Clown",
        "/datum/antagonist/ert/clown/gamma",
        &[
            "/datum/antagonist/ert/clown/red",
            "/datum/antagonist/ert/clown/amber",
        ],
        None,
        MISSION_HELP,
        5,
        "Clown Emergency Response Team",
        true,
    ),
];

pub fn template_of(path: &str) -> Option<&'static ErtTemplate> {
    TEMPLATES.iter().find(|t| t.path == path)
}

/// `ert_outfit` paths the converter needs, every [`Role`]'s.
pub fn outfit_paths() -> impl Iterator<Item = &'static str> {
    ROLES.iter().map(|r| r.outfit)
}

/// What was asked for: a template with the admin's overrides.
#[derive(Clone, Debug)]
pub struct ErtOrder {
    pub leader: Option<&'static Role>,
    pub roles: Vec<&'static Role>,
    pub team_name: Option<&'static str>,
    pub mission: String,
    pub polldesc: String,
    pub team_size: u32,
    pub random_names: bool,
    pub enforce_human: bool,
    /// BandaStation's manager announces the outcome (the ERT's code name, `None` for a verb).
    pub announce: Option<&'static str>,
}

/// What the round has sent (`/datum/team/ert`, BandaStation's `ert_request_*` globals).
#[derive(Resource, Default, Debug)]
pub struct ErtState {
    pub teams: Vec<ErtTeam>,
    pub request_answered: bool,
    /// Newest first (`ert_request_messages`).
    pub requests: Vec<String>,
}

/// `/datum/team/ert`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErtTeam {
    pub name: String,
    pub mission: String,
}

/// `/datum/antagonist/ert` on its mob.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ErtMember {
    pub team: usize,
    pub role: &'static str,
    pub leader: bool,
}

/// `AntagData`'s outfits and their item templates.
#[derive(Resource, Default)]
struct ErtData {
    outfits: Vec<Outfit>,
    templates: std::collections::HashMap<String, Template>,
    last_names: Vec<String>,
}

pub struct ErtPlugin;

impl Plugin for ErtPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ErtState>()
            .init_resource::<ErtData>()
            .add_systems(Startup, load_data);
    }
}

fn load_data(world: &mut World) {
    let Some(assets) = world
        .get_resource::<MapLoadPlugin>()
        .map(|m| m.assets.clone())
    else {
        return;
    };
    let Ok(data) =
        ss13_mapload::antag::AntagData::read(&assets.join(ss13_mapload::antag::ANTAG_FILE))
    else {
        return;
    };
    let manifest = ss13_mapload::read_manifest(&assets).unwrap_or_default();
    let index = manifest.index();
    let mut templates = std::collections::HashMap::new();
    let prototypes = world.get_resource::<ss13_proto::Prototypes>();
    for path in data.ert_types() {
        if let Some(proto) = prototypes.and_then(|p| p.get(path).cloned()) {
            templates.insert(path.to_owned(), Template::new(proto, &index));
        }
    }
    world.insert_resource(ErtData {
        outfits: data.ert_outfits,
        templates,
        last_names: data.last_names,
    });
}

/// `slots_to_roles`: one of each category with slots left per pass over the team's size.
pub fn slots_to_roles(ert_type: &str, slots: &ErtSlots) -> Vec<&'static Role> {
    let suffix = match ert_type {
        "Amber" => "amber",
        "Red" => "red",
        "Gamma" => "gamma",
        "Inquisition" => "inquisitor",
        _ => return Vec::new(),
    };
    let inquisition = ert_type == "Inquisition";
    let mut left = [
        slots.security as u32,
        slots.medical as u32,
        slots.engineering as u32,
        slots.janitor as u32,
        slots.chaplain as u32,
        slots.clown as u32,
    ];
    let bases = [
        "security", "medic", "engineer", "janitor", "chaplain", "clown",
    ];
    let total: u32 = left.iter().sum();
    let mut out = Vec::new();
    for _ in 0..total {
        for (i, base) in bases.iter().enumerate() {
            // tg's manager has no engineer, janitor or clown inquisitors.
            if inquisition && matches!(*base, "engineer" | "janitor" | "clown") {
                left[i] = 0;
                continue;
            }
            if left[i] > 0 {
                left[i] -= 1;
                let path = format!("/datum/antagonist/ert/{base}/{suffix}");
                if let Some(role) = role_of(&path) {
                    out.push(role);
                }
            }
        }
    }
    out
}

/// `Summon ERT`: the admin's edited template. The texts are the caller's, sanitized.
pub fn order_from_summon(
    world: &mut World,
    request: &ErtSummon,
    mission: String,
    polldesc: String,
) -> Result<ErtOrder, String> {
    let template = template_of(&request.template).ok_or("Неизвестный шаблон ОБР.")?;
    if !(1..=MAX_TEAM_SIZE).contains(&u32::from(request.team_size)) {
        return Err(format!("Размер отряда: 1-{MAX_TEAM_SIZE}."));
    }
    let mission = if mission.is_empty() {
        default_mission(world, template)
    } else {
        mission
    };
    let polldesc = if polldesc.is_empty() {
        template.polldesc.to_owned()
    } else {
        polldesc
    };
    let roles = template.roles.iter().filter_map(|p| role_of(p)).collect();
    Ok(ErtOrder {
        leader: role_of(template.leader),
        roles,
        team_name: template.team_name,
        mission,
        polldesc,
        team_size: u32::from(request.team_size),
        random_names: request.random_names,
        enforce_human: request.enforce_human,
        announce: None,
    })
}

/// `makeERTFromSlots`: the manager's team. The caller has checked the slots.
pub fn order_from_slots(
    world: &mut World,
    ert_type: &str,
    commander: bool,
    slots: &ErtSlots,
) -> Result<ErtOrder, String> {
    let template_path = match ert_type {
        "Amber" => AMBER,
        "Red" => RED,
        "Gamma" => GAMMA,
        "Inquisition" => INQUISITION,
        _ => return Err("Неизвестный тип ОБР.".into()),
    };
    let template = template_of(template_path).ok_or("Неизвестный шаблон ОБР.")?;
    let roles = slots_to_roles(ert_type, slots);
    let team_size = roles.len() as u32 + u32::from(commander);
    if team_size == 0 {
        return Err("В ОБР должен быть хотя бы один слот.".into());
    }
    if team_size > MAX_TEAM_SIZE {
        return Err(format!("Размер отряда: 1-{MAX_TEAM_SIZE}."));
    }
    Ok(ErtOrder {
        leader: commander.then(|| role_of(template.leader)).flatten(),
        roles,
        team_name: template.team_name,
        mission: default_mission(world, template),
        polldesc: template.polldesc.to_owned(),
        team_size,
        random_names: template.random_names,
        enforce_human: template.enforce_human,
        announce: Some(match ert_type {
            "Amber" => "Amber",
            "Red" => "Red",
            "Gamma" => "Gamma",
            _ => "Inquisition",
        }),
    })
}

/// The template's `mission`; the two that name the station or its self-destruct codes are
/// built here (`/datum/ert/deathsquad/New`, `/datum/ert/centcom_official/New`).
fn default_mission(world: &mut World, template: &'static ErtTemplate) -> String {
    let station = crate::events::station_name(world);
    match template.path {
        DEATHSQUAD => {
            let code = crate::antag::nukeop::server::selfdestruct(world)
                .and_then(|nuke| {
                    world
                        .get::<crate::antag::nukeop::bomb::NuclearBomb>(nuke)
                        .map(|b| b.state.r_code.clone())
                })
                .unwrap_or_else(|| "неизвестны".to_owned());
            format!("Запустите механизм самоуничтожения на {station}, коды активации: {code}")
        }
        OFFICIAL => format!("Проведите проверку работоспособности {station} и её Капитана."),
        _ => template.mission.to_owned(),
    }
}

/// `make_emergency_response_team`: the poll; [`spawn_team`] when it ends.
pub fn dispatch(world: &mut World, order: ErtOrder) {
    let spec = PollSpec {
        question: format!("Вы хотите стать частью: {}?", order.polldesc),
        role_name: "отряд быстрого реагирования".to_owned(),
        secs: DEFAULT_POLL_SECS,
        ..default()
    };
    let group = ghost_poll::ghosts(world);
    ghost_poll::poll_candidates(world, spec, group, move |world, signed_up| {
        let spawned = spawn_team(world, &order, signed_up);
        report(world, &order, spawned);
    });
}

/// The manager's outcome announcements (`makeERTFromSlots`).
fn report(world: &mut World, order: &ErtOrder, spawned: u32) {
    let Some(ert_type) = order.announce else {
        return;
    };
    let station = crate::events::station_name(world);
    if spawned == 0 {
        crate::announce::Priority::new(
            format!("{station}, уведомляем вас, что развёртывание ОБР в данный момент невозможно."),
            "ОБР недоступен",
        )
        .send(world);
        return;
    }
    let (adjective, code) = match ert_type {
        "Amber" => ("стандартный", "ЭМБЕР"),
        "Red" => ("усиленный", "РЭД"),
        "Gamma" => ("элитный", "ГАММА"),
        _ => ("инквизиторский", "ГАММА"),
    };
    crate::announce::Priority::new(
        format!(
            "Внимание, {station}. Мы направляем {adjective} отряд быстрого реагирования кода \
             «{code}». Ожидайте."
        ),
        "ОБР в пути",
    )
    .send(world);
}

fn spawn_points(world: &World) -> Vec<TilePos> {
    world
        .get_resource::<crate::server::SpawnPoints>()
        .map(|s| {
            if s.ert.is_empty() {
                s.latejoin.clone()
            } else {
                s.ert.clone()
            }
        })
        .unwrap_or_default()
}

/// The role of the agent spawned with `numagents` left to place (`roles[WRAP(numagents, 1,
/// length + 1)]`).
fn role_for(roles: &[&'static Role], numagents: u32) -> Option<&'static Role> {
    if roles.is_empty() || numagents == 0 {
        return None;
    }
    Some(roles[((numagents - 1) as usize) % roles.len()])
}

/// The rest of `make_emergency_response_team`; how many spawned.
fn spawn_team(world: &mut World, order: &ErtOrder, mut candidates: Vec<Candidate>) -> u32 {
    if candidates.is_empty() {
        info!("emergency response team: no candidates");
        return 0;
    }
    let spawns = spawn_points(world);
    if spawns.is_empty() {
        warn!("emergency response team: no spawn points");
        return 0;
    }
    let team = {
        let mut state = world.resource_mut::<ErtState>();
        state.teams.push(ErtTeam {
            name: order
                .team_name
                .unwrap_or("Отряд быстрого реагирования")
                .to_owned(),
            mission: order.mission.clone(),
        });
        state.teams.len() - 1
    };
    let mut numagents = order.team_size.min(candidates.len() as u32);
    let mut leader = order.leader;
    let mut index = 0usize;
    let mut spawned = 0;
    while numagents > 0 {
        let Some(chosen) = ghost_poll::pick_candidate(world, &mut candidates) else {
            break;
        };
        // The first picked takes the leader's slot; the earmarked one in tg is picked first too.
        let (role, is_leader) = match leader.take() {
            Some(role) => (role, true),
            None => {
                let Some(role) = role_for(&order.roles, numagents) else {
                    break;
                };
                (role, false)
            }
        };
        let pos = spawns[index % spawns.len()];
        index += 1;
        spawn_member(world, chosen, order, role, is_leader, team, pos);
        spawned += 1;
        numagents -= 1;
    }
    spawned
}

fn spawn_member(
    world: &mut World,
    chosen: Candidate,
    order: &ErtOrder,
    role: &'static Role,
    leader: bool,
    team: usize,
    pos: TilePos,
) {
    let client = chosen.client;
    let name = if order.random_names {
        format!("{} {}", role.title, pick_surname(world))
    } else {
        crate::jobs::character_name(world, client)
    };
    let mob = crate::server::spawn_mob(world, client, pos, name.clone());
    if order.enforce_human {
        crate::species::set_species(world, mob, SpeciesId::Human);
    }
    noncrew_jobs::set_assigned_role(world, mob, role.job);
    world.entity_mut(mob).insert(ErtMember {
        team,
        role: role.path,
        leader,
    });
    equip(world, mob, role, &name);
    greet(world, client, role, leader, &order.mission);
}

fn pick_surname(world: &mut World) -> String {
    let names_len = world.resource::<ErtData>().last_names.len();
    if names_len == 0 {
        return "Иванов".to_owned();
    }
    let i = {
        let mut rng = world.get_resource_or_init::<CombatRng>();
        rng.range(0, (names_len - 1) as u32) as usize
    };
    world.resource::<ErtData>().last_names[i].clone()
}

/// `equipERT` and `/datum/outfit/centcom/ert/post_equip`.
fn equip(world: &mut World, mob: Entity, role: &'static Role, name: &str) {
    let Some(outfit) = world
        .resource::<ErtData>()
        .outfits
        .iter()
        .find(|o| o.path == role.outfit)
        .cloned()
    else {
        warn!("{}: no outfit data (run ss13-mapload convert)", role.outfit);
        return;
    };
    let mut inventory = world.resource_scope(|world, data: Mut<ErtData>| {
        crate::outfit::equip_outfit(world, mob, &outfit, &data.templates)
    });
    if let Some(card) = inventory.id {
        let access = team_access(role.job);
        let label = crate::outfit::id_label(name, role.title);
        let mut card = world.entity_mut(card);
        card.insert((
            IdCard { access },
            IdRegistration {
                registered_name: name.to_owned(),
                assignment: role.title.to_owned(),
                job: role.title.to_owned(),
            },
        ));
        if let Some(mut n) = card.get_mut::<DmName>() {
            n.name = label;
        }
    }
    if let Some(ears) = inventory.ears
        && let Some(mut radio) = world.get_mut::<crate::radio::Radio>(ears)
    {
        radio.frequency = FREQ_CENTCOM;
    }
    inventory.back = storage::spawn_backpack(world, mob);
    world.entity_mut(mob).insert(inventory.clone());
    crate::modpc::pda::post_equip(world, mob, name, role.title);
    if let Some(pack) = inventory.back {
        for (path, count) in &outfit.backpack_contents {
            for _ in 0..*count {
                let item = world.resource_scope(|world, data: Mut<ErtData>| {
                    Some(data.templates.get(path)?.spawn(world, None))
                });
                let Some(item) = item else {
                    continue;
                };
                storage::finish_item(world, item, 1);
                if storage::can_insert(world, pack, item).is_err() {
                    world.despawn(item);
                    continue;
                }
                world.entity_mut(item).insert(Networked);
                storage::put_inside(world, item, pack);
            }
        }
    }
    // `/datum/outfit/centcom/post_equip`: every member is mindshielded.
    let id = crate::implants::next_id(world);
    if let Some(implant) = crate::implants::Implant::new(crate::implants::MINDSHIELD, id) {
        let _ = crate::implants::implant(world, mob, None, implant);
    }
}

/// `/datum/id_trim/centcom/ert/*`'s `New`: the station's access, `ACCESS_CHANGE_IDS` for the
/// commander, the CentCom accesses by role.
fn team_access(job: &str) -> Vec<String> {
    let mut access: Vec<String> = id_access::all_station_access()
        .into_iter()
        .filter(|a| *a != "change_ids" || job == JOB_COMMANDER)
        .map(str::to_owned)
        .collect();
    access.extend(["cent_general".to_owned(), "cent_specops".to_owned()]);
    if job == JOB_COMMANDER {
        access.extend(
            ["cent_living", "cent_storage", "cent_medical", "cent_bar"]
                .into_iter()
                .map(str::to_owned),
        );
    } else {
        access.push("cent_living".to_owned());
    }
    access
}

/// `/datum/antagonist/ert/greet`.
fn greet(world: &mut World, client: Entity, role: &Role, leader: bool, mission: &str) {
    let station = crate::events::station_name(world);
    tell(
        world,
        client,
        Span::BoldWarning,
        format!("Вы - {}.", role.name),
    );
    let mut text = format!(
        "Ваш отряд был отправлен с миссией на {station} департаментом защиты активов Нанотрейзен."
    );
    if leader {
        text.push_str(
            " Возглавьте свой отряд, чтобы обеспечить выполнение миссии. Отправляйтесь на шаттл, \
             когда ваша команда будет готова.",
        );
    } else {
        text.push_str(" Следуйте приказам, отданным вашим лидером отряда.");
    }
    if !role.path.contains("deathsquad") {
        text.push_str(" Избегайте жертв среди гражданских, если это возможно.");
    }
    text.push_str(&format!(" Ваша задача: {mission}"));
    tell(world, client, Span::Warning, text);
}

/// `ErtManager`'s `denyErt` announcement.
pub fn deny(world: &mut World, reason: &str) {
    world.resource_mut::<ErtState>().request_answered = true;
    let station = crate::events::station_name(world);
    let mut text =
        format!("{station}, уведомляем вас, что развёртывание ОБР в данный момент невозможно.");
    if !reason.is_empty() {
        text.push_str(&format!(
            " Ваш запрос ОБР был отклонён по данной причине: {reason}"
        ));
    }
    crate::announce::Priority::new(text, "ОБР недоступен").send(world);
}

/// `ErtManager`'s `dispatchErt` before the poll: the first announcement.
pub fn announce_authorized(world: &mut World) {
    let station = crate::events::station_name(world);
    crate::announce::Priority::new(
        format!(
            "Внимание, {station}. Ваш запрос ОБР санкционирован. Идёт проверка доступности \
             ресурсов. Результат будет объявлен в течении нескольких минут."
        ),
        "Активирован протокол ОБР",
    )
    .send(world);
}

/// `GLOB.ert_request_messages`, newest first (`Insert(1, ...)`).
pub fn record_request(world: &mut World, text: String) {
    if let Some(mut state) = world.get_resource_mut::<ErtState>() {
        state.requests.insert(0, text);
        state.requests.truncate(MAX_TEXT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_role_and_template_resolves() {
        for template in TEMPLATES {
            assert!(role_of(template.leader).is_some(), "{}", template.path);
            for path in template.roles {
                assert!(role_of(path).is_some(), "{path}");
            }
        }
        // Each outfit is in the converter's list (the role table and `ERT_OUTFITS` stay in step).
        for role in ROLES {
            assert!(
                ss13_mapload::antag::ERT_OUTFITS.contains(&role.outfit),
                "{}: {}",
                role.path,
                role.outfit
            );
        }
    }

    #[test]
    fn slots_become_roles_one_category_per_pass() {
        let slots = ErtSlots {
            security: 2,
            medical: 1,
            engineering: 0,
            janitor: 0,
            chaplain: 0,
            clown: 0,
        };
        let roles = slots_to_roles("Red", &slots);
        let paths: Vec<_> = roles.iter().map(|r| r.path).collect();
        assert_eq!(
            paths,
            [
                "/datum/antagonist/ert/security/red",
                "/datum/antagonist/ert/medic/red",
                "/datum/antagonist/ert/security/red",
            ]
        );
        // The inquisition has no clowns.
        let slots = ErtSlots {
            clown: 3,
            chaplain: 1,
            ..ErtSlots::default()
        };
        let paths: Vec<_> = slots_to_roles("Inquisition", &slots)
            .iter()
            .map(|r| r.path)
            .collect();
        assert_eq!(paths, ["/datum/antagonist/ert/chaplain/inquisitor"]);
        assert!(slots_to_roles("Bogus", &slots).is_empty());
    }

    #[test]
    fn roles_wrap_like_tg() {
        let roles: Vec<&'static Role> = COMMANDER_ROLES
            .iter()
            .map(|p| role_of(p).unwrap())
            .collect();
        // `WRAP(numagents, 1, 4)`: 3, 2, 1, 3, ...
        assert_eq!(role_for(&roles, 3).unwrap().path, COMMANDER_ROLES[2]);
        assert_eq!(role_for(&roles, 1).unwrap().path, COMMANDER_ROLES[0]);
        assert_eq!(role_for(&roles, 4).unwrap().path, COMMANDER_ROLES[0]);
        assert!(role_for(&[], 1).is_none());
    }

    #[test]
    fn orders_are_validated() {
        let mut world = World::new();
        let request = ErtSummon {
            template: "/datum/ert/bogus".into(),
            team_size: 5,
            mission: String::new(),
            poll_desc: String::new(),
            enforce_human: true,
            random_names: true,
        };
        assert!(order_from_summon(&mut world, &request, String::new(), String::new()).is_err());
        assert!(order_from_slots(&mut world, "Bogus", true, &ErtSlots::default()).is_err());
        assert!(order_from_slots(&mut world, "Red", false, &ErtSlots::default()).is_err());
    }
}
