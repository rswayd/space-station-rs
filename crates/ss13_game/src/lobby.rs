//! The lobby's data, shared by both halves: tg's title screen with its lobby buttons
//! (`/mob/dead/new_player`, BandaStation `title_screen`) as a headless tgui interface
//! ([`LobbyUi`], kept open by the server while the client is a new player), and the jobs page of
//! tg's preferences menu (`PreferencesMenu/CharacterPreferences/JobsPage.tsx`) as a window
//! ([`JobPrefsUi`]) with an antagonist toggle (tg's antagonists page). The server half is `crate::round`, the client half `crate::lobby_client`.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::job_selection::DepartmentStatic;

/// tg `GAME_STATE_*`.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum GameState {
    #[default]
    Startup,
    /// The lobby countdown.
    Pregame,
    SettingUp,
    Playing,
    Finished,
}

/// The lobby screen of one new player (no window: `crate::lobby_client` draws it over the map).
pub struct LobbyUi;

impl UiInterface for LobbyUi {
    const NAME: &'static str = "Lobby";
    type Data = LobbyData;
    type StaticData = ();
}

/// What BandaStation's title screen shows (`title_screen_html.dm`, `SStitle.update_info`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LobbyData {
    pub state: GameState,
    /// Whole seconds until the round starts, while the countdown runs.
    pub time_left: Option<u32>,
    /// tg `ready == PLAYER_READY_TO_PLAY`.
    pub ready: bool,
    /// The character's name (tg `prefs.read_preference(/datum/preference/name/real_name)`).
    pub name: String,
    /// Connected clients.
    pub players: u32,
    /// New players who are ready (tg `totalPlayersReady`).
    pub ready_players: u32,
    /// Clients still in the lobby (tg `GLOB.new_player_list`).
    pub in_lobby: u32,
    /// BandaStation `/datum/preference/toggle/antag_roll`: may be picked for antagonist roles.
    pub antag_roll: bool,
    /// `SS13_SOURCE_URL` (`crate::round::RoundConfig::source_url`): the AGPL source offer.
    pub source_url: Option<String>,
}

/// Lobby acts (BandaStation `new_player/Topic` hrefs). "Join game" is
/// `ClientIntent::LateJoinMenu`.
pub const TOGGLE_READY: &str = "toggle_ready";
pub const OBSERVE: &str = "observe";
/// Opens the job preferences window.
pub const CHARACTER_SETUP: &str = "character_setup";
/// BandaStation's lobby "Антагонисты" checkbox (`toggle_antag_roll` href).
pub const TOGGLE_ANTAG_ROLL: &str = "toggle_antag_roll";

/// tg `JP_LOW`, `JP_MEDIUM`, `JP_HIGH`; a job without a level is "never".
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum JobPriority {
    Low = 1,
    Medium = 2,
    High = 3,
}

impl JobPriority {
    /// The `level` act param as tgui sends it (`JobPriority` numbers): `Some(None)` for
    /// "never", `None` for anything invalid.
    pub fn from_param(level: &str) -> Option<Option<Self>> {
        match level {
            "" | "null" => Some(None),
            "1" => Some(Some(Self::Low)),
            "2" => Some(Some(Self::Medium)),
            "3" => Some(Some(Self::High)),
            _ => None,
        }
    }

    pub fn param(level: Option<Self>) -> String {
        level.map_or_else(String::new, |l| (l as u8).to_string())
    }
}

/// tg `/datum/preference/choiced/jobless_role`: what happens to a ready player no wanted job
/// had room for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JoblessRole {
    /// `BEOVERFLOW`, the default.
    #[default]
    BeOverflow = 1,
    BeRandomJob = 2,
    ReturnToLobby = 3,
}

impl JoblessRole {
    pub const ALL: [Self; 3] = [Self::BeOverflow, Self::BeRandomJob, Self::ReturnToLobby];

    pub fn from_param(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|r| (*r as u8).to_string() == value)
    }
}

/// The jobs page of the preferences menu.
pub struct JobPrefsUi;

