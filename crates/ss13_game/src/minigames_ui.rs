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
