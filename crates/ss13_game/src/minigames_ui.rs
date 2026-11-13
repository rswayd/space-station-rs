//! The ghost minigame windows, shared by the server (`crate::minigames`) and the client
//! (`crate::minigames_client`): the minigames menu ([`MinigamesMenuUi`], tg
//! `/datum/minigames_menu`) and the basketball lobby ([`BasketballPanelUi`], tg
//! `/datum/basketball_controller` `BasketballPanel`).

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

/// tgui `MinigamesMenu`.
pub struct MinigamesMenuUi;

impl UiInterface for MinigamesMenuUi {
    const NAME: &'static str = "MinigamesMenu";
    type Data = ();
    type StaticData = ();
}

/// `act('basketball')`: the menu closes and the basketball lobby opens.
pub const MENU_BASKETBALL: &str = "basketball";

/// tgui `BasketballPanel`.
pub struct BasketballPanelUi;

impl UiInterface for BasketballPanelUi {
    const NAME: &'static str = "BasketballPanel";
    type Data = BasketballPanelData;
    type StaticData = ();
}

/// One signed-up player (`lobbydata` entry).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LobbyMember {
    pub ckey: String,
    /// Disconnected (`basketball_bad_signup`): kept in the lobby, not counted for a start.
    pub disconnected: bool,
}

/// `/datum/basketball_controller/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BasketballPanelData {
    pub total_votes: u32,
    pub players_min: u32,
    pub players_max: u32,
    pub lobbydata: Vec<LobbyMember>,
}

/// `act('basketball_signup')`: signs up, or out when already signed up.
pub const BASKETBALL_SIGNUP: &str = "basketball_signup";
/// `act('basketball_start')`.
pub const BASKETBALL_START: &str = "basketball_start";

/// `act('deathmatch')`: the menu closes and the deathmatch panel opens.
pub const MENU_DEATHMATCH: &str = "deathmatch";

/// tgui `DeathmatchPanel` (`/datum/deathmatch_controller`).
pub struct DeathmatchPanelUi;

impl UiInterface for DeathmatchPanelUi {
    const NAME: &'static str = "DeathmatchPanel";
    type Data = DeathmatchPanelData;
    type StaticData = ();
}

/// One open lobby, named by its host's key.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeathmatchLobbyRow {
    pub name: String,
    pub players: u32,
    pub max_players: u32,
    pub map: String,
    /// `DEATHMATCH_NOT_PLAYING` (0), `PRE_PLAYING` (1), `PLAYING` (2).
    pub playing: u8,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeathmatchPanelData {
    pub lobbies: Vec<DeathmatchLobbyRow>,
    pub hosting: bool,
    /// The host key of the lobby the viewer is in.
    pub playing: Option<String>,
}

/// `act('host')`, `act('join', {id})`, `act('spectate', {id})`.
pub const DM_HOST: &str = "host";
pub const DM_JOIN: &str = "join";
pub const DM_SPECTATE: &str = "spectate";

/// tgui `DeathmatchLobby` (`/datum/deathmatch_lobby`).
pub struct DeathmatchLobbyUi;

impl UiInterface for DeathmatchLobbyUi {
    const NAME: &'static str = "DeathmatchLobby";
    type Data = DeathmatchLobbyData;
    type StaticData = DeathmatchLobbyStatic;
}

/// `ui_static_data`: the map names.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeathmatchLobbyStatic {
    pub maps: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeathmatchMapInfo {
    pub name: String,
    pub desc: String,
    /// Seconds.
    pub time: u32,
    pub min_players: u32,
    pub max_players: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeathmatchLobbyPlayer {
    pub key: String,
    pub host: bool,
    pub ready: bool,
    pub loadout: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeathmatchLobbyObserver {
    pub key: String,
    pub host: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeathmatchLobbyData {
    #[serde(rename = "self")]
    pub self_key: String,
    pub host: bool,
    pub playing: u8,
    pub map: DeathmatchMapInfo,
    /// "Случайно" first, then the map's loadouts.
    pub loadouts: Vec<String>,
    pub loadoutdesc: String,
    pub players: Vec<DeathmatchLobbyPlayer>,
    pub observers: Vec<DeathmatchLobbyObserver>,
}

pub const DM_START: &str = "start_game";
pub const DM_LEAVE: &str = "leave_game";
/// `{player, loadout}`.
pub const DM_CHANGE_LOADOUT: &str = "change_loadout";
pub const DM_OBSERVE: &str = "observe";
pub const DM_READY: &str = "ready";
/// `act('host', {func, id, map})`; `func` is one of the `DM_FUNC_*`.
pub const DM_HOST_FUNC: &str = "host";
pub const DM_FUNC_KICK: &str = "Kick";
pub const DM_FUNC_TRANSFER: &str = "Transfer host";
pub const DM_FUNC_OBSERVE: &str = "Toggle observe";
pub const DM_FUNC_MAP: &str = "change_map";