impl UiInterface for JobPrefsUi {
    const NAME: &'static str = "JobPreferences";
    type Data = JobPrefsData;
    type StaticData = JobPrefsStatic;
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct JobPrefsStatic {
    pub departments: Vec<DepartmentStatic>,
    /// The overflow role's title: an on/off toggle instead of levels.
    pub overflow: String,
    /// Titles that cannot be played here (no human outfit): shown without buttons.
    pub unavailable: Vec<String>,
    /// The appearance section's choices, in table order (`crate::human`): skin tone names and
    /// colors, hair style names, hair and eye colors.
    pub skin_tones: Vec<(String, [u8; 3])>,
    pub hair_styles: Vec<String>,
    pub hair_colors: Vec<[u8; 3]>,
    pub eye_colors: Vec<[u8; 3]>,
    /// tg `quirk_info`, in menu order (`crate::quirks::QUIRKS`).
    pub quirks: Vec<QuirkInfo>,
    /// tg `quirk_blacklist`: names that exclude each other.
    pub quirk_blacklist: Vec<Vec<String>>,
    /// tg `default_quirk_balance`, `max_positive_quirks`.
    pub default_quirk_balance: i32,
    pub max_positive_quirks: u8,
    /// The selectable species' names (`crate::human::roundstart_species`) and the mutant colors
    /// of species with `TRAIT_MUTANT_COLORS`.
    pub species: Vec<String>,
    pub mutant_colors: Vec<[u8; 3]>,
}

/// One quirk of the menu (tg `quirk_info` entry).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuirkInfo {
    pub name: String,
    pub description: String,
    /// A FontAwesome icon name.
    pub icon: String,
    /// Positive: costs points; negative: gives them.
    pub value: i32,
}

/// The character's appearance: indexes into [`JobPrefsStatic`]'s lists (`hair_style` `None` =
/// bald).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppearanceData {
    pub species: u8,
    pub female: bool,
    pub skin_tone: u8,
    pub hair_style: Option<u8>,
    pub hair_color: u8,
    pub eye_color: u8,
    pub mutant_color: u8,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobPrefsData {
    /// tg `job_preferences`: titles with a level, the rest are "never".
    pub levels: Vec<(String, JobPriority)>,
    pub jobless_role: JoblessRole,
    /// tg `be_special`: the antagonist roles the player wants (`ROLE_TRAITOR`, ...).
    pub be_special: Vec<String>,
    pub appearance: AppearanceData,
    /// tg `selected_quirks`: the chosen quirks' names.
    pub quirks: Vec<String>,
    /// The points left: `default_quirk_balance` minus the chosen quirks' values.
    pub quirk_balance: i32,
}

impl JobPrefsData {
    pub fn level(&self, title: &str) -> Option<JobPriority> {
        self.levels
            .iter()
            .find(|(t, _)| t == title)
            .map(|(_, l)| *l)
    }
}

/// `act('set_job_preference', { job, level })`; `level` is empty for "never".
pub const SET_JOB_PREFERENCE: &str = "set_job_preference";
/// `act('set_preference', { preference: 'joblessrole', value })`.
pub const SET_PREFERENCE: &str = "set_preference";
pub const JOBLESS_ROLE: &str = "joblessrole";
/// tg `preference_middleware/antags` `set_antags`: `{ antags, toggled }` (`toggled` "1"/"0").
pub const SET_ANTAGS: &str = "set_antags";

/// tg `preference_middleware/quirks` `give_quirk`/`remove_quirk`: `{ quirk }` (a quirk name).
pub const GIVE_QUIRK: &str = "give_quirk";
pub const REMOVE_QUIRK: &str = "remove_quirk";

/// `act('set_appearance', { key, value })`: `key` one of the `APPEARANCE_*` keys, `value` an index
/// (`female`: "1"/"0"; `hair_style`: empty for bald).
pub const SET_APPEARANCE: &str = "set_appearance";
/// tg "Randomize": a random appearance from the tables.
pub const RANDOMIZE_APPEARANCE: &str = "randomize_appearance";
pub const APPEARANCE_FEMALE: &str = "female";
pub const APPEARANCE_SKIN_TONE: &str = "skin_tone";
pub const APPEARANCE_HAIR_STYLE: &str = "hair_style";
pub const APPEARANCE_HAIR_COLOR: &str = "hair_color";
pub const APPEARANCE_EYE_COLOR: &str = "eye_color";
pub const APPEARANCE_SPECIES: &str = "species";
pub const APPEARANCE_MUTANT_COLOR: &str = "mutant_color";
