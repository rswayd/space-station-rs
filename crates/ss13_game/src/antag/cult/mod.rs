//! The blood cult of Nar'Sie (tg `/datum/antagonist/cult`, `/datum/team/cult`,
//! `code/modules/antagonists/cult/`). [`server`] is the antagonist, its runes and blood magic;
//! [`client`] draws its windows; this module holds what both share.
//!
//! - `AntagInfoGeneric`: tg's default antagonist panel (`AntagInfoGeneric.tsx`): "You are the
//!   Культист!" and the cult's objectives. The cultist datum sets no `ui_name` of its own.
//! - `TextInputModal`: tg `tgui_input_text` (`TextInputModal.tsx`): the Communion message and the
//!   teleport rune's keyword. Acts `submit` (`entry`) and `cancel`.
//! - The rune list, the teleport targets and the spell choices are `ListInputWindow`s.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

use super::BrotherInfo;

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod server;

/// tg `ROLE_CULTIST`: the cultist's `be_special` key.
pub const ROLE_CULTIST: &str = "Cultist";

/// The default antagonist panel.
pub struct AntagInfoGenericUi;

impl UiInterface for AntagInfoGenericUi {
    const NAME: &'static str = "AntagInfoGeneric";
    type Data = ();
    /// `antag_name` and `objectives`: the same fields as the brother's panel.
    type StaticData = BrotherInfo;
}

/// `tgui_input_text`.
pub struct TextInputUi;

impl UiInterface for TextInputUi {
    const NAME: &'static str = "TextInputModal";
    type Data = TextInputData;
    type StaticData = ();
}

/// `/datum/tgui_input_text/ui_data` + `ui_static_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextInputData {
    pub title: String,
    pub message: String,
    pub placeholder: String,
    pub max_length: u32,
}

/// `act('submit', { entry })`.
pub const SUBMIT: &str = "submit";
/// `act('cancel')`.
pub const CANCEL: &str = "cancel";
