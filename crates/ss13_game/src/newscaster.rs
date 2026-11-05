//! The newscaster window's data, shared by both halves: tgui `PhysicalNewscaster`
//! (`tgui/packages/tgui/interfaces/Newscaster.jsx`): the channel list with the wanted issue,
//! the selected channel's stories with their comments, and the wanted issue screen. The server
//! half is `crate::newscaster_server`, the client view `crate::newscaster_client`.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

pub struct NewscasterUi;

impl UiInterface for NewscasterUi {
    const NAME: &'static str = "PhysicalNewscaster";
    type Data = NewscasterData;
    type StaticData = ();
}

/// tg `ui_data` `user`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewsUser {
    pub name: String,
    pub job: String,
    pub department: String,
}

/// One entry of `ui_static_data` `channels`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewsChannel {
    pub id: u32,
    pub name: String,
    pub author: String,
    pub censored: bool,
    pub locked: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewsComment {
    pub auth: String,
    pub body: String,
    pub time: String,
}

/// One story of the viewed channel (`messages`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewsMessage {
    pub id: u32,
    pub auth: String,
    pub body: String,
    pub time: String,
    pub censored_message: bool,
    pub censored_author: bool,
    pub comments: Vec<NewsComment>,
}

/// The active wanted issue (`wanted`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewsWanted {
    pub criminal: String,
    pub crime: String,
    pub author: String,
}

/// `ui_data` + `ui_static_data` of `/obj/machinery/newscaster`, the fields the ported screens
/// read.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewscasterData {
    pub user: NewsUser,
    /// The user's ID has `ACCESS_LIBRARY` (D-notices) / `ACCESS_SECURITY` (wanted issues).
    pub admin_mode: bool,
    pub security_mode: bool,
    pub channels: Vec<NewsChannel>,
    pub viewing_channel: Option<u32>,
    pub viewing_wanted: bool,
    pub channel_name: String,
    pub channel_author: String,
    pub channel_desc: String,
    pub channel_locked: bool,
    pub channel_censored: bool,
    pub messages: Vec<NewsMessage>,
    pub wanted: Option<NewsWanted>,
    /// The wanted issue being written on this newscaster (`criminal_name`, `crime_description`).
    pub criminal_name: String,
    pub crime_description: String,
    pub paper: u32,
}

impl NewscasterData {
    /// The "Submit Story" button's `disabled`: a locked channel takes stories from its author
    /// only, a censored one from nobody.
    pub fn can_submit(&self) -> bool {
        self.viewing_channel.is_some()
            && !(self.channel_locked && self.channel_author != self.user.name)
            && !self.channel_censored
    }

    /// The comment button's `disabled`.
    pub fn can_comment(&self, message: &NewsMessage) -> bool {
        !message.censored_author
            && !message.censored_message
            && self.user.name != "Unknown"
            && !(self.channel_locked && self.channel_author != self.user.name)
    }
}

pub const SET_CHANNEL: &str = "setChannel";
pub const CREATE_STORY: &str = "createStory";
pub const START_CREATE_CHANNEL: &str = "startCreateChannel";
pub const CANCEL_CREATION: &str = "cancelCreation";
pub const STORY_CENSOR: &str = "storyCensor";
pub const AUTHOR_CENSOR: &str = "authorCensor";
pub const CHANNEL_D_NOTICE: &str = "channelDNotice";
pub const START_COMMENT: &str = "startComment";
pub const TOGGLE_WANTED: &str = "toggleWanted";
pub const SET_CRIMINAL_NAME: &str = "setCriminalName";
pub const SET_CRIME_DATA: &str = "setCrimeData";
pub const SUBMIT_WANTED_ISSUE: &str = "submitWantedIssue";
pub const CLEAR_WANTED_ISSUE: &str = "clearWantedIssue";
pub const PRINT_NEWSPAPER: &str = "printNewspaper";
/// Params.
pub const CHANNEL: &str = "channel";
pub const MESSAGE_ID: &str = "messageID";

/// `CENSOR_MESSAGE` of `Newscaster.jsx`.
pub const CENSOR_MESSAGE: &str = "This channel has been deemed as threatening to the welfare of \
     the station, and marked with a Nanotrasen D-Notice.";
